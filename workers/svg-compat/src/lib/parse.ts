/**
 * Extraction primitives that turn raw upstream JSON (BCD `__compat`
 * + web-features feature map) into typed `CompatEntry` / `Baseline`
 * / `BrowserSupport` objects.
 *
 * Recognized status and dates are independent. Preserve raw input and parsing
 * diagnostics without guessing tiers or the meaning of unknown qualifiers.
 * @module
 */

import type { Baseline, BaselineDate, BrowserFlag, BrowserSupport, BrowserVersion, CompatEntry, Discouraged, VersionQualifier } from '#lib/types.ts';
import type { JsonRecord } from '#src/sources.ts';
import { isRecord } from '#src/sources.ts';

const WEB_FEATURE_KIND_FEATURE = 'feature';

const loggedWarnings = new Set<string>();

/**
 * Stable string representation of an unknown-typed value, used to
 * key `warnOnce` so identical unknowns don't spam the log.
 */
export function stringifyUnknown(value: unknown): string {
	if (typeof value === 'string') return JSON.stringify(value);
	if (typeof value === 'number' || typeof value === 'boolean') return String(value);
	if (value === null) return 'null';
	if (value === undefined) return 'undefined';
	try {
		return JSON.stringify(value);
	} catch {
		return String(value);
	}
}

/** Emits `console.warn(message)` exactly once per distinct `key`. */
export function warnOnce(key: string, message: string): void {
	if (loggedWarnings.has(key)) return;
	loggedWarnings.add(key);
	console.warn(message);
}

/** Test helper — clears the warn-dedupe cache between unit tests. */
export function _resetLoggedWarnings(): void {
	loggedWarnings.clear();
}

export function getString(value: unknown): string | undefined {
	return typeof value === 'string' ? value : undefined;
}

export function getBoolean(value: unknown): boolean | undefined {
	return typeof value === 'boolean' ? value : undefined;
}

export function getStringArray(value: unknown): string[] | undefined {
	if (!Array.isArray(value)) return undefined;
	const strings = value.filter((entry): entry is string => typeof entry === 'string');
	return strings.length === value.length ? strings : undefined;
}

export function getRecord(value: unknown): JsonRecord | undefined {
	return isRecord(value) ? value : undefined;
}

export function getRecordProperty(record: JsonRecord, key: string): JsonRecord | undefined {
	return getRecord(record[key]);
}

export function getCompat(node: JsonRecord): JsonRecord | undefined {
	return getRecordProperty(node, '__compat');
}

/**
 * Lookup table for known web-features baseline date prefixes.
 *
 * Currently the dataset only ships `≤` (14 distinct date strings as of v3.23.0),
 * but the upstream schema declares the field as plain `string` with no pattern,
 * so future versions could ship `≥` / `~` / etc.
 *
 * Extend this table when that happens — `parseBaselineDate` will warn on any
 * unknown prefix until it's added here.
 */
const KNOWN_DATE_PREFIXES: Record<string, BaselineDate['qualifier']> = {
	'≤': 'before',
	'<': 'before',
	'<=': 'before',
	'≥': 'after',
	'>': 'after',
	'>=': 'after',
	'~': 'approximately',
	'≈': 'approximately',
};

/** Parse a full valid calendar date with known qualifiers; retain unknown input. */
export function parseBaselineDate(raw: unknown, compatKey: string): BaselineDate | undefined {
	if (raw === undefined) return undefined;
	if (typeof raw !== 'string') return { raw: stringifyUnknown(raw) };
	const match = raw.match(/^(≤|≥|<=|>=|<|>|~|≈)?(\d{4}-\d{2}-\d{2})$/);
	if (match) {
		const [, prefix, date] = match;
		const parsed = new Date(date);
		if (date.slice(0, 4) !== '0000' && !Number.isNaN(parsed.valueOf()) && parsed.toISOString().slice(0, 10) === date) {
			return { raw, date, qualifier: prefix ? KNOWN_DATE_PREFIXES[prefix] : undefined };
		}
	}
	warnOnce(
		`wf-date:${raw}`,
		`svg-compat: unrecognized baseline date ${stringifyUnknown(raw)} for "${compatKey}"; preserving raw input without a date interpretation.`,
	);
	return { raw };
}

/** Year summaries are derived at presentation time, never stored in the data contract. */
export function yearOfBaselineDate(parsed: BaselineDate): number | undefined {
	return parsed.date ? Number(parsed.date.slice(0, 4)) : undefined;
}

/** Import recognized status independently of the presence or meaning of dates. */
export function parseBaseline(input: JsonRecord, compatKey: string): Baseline | undefined {
	const value = input.baseline;
	const low_date = parseBaselineDate(input.baseline_low_date, compatKey);
	const high_date = parseBaselineDate(input.baseline_high_date, compatKey);
	if (value === undefined && !low_date && !high_date) return undefined;
	const status = value === false ? 'limited' : value === 'low' ? 'newly' : value === 'high' ? 'widely' : undefined;
	const status_diagnostic = status ? undefined : value === undefined ? 'missing' : 'unrecognized';
	if (status_diagnostic) {
		warnOnce(`wf-baseline:${stringifyUnknown(value)}`, `svg-compat: ${status_diagnostic} baseline value for "${compatKey}"; no recognized tier.`);
	}
	return { status, raw_status: value === undefined ? undefined : stringifyUnknown(value), status_diagnostic, low_date, high_date };
}

/** Keep all applicable feature-level discouragement separately from Baseline. */
export function extractDiscouraged(compat: JsonRecord, featureMap: JsonRecord, compatKey: string): Discouraged[] | undefined {
	const ids = new Set(
		(getStringArray(compat.tags) ?? []).filter(tag => tag.startsWith('web-features:')).map(tag => tag.slice('web-features:'.length)),
	);
	const result: Discouraged[] = [];
	for (const [id, feature] of Object.entries(featureMap)) {
		if (!isRecord(feature) || feature.kind !== WEB_FEATURE_KIND_FEATURE) continue;
		if (!ids.has(id) && !getStringArray(feature.compat_features)?.includes(compatKey)) continue;
		const advice = getRecordProperty(feature, 'discouraged');
		if (!advice) continue;
		result.push({
			feature_id: id,
			compat_key: compatKey,
			scope: 'feature',
			feature_name: getString(feature.name),
			reason: getString(advice.reason) ?? '',
			reason_html: getString(advice.reason_html),
			according_to: getStringArray(advice.according_to) ?? [],
			alternatives: getStringArray(advice.alternatives) ?? [],
			removal_date: getString(advice.removal_date),
		});
	}
	return result.length ? result : undefined;
}

/** Resolves baseline from web-features using BCD tags or Web Features compat keys, preferring per-key status. */
export function extractBaseline(
	compat: JsonRecord,
	featureMap: JsonRecord,
	compatKey: string,
): Baseline | undefined {
	const tags = getStringArray(compat.tags) ?? [];

	const featureTag = tags.find((tag) => tag.startsWith('web-features:'));

	const featureId = featureTag?.slice('web-features:'.length)
		?? Object.keys(featureMap).find(id => getStringArray(getRecordProperty(featureMap, id)?.compat_features)?.includes(compatKey));
	if (!featureId) return undefined;
	const feature = getRecordProperty(featureMap, featureId);
	if (!feature) return undefined;
	const featureKind = getString(feature.kind);
	if (featureKind !== WEB_FEATURE_KIND_FEATURE) {
		warnOnce(
			`wf-kind:${featureKind ?? '<missing>'}`,
			`svg-compat: unsupported web-features kind ${stringifyUnknown(featureKind)} for "${featureId}".`,
		);
		return undefined;
	}

	const status = getRecordProperty(feature, 'status');
	if (!status) return undefined;

	const byCompatKey = getRecordProperty(status, 'by_compat_key');
	if (byCompatKey && Object.hasOwn(byCompatKey, compatKey)) {
		const overrideStatus = getRecordProperty(byCompatKey, compatKey);
		return overrideStatus ? parseBaseline(overrideStatus, compatKey) : undefined;
	}

	return parseBaseline(status, compatKey);
}

/**
 * Lookup table for known BCD version-string prefixes. Same set as
 * `KNOWN_DATE_PREFIXES` — version numbers carry the same "at or
 * before / at or after" semantics as baseline dates.
 */
const KNOWN_VERSION_PREFIXES: Record<string, VersionQualifier> = {
	'≤': 'before',
	'<': 'before',
	'<=': 'before',
	'≥': 'after',
	'>': 'after',
	'>=': 'after',
	'~': 'approximately',
};

interface ParsedVersionString {
	version: string;
	qualifier?: VersionQualifier;
}

/**
 * Splits a BCD version string into a clean version + qualifier.
 * Mirrors `parseBaselineDate` on version numbers rather than dates.
 * Returns `undefined` only when `raw` is empty or not a string.
 *
 * Examples:
 *   "50"   → { version: "50" }
 *   "≤50"  → { version: "50", qualifier: "before" }
 *   "<=50" → { version: "50", qualifier: "before" }
 *   "~50"  → { version: "50", qualifier: "approximately" }
 *   "%50"  → { version: "50", qualifier: "approximately" } + warnOnce
 */
function parseBrowserVersionString(
	raw: string,
	compatKey: string,
): ParsedVersionString | undefined {
	if (raw.length === 0) return undefined;
	const match = raw.match(/^([^0-9A-Za-z]*)(.+)$/);
	if (!match) return undefined;
	const [, prefix, body] = match;
	if (body.length === 0) return undefined;
	if (prefix.length === 0) return { version: body };
	const known = KNOWN_VERSION_PREFIXES[prefix];
	if (known) return { version: body, qualifier: known };
	warnOnce(
		`wf-version-prefix:${prefix}`,
		`svg-compat: unrecognised version prefix ${stringifyUnknown(prefix)} (in ${
			stringifyUnknown(raw)
		}) for "${compatKey}" — treating as "approximately". Add it to KNOWN_VERSION_PREFIXES if it should map to "before" or "after".`,
	);
	return { version: body, qualifier: 'approximately' };
}

function parseBrowserNotes(value: unknown): string[] | undefined {
	if (typeof value === 'string') return [value];
	if (!Array.isArray(value)) return undefined;
	const notes = value.filter((entry): entry is string => typeof entry === 'string');
	return notes.length > 0 ? notes : undefined;
}

function parseBrowserFlags(
	value: unknown,
	compatKey: string,
	browser: string,
): BrowserFlag[] | undefined {
	if (!Array.isArray(value)) return undefined;
	const flags: BrowserFlag[] = [];
	for (const entry of value) {
		if (!isRecord(entry)) continue;
		const type = getString(entry.type);
		const name = getString(entry.name);
		if (type === undefined || name === undefined) {
			warnOnce(
				`wf-flag-shape:${stringifyUnknown(entry)}`,
				`svg-compat: unrecognised flag shape ${stringifyUnknown(entry)} for "${compatKey}" / ${browser}. Skipping.`,
			);
			continue;
		}
		const flag: BrowserFlag = { type, name };
		const valueToSet = getString(entry.value_to_set);
		if (valueToSet !== undefined) flag.value_to_set = valueToSet;
		flags.push(flag);
	}
	return flags.length > 0 ? flags : undefined;
}

/**
 * Parses a single browser's `support` entry from a BCD compat block
 * into our typed `BrowserVersion` form.
 *
 * `support[browser]` can be:
 * - a single statement object (most common),
 * - an array of statements (BCD convention: most-recent first),
 * - absent entirely — returns `undefined`.
 *
 * When the upstream shape is unexpected, we warn and return
 * `undefined` rather than inventing data. When the upstream shape
 * is recognised, we ALWAYS return a `BrowserVersion` — no silent
 * discard, even for `version_added: false`.
 */
export function parseBrowserVersion(
	value: unknown,
	browser: string,
	compatKey: string,
): BrowserVersion | undefined {
	if (value === undefined) return undefined;
	const stmt = isRecord(value)
		? value
		: Array.isArray(value) && value.length > 0 && isRecord(value[0])
		? value[0]
		: undefined;
	if (!stmt) {
		warnOnce(
			`wf-browser-shape:${browser}`,
			`svg-compat: unrecognised support statement shape ${stringifyUnknown(value)} for "${compatKey}" / ${browser}. Skipping.`,
		);
		return undefined;
	}

	const rawAdded = stmt.version_added;
	let raw_value_added: BrowserVersion['raw_value_added'];
	if (
		typeof rawAdded === 'string'
		|| typeof rawAdded === 'boolean'
		|| rawAdded === null
	) {
		raw_value_added = rawAdded;
	} else {
		warnOnce(
			`wf-version-added-type:${typeof rawAdded}`,
			`svg-compat: unexpected version_added type ${stringifyUnknown(rawAdded)} for "${compatKey}" / ${browser}. Coercing to null.`,
		);
		raw_value_added = null;
	}

	const result: BrowserVersion = { raw_value_added };

	if (typeof raw_value_added === 'string') {
		const parsed = parseBrowserVersionString(raw_value_added, compatKey);
		if (parsed) {
			result.version_added = parsed.version;
			if (parsed.qualifier !== undefined) result.version_qualifier = parsed.qualifier;
		}
	} else if (raw_value_added === false) {
		result.supported = false;
	} else if (raw_value_added === true) {
		result.supported = true;
	}

	const rawRemoved = getString(stmt.version_removed);
	if (rawRemoved !== undefined) {
		const parsedRemoved = parseBrowserVersionString(rawRemoved, compatKey);
		if (parsedRemoved) {
			result.version_removed = parsedRemoved.version;
			if (parsedRemoved.qualifier !== undefined) {
				result.version_removed_qualifier = parsedRemoved.qualifier;
			}
		}
	}

	if (stmt.partial_implementation === true) result.partial_implementation = true;
	const prefix = getString(stmt.prefix);
	if (prefix !== undefined) result.prefix = prefix;
	const altName = getString(stmt.alternative_name);
	if (altName !== undefined) result.alternative_name = altName;
	const flags = parseBrowserFlags(stmt.flags, compatKey, browser);
	if (flags !== undefined) result.flags = flags;
	const notes = parseBrowserNotes(stmt.notes);
	if (notes !== undefined) result.notes = notes;

	return result;
}

export function extractBrowserSupport(
	compat: JsonRecord,
	compatKey: string,
): BrowserSupport | undefined {
	const support = getRecordProperty(compat, 'support');
	if (!support) return undefined;

	const chrome = parseBrowserVersion(support.chrome, 'chrome', compatKey);
	const edge = parseBrowserVersion(support.edge, 'edge', compatKey);
	const firefox = parseBrowserVersion(support.firefox, 'firefox', compatKey);
	const safari = parseBrowserVersion(support.safari, 'safari', compatKey);

	if (
		chrome === undefined
		&& edge === undefined
		&& firefox === undefined
		&& safari === undefined
	) {
		return undefined;
	}

	const result: BrowserSupport = {};
	if (chrome !== undefined) result.chrome = chrome;
	if (edge !== undefined) result.edge = edge;
	if (firefox !== undefined) result.firefox = firefox;
	if (safari !== undefined) result.safari = safari;
	return result;
}

export function extractSpecUrls(compat: JsonRecord): string[] {
	const url = compat.spec_url;
	if (typeof url === 'string') return [url];
	if (!Array.isArray(url)) return [];
	return url.filter((entry): entry is string => typeof entry === 'string');
}

/** Builds a {@linkcode CompatEntry} from a BCD `__compat` node and web-features lookup. */
export function makeCompatEntry(
	compat: JsonRecord,
	featureMap: JsonRecord,
	compatKey: string,
): CompatEntry {
	const status = getRecordProperty(compat, 'status');
	return {
		description: getString(compat.description),
		mdn_url: getString(compat.mdn_url),
		deprecated: getBoolean(status?.deprecated) ?? false,
		experimental: getBoolean(status?.experimental) ?? false,
		standard_track: getBoolean(status?.standard_track) ?? true,
		spec_url: extractSpecUrls(compat),
		baseline: extractBaseline(compat, featureMap, compatKey),
		discouraged: extractDiscouraged(compat, featureMap, compatKey),
		browser_support: extractBrowserSupport(compat, compatKey),
	};
}
