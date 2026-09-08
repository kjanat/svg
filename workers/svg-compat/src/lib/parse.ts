/**
 * Extraction primitives that turn raw upstream JSON (BCD `__compat`
 * + web-features feature map) into typed `CompatEntry` / `Baseline`
 * / `BrowserSupport` objects.
 *
 * Recognized status and dates are independent. Preserve raw input and parsing
 * diagnostics without guessing tiers or the meaning of unknown qualifiers.
 * @module
 */

import type { Baseline, BaselineDate, BrowserFlag, BrowserSupport, BrowserVersion, CompatEntry, Discouraged } from '#lib/types.ts';
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
	const rawSupport = getRecordProperty(input, 'support');
	const support = rawSupport
		? Object.fromEntries(Object.entries(rawSupport).filter((entry): entry is [string, string] => typeof entry[1] === 'string'))
		: undefined;
	if (value === undefined && !low_date && !high_date && !support) return undefined;
	const status = value === false ? 'limited' : value === 'low' ? 'newly' : value === 'high' ? 'widely' : undefined;
	const status_diagnostic = status ? undefined : value === undefined ? 'missing' : 'unrecognized';
	if (status_diagnostic) {
		warnOnce(`wf-baseline:${stringifyUnknown(value)}`, `svg-compat: ${status_diagnostic} baseline value for "${compatKey}"; no recognized tier.`);
	}
	return { support, status, raw_status: value === undefined ? undefined : stringifyUnknown(value), status_diagnostic, low_date, high_date };
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

/** Parse a single statement without selecting or discarding support history. */
export function parseBrowserVersion(value: unknown, browser: string, compatKey: string): BrowserVersion | undefined {
	if (!isRecord(value)) return undefined;
	const strings = (v: unknown): string[] => typeof v === 'string' ? [v] : Array.isArray(v) ? v.filter((s): s is string => typeof s === 'string') : [];
	const flags: BrowserFlag[] = [];
	for (const flag of Array.isArray(value.flags) ? value.flags : []) {
		if (!isRecord(flag) || typeof flag.name !== 'string' || (flag.type !== 'preference' && flag.type !== 'runtime_flag')) {
			warnOnce(`bcd-flag:${stringifyUnknown(flag)}`, `Invalid BCD flag for ${compatKey}/${browser}`);
			continue;
		}
		flags.push({ type: flag.type, name: flag.name, ...(typeof flag.value_to_set === 'string' ? { value_to_set: flag.value_to_set } : {}) });
	}
	const result: BrowserVersion = {
		flags,
		notes: strings(value.notes),
		impl_url: strings(value.impl_url),
		partial_implementation: value.partial_implementation === true,
	};
	if (typeof value.version_added === 'string' || value.version_added === false) result.version_added = value.version_added;
	for (const key of ['version_removed', 'version_last', 'prefix', 'alternative_name'] as const) {
		if (typeof value[key] === 'string') result[key] = value[key];
	}
	return result;
}

export function extractBrowserSupport(compat: JsonRecord, compatKey: string): BrowserSupport | undefined {
	const support = getRecordProperty(compat, 'support');
	if (!support) return undefined;
	return Object.fromEntries(
		Object.entries(support).map(([id, value]) => [
			id,
			(Array.isArray(value) ? value : [value]).map(v => parseBrowserVersion(v, id, compatKey)).filter((v): v is BrowserVersion => v !== undefined),
		]),
	);
}

/** Choose current unrestricted support for compact display; storage always keeps every statement. */
export function selectBrowserStatement(statements: BrowserVersion[] | undefined): BrowserVersion | undefined {
	return statements?.find(v =>
		typeof v.version_added === 'string' && v.version_removed === undefined && !v.flags.length && v.prefix === undefined
		&& v.alternative_name === undefined && !v.partial_implementation
	)
		?? statements?.find(v => typeof v.version_added === 'string' && v.version_removed === undefined)
		?? statements?.[0];
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
