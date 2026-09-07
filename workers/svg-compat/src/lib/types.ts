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

/**
 * Qualifier on a browser version number when upstream carries a
 * comparison prefix (`"≤50"`, `"~50"`, etc.). Same semantics as
 * `BaselineDate.qualifier` — one mental model for every inexact
 * upstream value.
 */
export type VersionQualifier = 'before' | 'after' | 'approximately';

/**
 * A per-browser flag declaration — BCD's `FlagStatement` mirrored
 * byte-for-byte. Present when a feature is gated behind a preference
 * or runtime flag in that browser.
 */
export interface BrowserFlag {
	/**
	 * Flag category. BCD currently only ships `"preference"` and
	 * `"runtime_flag"`; the type is open-ended so future values pass
	 * through with a warning rather than getting dropped.
	 */
	type: string;
	/** Preference/flag name. */
	name: string;
	/** Value the flag must be set to for the feature to work. */
	value_to_set?: string;
}

/**
 * A single browser's support state, parsed from a BCD
 * `SimpleSupportStatement`.
 *
 * `raw_value_added` is **always** present — the literal upstream
 * JSON value for `version_added`, byte-for-byte — so no upstream
 * signal is ever silently dropped. Parsed companion fields are
 * best-effort extractions.
 *
 * This shape lets downstream consumers distinguish three states
 * that the old flat-string shape conflated:
 *
 * - **Supported since v50** → `{ raw_value_added: "50", version_added: "50" }`
 * - **Explicitly unsupported** → `{ raw_value_added: false, supported: false }`
 * - **Not in upstream at all** → the field itself is `undefined`
 *
 * Upstream `false` statements (478 in the SVG tree today) previously
 * collapsed to `undefined` and were indistinguishable from "no data".
 */
export interface BrowserVersion {
	/**
	 * Literal `version_added` value from BCD. Always present. One of:
	 *
	 * - a version string like `"50"`, `"≤50"`, `"preview"`,
	 * - `false` (explicitly not supported),
	 * - `true` (supported, version unknown),
	 * - `null` (upstream doesn't know).
	 */
	raw_value_added: string | boolean | null;
	/**
	 * Parsed version string when `raw_value_added` was a usable
	 * version literal. Absent when `raw_value_added` was
	 * `false`/`true`/`null` or unparseable.
	 */
	version_added?: string;
	/**
	 * Qualifier on `version_added` when upstream used `"≤50"` /
	 * `"≥50"` / `"~50"`. Unknown prefixes fall through to
	 * `"approximately"` with a one-time warning.
	 */
	version_qualifier?: VersionQualifier;
	/**
	 * `false` when `raw_value_added === false` — i.e. BCD explicitly
	 * stated this browser does NOT support the feature.
	 * `true` when `raw_value_added === true` — supported, version
	 * unknown. Absent in every other case so callers can distinguish
	 * "upstream is silent" from "upstream explicitly said no".
	 */
	supported?: boolean;
	/** Upstream `version_removed` — present when support was dropped. */
	version_removed?: string;
	/** Qualifier on `version_removed` (same semantics as `version_qualifier`). */
	version_removed_qualifier?: VersionQualifier;
	/**
	 * Upstream `partial_implementation: true` — the browser ships
	 * the feature but deviates from the spec in a compatibility-
	 * affecting way.
	 */
	partial_implementation?: boolean;
	/** Upstream `prefix` — vendor prefix required (e.g. `"-webkit-"`). */
	prefix?: string;
	/**
	 * Upstream `alternative_name` — the browser ships this feature
	 * under a different name.
	 */
	alternative_name?: string;
	/** Upstream `flags` — the feature is behind a pref or runtime flag. */
	flags?: BrowserFlag[];
	/**
	 * Upstream `notes` — free-form caveats. BCD accepts a single
	 * string OR an array of 2+ strings; we normalise to `string[]`.
	 */
	notes?: string[];
}

/**
 * Per-browser support for the four major desktop browsers we track.
 *
 * A field is `undefined` only when upstream has no entry at all for
 * that browser; an explicit `version_added: false` upstream surfaces
 * as `{ raw_value_added: false, supported: false }` so the distinction
 * is preserved.
 */
export interface BrowserSupport {
	/** Chrome desktop support state. */
	chrome?: BrowserVersion;
	/** Edge support state. */
	edge?: BrowserVersion;
	/** Firefox desktop support state. */
	firefox?: BrowserVersion;
	/** Safari desktop support state. */
	safari?: BrowserVersion;
}

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
	/** Minimum browser versions from BCD. */
	browser_support?: BrowserSupport;
}

/** Compat entry for an attribute, with the list of elements it applies to (`["*"]` = global). */
export interface AttributeEntry extends CompatEntry {
	/** Element names this attribute applies to. `["*"]` means global. */
	elements: string[];
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
