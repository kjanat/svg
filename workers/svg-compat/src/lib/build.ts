/**
 * Assembly logic that turns loaded BCD + web-features payloads into
 * the final `SvgCompatOutput`. Walks `bcd.svg.elements` and
 * `bcd.svg.global_attributes`, merges per-element attribute compat
 * across elements, and produces a sorted record keyed by name.
 *
 * Pure data — no Deno HTTP plumbing, no preact. Safe to call from
 * both the worker server and the CLI.
 *
 * @module
 */

import { getCompat, getRecordProperty, makeCompatEntry, selectBrowserStatement } from '#lib/parse.ts';
import type { AttributeEntry, Baseline, BrowserSupport, BrowserVersion, CompatEntry, SvgCompatOutput, SvgCompatSnapshot } from '#lib/types.ts';
import type { JsonRecord, LoadedSourceData } from '#src/sources.ts';
import { isRecord, UpstreamSourceError } from '#src/sources.ts';

/**
 * BCD uses underscore-delimited namespace names (`xlink_href`,
 * `xml_lang`); SVG uses colon-delimited (`xlink:href`, `xml:lang`).
 * Canonicalise here. Regex subsumes the old hand-written XLINK_MAP
 * and auto-covers any future `xml_*` / `xlink_*` additions upstream.
 */
const NAMESPACE_UNDERSCORE = /^(xlink|xml)_(\w+)$/;

interface DocsFallback {
	mdn_url?: string;
	spec_url: string[];
}

/**
 * Known upstream gaps where BCD omits docs links entirely.
 * Keep this narrowly scoped and data-backed.
 */
const ATTRIBUTE_DOCS_FALLBACKS: Record<string, DocsFallback> = {
	path: {
		mdn_url: 'https://developer.mozilla.org/en-US/docs/Web/SVG/Reference/Attribute/path',
		spec_url: [
			'https://svgwg.org/svg2-draft/text.html#TextPathElementPathAttribute',
			'https://svgwg.org/specs/animations/#AnimateMotionElementPathAttribute',
		],
	},
};

function canonicalAttributeName(name: string): string {
	const match = name.match(NAMESPACE_UNDERSCORE);
	return match ? `${match[1]}:${match[2]}` : name;
}

function baselineRank(baseline: Baseline): number {
	if (baseline.status === 'limited') return 0;
	if (baseline.status === 'newly') return 1;
	return baseline.status === 'widely' ? 2 : 3;
}

function baselineMilestone(baseline: Baseline): string {
	if (baseline.status === 'widely') return baseline.high_date?.date ?? '';
	if (baseline.status === 'newly') return baseline.low_date?.date ?? '';
	return '';
}

function parseVersionParts(version: string): number[] | undefined {
	const literal = version.match(/^(?:≤|≥|<=|>=|<|>|~|≈)?(\d+(?:\.\d+)*)$/)?.[1];
	if (literal === undefined) return undefined;
	const parts = literal.split('.').map(Number);
	if (parts.some(Number.isNaN)) return undefined;
	return parts;
}

/**
 * Compares two parsed version strings (e.g. `"50"` vs `"50.1"`).
 * Returns negative if `left < right`, positive if `left > right`.
 * Unparseable pairs fall back to `0` — merge will keep existing.
 */
function compareVersionStrings(left: string, right: string): number {
	const parsedLeft = parseVersionParts(left);
	const parsedRight = parseVersionParts(right);
	if (!parsedLeft || !parsedRight) return 0;
	const maxLength = Math.max(parsedLeft.length, parsedRight.length);
	for (let index = 0; index < maxLength; index++) {
		const leftPart = parsedLeft[index] ?? 0;
		const rightPart = parsedRight[index] ?? 0;
		if (leftPart !== rightPart) return leftPart - rightPart;
	}
	return 0;
}

/**
 * Rank for cross-element merging. Higher = more restrictive = wins.
 * Rationale: an attribute shared across elements surfaces the
 * tightest support envelope. `false` (explicitly unsupported here)
 * trumps any concrete version; a concrete version trumps unknown support.
 *
 * Two BrowserVersions with concrete string versions fall through to
 * a numeric compare on `version_added`.
 */
function browserVersionRank(version: BrowserVersion): number {
	const raw = version.version_added;
	if (raw === false) return 4;
	if (typeof raw === 'string') return 3;
	if (raw === undefined) return 1;
	return 0;
}

function mergeBrowserVersion(
	existing: BrowserVersion | undefined,
	incoming: BrowserVersion | undefined,
): BrowserVersion | undefined {
	if (incoming === undefined) return existing;
	if (existing === undefined) return incoming;
	const existingRank = browserVersionRank(existing);
	const incomingRank = browserVersionRank(incoming);
	if (incomingRank > existingRank) return incoming;
	if (incomingRank < existingRank) return existing;
	// Same rank. For concrete versions, compare numerically.
	if (
		typeof existing.version_added === 'string'
		&& typeof incoming.version_added === 'string'
	) {
		return compareVersionStrings(incoming.version_added, existing.version_added) > 0
			? incoming
			: existing;
	}
	return existing;
}

function mergeBrowserSupport(existing: BrowserSupport | undefined, incoming: BrowserSupport): BrowserSupport {
	const result = { ...existing };
	for (const [id, history] of Object.entries(incoming)) {
		// An aggregate is a summary. Keep the complete history of the more restrictive context;
		// every context's own history remains available in attribute.contexts.
		const previous = selectBrowserStatement(result[id]);
		const next = selectBrowserStatement(history);
		if (!result[id] || mergeBrowserVersion(previous, next) === next) result[id] = history;
	}
	return result;
}

/**
 * Merges an attribute compat entry from one element into the global
 * attribute map. Consensus merge: deprecation/experimental only
 * become true when every observed element agrees.
 */
function mergeAttributeEntry(
	attributes: Map<string, AttributeEntry>,
	attributeName: string,
	elementName: string,
	compat: CompatEntry,
	compatKey: string,
): void {
	const attribute = attributes.get(attributeName);
	if (!attribute) {
		attributes.set(attributeName, {
			aggregate: structuredClone(compat),
			contexts: { [compatKey]: compat },
			elements: [elementName],
			aggregation: 'observed-contexts',
			coverage: { contexts: 0, baseline_known: 0, baseline_unknown: 0, baseline_missing: 0 },
		});
		return;
	}

	attribute.contexts[compatKey] = compat;
	const existing = attribute.aggregate;
	existing.deprecated = existing.deprecated && compat.deprecated;
	existing.experimental = existing.experimental && compat.experimental;
	if (!compat.standard_track) existing.standard_track = false;
	if (!existing.description && compat.description) existing.description = compat.description;
	if (!existing.mdn_url && compat.mdn_url) existing.mdn_url = compat.mdn_url;
	for (const url of compat.spec_url) {
		if (!existing.spec_url.includes(url)) existing.spec_url.push(url);
	}
	if (!attribute.elements.includes('*') && !attribute.elements.includes(elementName)) {
		attribute.elements.push(elementName);
	}

	if (!existing.baseline) {
		existing.baseline = compat.baseline;
	} else if (compat.baseline) {
		const existingRank = baselineRank(existing.baseline);
		const incomingRank = baselineRank(compat.baseline);
		if (
			incomingRank < existingRank
			|| (incomingRank === existingRank
				&& baselineMilestone(compat.baseline) > baselineMilestone(existing.baseline))
		) {
			existing.baseline = compat.baseline;
		}
	}

	for (const advice of compat.discouraged ?? []) {
		existing.discouraged ??= [];
		if (!existing.discouraged.some(item => item.feature_id === advice.feature_id && item.compat_key === advice.compat_key)) {
			existing.discouraged.push(advice);
		}
	}
	if (compat.browser_support) {
		existing.browser_support = mergeBrowserSupport(
			existing.browser_support,
			compat.browser_support,
		);
	}
}

function applyAttributeDocsFallback(attributeName: string, entry: CompatEntry): void {
	const fallback = ATTRIBUTE_DOCS_FALLBACKS[attributeName];
	if (!fallback) return;
	if (!entry.mdn_url && fallback.mdn_url) entry.mdn_url = fallback.mdn_url;
	for (const url of fallback.spec_url) {
		if (!entry.spec_url.includes(url)) entry.spec_url.push(url);
	}
}

/** Walks `bcd.svg.elements`, extracts `__compat` for each, returns sorted record. */
function collectElements(
	svgElements: JsonRecord,
	featureMap: JsonRecord,
): Record<string, CompatEntry> {
	const result: Record<string, CompatEntry> = {};
	const names = Object.keys(svgElements).filter((key) => key !== '__compat').sort();
	for (const name of names) {
		const element = getRecordProperty(svgElements, name);
		if (!element) continue;
		const compat = getCompat(element);
		if (!compat) continue;
		result[name] = makeCompatEntry(compat, featureMap, `svg.elements.${name}`);
	}
	return result;
}

/** Collects global + element-specific attributes from BCD, merges across elements, returns sorted. */
function collectAttributes(
	svgRoot: JsonRecord,
	featureMap: JsonRecord,
): Record<string, AttributeEntry> {
	const attributes = new Map<string, AttributeEntry>();

	const globalAttributes = getRecordProperty(svgRoot, 'global_attributes');
	if (globalAttributes) {
		for (const [name, value] of Object.entries(globalAttributes)) {
			if (name === '__compat') continue;
			if (!isRecord(value)) continue;
			const compat = getCompat(value);
			if (!compat) continue;
			const canonicalName = canonicalAttributeName(name);
			const entry = makeCompatEntry(compat, featureMap, `svg.global_attributes.${name}`);
			mergeAttributeEntry(attributes, canonicalName, '*', entry, `svg.global_attributes.${name}`);
		}
	}

	const elements = getRecordProperty(svgRoot, 'elements');
	if (elements) {
		for (const [elementName, value] of Object.entries(elements)) {
			if (elementName === '__compat') continue;
			if (!isRecord(value)) continue;
			for (const [attributeName, attributeValue] of Object.entries(value)) {
				if (attributeName === '__compat') continue;
				if (!isRecord(attributeValue)) continue;
				const compat = getCompat(attributeValue);
				if (!compat) continue;
				const canonicalName = canonicalAttributeName(attributeName);
				const entry = makeCompatEntry(
					compat,
					featureMap,
					`svg.elements.${elementName}.${attributeName}`,
				);
				mergeAttributeEntry(attributes, canonicalName, elementName, entry, `svg.elements.${elementName}.${attributeName}`);
			}
		}
	}

	for (const [name, entry] of attributes.entries()) {
		applyAttributeDocsFallback(name, entry.aggregate);
		const contexts = Object.values(entry.contexts);
		entry.coverage = {
			contexts: contexts.length,
			baseline_known: contexts.filter(c => c.baseline?.status !== undefined).length,
			baseline_unknown: contexts.filter(c => c.baseline !== undefined && c.baseline.status === undefined).length,
			baseline_missing: contexts.filter(c => c.baseline === undefined).length,
		};
	}

	return Object.fromEntries(
		[...attributes.entries()].sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0)),
	);
}

/** Processes raw loaded source data into a processed snapshot of elements and attributes. */
export function buildSnapshot(data: LoadedSourceData): SvgCompatSnapshot {
	const elements = getRecordProperty(data.svgRoot, 'elements');
	if (!elements) {
		throw new UpstreamSourceError('BCD payload is missing the svg.elements map.');
	}

	return {
		sources: data.sources,
		elements: collectElements(elements, data.featureMap),
		attributes: collectAttributes(data.svgRoot, data.featureMap),
	};
}

/**
 * Wraps a snapshot with a generation timestamp into the final
 * output shape. `generatedAt` is required so the lib has no
 * dependency on `http.ts` / `BOOT` / `DEV` — each caller (server,
 * CLI) provides its own ISO string.
 */
export function buildOutput(
	snapshot: SvgCompatSnapshot,
	generatedAt: string,
): SvgCompatOutput {
	return {
		schema_version: 2,
		generated_at: generatedAt,
		sources: snapshot.sources,
		elements: snapshot.elements,
		attributes: snapshot.attributes,
	};
}
