/**
 * Public type definitions for the SVG compat output.
 *
 * Pure type module — no runtime, no imports beyond `sources.ts` for
 * the upstream-source descriptor types. Both the HTTP server in
 * `main.ts` and the CLI in `cli.ts` consume this module via
 * `lib/mod.ts`, so there is exactly one source of truth for the
 * shape of `/data.json`.
 *
 * @module
 */

import type { SvgCompatSources } from '#src/sources.ts';

export type { SourceInfo, SvgCompatSources } from '#src/sources.ts';

/**
 * A baseline date as we received it from web-features, plus the
 * parsed-out clean form when extractable.
 *
 * `raw` is **always** present, even when we successfully parsed
 * `date` and `qualifier` — that way no upstream byte is ever
 * silently lost. Web-features uses prefixes like `≤2021-04-02`
 * for "at or before this date", `≥` / `~` etc. for other forms
 * of uncertainty; the upstream schema declares the field as plain
 * `string` so future versions can ship any prefix.
 *
 * If the parser could not extract a clean `YYYY-MM-DD` from `raw`,
 * only `raw` is set and a `warnOnce` fires so the unknown shape
 * is visible in worker logs.
 */
export interface BaselineDate {
	/** Original upstream value, byte-for-byte. Always present. */
	raw: string;
	/** ISO `YYYY-MM-DD` extracted from `raw`. Absent if unparseable. */
	date?: string;
	/**
	 * Set when `raw` carried a qualifier prefix:
	 *
	 * - `"before"`        — `≤` / `<` / `<=`
	 * - `"after"`         — `≥` / `>` / `>=`
	 * - `"approximately"` — `~` / `≈`. Unknown prefixes retain only raw input.
	 */
	qualifier?: 'before' | 'after' | 'approximately';
}

/** Imported Baseline facts. Unknown and missing statuses have no recognized tier. */
export interface Baseline {
	/** Independently resolved Web Features browser versions for this exact status. */
	support?: Record<string, string>;
	status?: 'widely' | 'newly' | 'limited';
	/** Original upstream baseline field encoded as JSON, preserving invalid types. */
	raw_status?: string;
	status_diagnostic?: 'missing' | 'unrecognized';
	/** Optional Newly Available milestone, including raw malformed input. */
	low_date?: BaselineDate;
	/** Optional Widely Available milestone, including raw malformed input. */
	high_date?: BaselineDate;
}

/** Whole-feature WebDX advice, separately scoped from Baseline and BCD flags. */
export interface Discouraged {
	feature_id: string;
	compat_key: string;
	scope: 'feature';
	feature_name?: string;
	reason: string;
	reason_html?: string;
	according_to: string[];
	alternatives: string[];
	removal_date?: string;
}

/** Flag categories and values use the declarations shipped with BCD. */
export type BrowserFlag = import('bcd').FlagStatement;

/** Complete statement fields; singleton notes/links are normalized to arrays. */
export interface BrowserVersion
	extends Omit<Partial<import('bcd').SimpleSupportStatement>, 'notes' | 'impl_url' | 'flags' | 'partial_implementation'>
{
	flags: BrowserFlag[];
	notes: string[];
	impl_url: string[];
	partial_implementation: boolean;
}

/** All products and all their original support statements, in upstream order. */
export type BrowserSupport = Record<string, BrowserVersion[]>;

/** Processed compatibility entry for an SVG element or attribute. */
export interface CompatEntry {
	/** Human-readable feature description from BCD. */
	description?: string;
	/** MDN documentation URL. */
	mdn_url?: string;
	/** Whether the feature is deprecated. */
	deprecated: boolean;
	/** Whether the feature is experimental (single-implementer). */
	experimental: boolean;
	/** Whether the feature is on a standards track. */
	standard_track: boolean;
	/** Specification URLs from BCD. */
	spec_url: string[];
	/** Baseline status from web-features. */
	baseline?: Baseline;
	/** WebDX explanations retain their feature identity across attribute aggregation. */
	discouraged?: Discouraged[];
	/** Browser support statements from BCD, including history and conditions. */
	browser_support?: BrowserSupport;
}

/** Exact upstream contexts and an explicitly project-derived attribute summary. */
export interface AttributeEntry {
	/** Element names observed in BCD. `*` denotes a global record. */
	elements: string[];
	/** Facts keyed by their full original BCD compatibility key. */
	contexts: Record<string, CompatEntry>;
	/** Summary of observed contexts, never an upstream attribute-wide status. */
	aggregate: CompatEntry;
	/** The aggregate considers observed contexts only; unobserved elements are unknown. */
	aggregation: 'observed-contexts';
	/** Coverage makes unknown or absent Baseline data visible alongside a known summary. */
	coverage: { contexts: number; baseline_known: number; baseline_unknown: number; baseline_missing: number };
}

/** Top-level JSON response shape served at `/data.json`. */
export interface SvgCompatOutput {
	/** Version of this output contract, independent of upstream package versions. */
	schema_version: 2;
	/** ISO timestamp of when this output was generated. */
	generated_at: string;
	/** Upstream package versions used to build this response. */
	sources: SvgCompatSources;
	/** SVG elements keyed by tag name. */
	elements: Record<string, CompatEntry>;
	/** SVG attributes keyed by attribute name (xlink uses colon notation). */
	attributes: Record<string, AttributeEntry>;
}

/** Internal snapshot before timestamp is added. Used as the cache value. */
export interface SvgCompatSnapshot {
	/** Resolved upstream package versions. */
	sources: SvgCompatSources;
	/** Processed SVG elements. */
	elements: Record<string, CompatEntry>;
	/** Processed SVG attributes. */
	attributes: Record<string, AttributeEntry>;
}
