# Compatibility metadata v2

Issues [#41](https://github.com/kjanat/svg/issues/41) and
[#42](https://github.com/kjanat/svg/issues/42) change the public Rust and JSON
compatibility contracts. This is a breaking change for consumers of the 0.2 API
and must ship in the next minor release on the 0.x line. Package versions remain
unchanged in this PR.

The split catalog documents and worker `/data.json` now declare
`schema_version: 2`. Check that version before consuming a document. Worker
clients previously received an unversioned document. `/schema.json` and the CLI
`emit schema` command describe the new worker contract; the catalog's individual
schema files describe its documents.

## Baseline

Only upstream `false`, `"low"`, and `"high"` map to `limited`, `newly`, and
`widely`. The selected `status.by_compat_key[compat_key]` takes precedence over
feature-wide status, including an empty or unknown override. Dates and the four
displayed desktop browser products do not determine eligibility.

A Baseline object can have no `status`. Treat that as unknown, display neutral
text or nothing, and count it separately from Limited availability. An absent
Baseline object means there were no status or date facts to retain.
`status_diagnostic` distinguishes missing status from unrecognized status.
`raw_status` is the original field encoded as JSON: `"false"` represents the
boolean false, `"null"` represents null, and `"\"high\""` represents the string
high. It is retained for recognized values too.

The catalog's old `{ "kind": "widely", "since": 2022, "qualifier": null }` and
the worker's `since` / `since_qualifier` fields are replaced by:

```json
{
	"status": "widely",
	"raw_status": "\"high\"",
	"low_date": { "raw": "2020-01-15", "date": "2020-01-15" },
	"high_date": { "raw": "2022-07-15", "date": "2022-07-15" }
}
```

Both optional dates survive regardless of tier. A missing or malformed date does
not remove a recognized tier. Each date retains `raw`; `date` is present only
for a full valid calendar date. Known qualifiers are `before`, `after`, and
`approximately`. An unknown prefix retains only `raw`, without inventing an
approximation. Unexpected non-string date values are retained as JSON text in
`raw` without a parsed date.

Derive a display year from `low_date.date` for **Newly Available** and from
`high_date.date` for **Widely Available**. If that milestone is missing, show
the tier without a year. Hover and dashboard details label both milestones
explicitly; in the example above, Newly Available is 2020 and Widely Available
is 2022. Never label the high date as an unqualified "Baseline since" date.

In Rust, match `BaselineStatus.status: Option<BaselineTier>` instead of the old
`BaselineStatus::{Widely, Newly, Limited}` enum variants. Generated facts use
borrowed strings through `BaselineStatus`; runtime facts use
`compat_model::Baseline<String>`. `as_ref()` provides a common borrowed view.
`BaselineDate::year()` and `Baseline::milestone()` support presentation.
`VerdictReason::BaselineNewly.since` is now optional. Struct literals for
elements, attributes, and compatibility facts must include `discouraged`.

## WebDX discouragement

The separate `discouraged` array retains feature identity (`feature_id` and
optional `feature_name`), the applicable `compat_key`, `scope: "feature"`,
`reason`, optional `reason_html`, supporting `according_to` references,
`alternatives` feature IDs, and optional `removal_date`.

The advice applies to the named whole feature. Context keys remain attached when
attribute facts are combined, so advice is not silently recast as an
attribute-wide or per-key decision. Do not equate it with BCD `deprecated`, SVG
lifecycle deprecation, or lack of a browser implementation. Ordinary Limited
availability does not imply discouragement. The imported Baseline status stays
available in JSON even when advice replaces its visual badge.

Hover and the dashboard show the explanation, context, references, and
alternatives. The CLI reports separate Baseline buckets and discouragement
details. Upstream prose is rendered as text; `reason_html` is preserved but is
not trusted HTML.

## Attribute contexts and summaries

Worker attributes now contain `contexts`, keyed by the exact original BCD key,
and a separate project-derived `aggregate`. A context such as
`svg.elements.rect.width` retains its own Baseline and advice; it cannot take
advice from `svg.elements.svg.width`.

The dashboard labels the aggregate as an observed-context summary and exposes
individual contexts in each row. The aggregate selects the worst **known**
Baseline tier, then the later corresponding milestone on ties. BCD deprecated
and experimental flags require agreement across observed contexts; non-standard
status and browser limitations use the conservative existing merge policy.
Discouragement is a union whose entries keep their exact feature and context
keys. An aggregate can combine facts from different contexts and is not an
upstream claim about an attribute everywhere.

`aggregation: "observed-contexts"` identifies this policy. `coverage` reports
`contexts`, `baseline_known`, `baseline_unknown`, and `baseline_missing`. A
known aggregate does not fill gaps: unlisted elements and missing Baseline
records remain unknown. The CLI reports this coverage alongside its summary.
JSON clients must read `attribute.aggregate.baseline` for a summary or
`attribute.contexts[key].baseline` for exact facts.

## Runtime refresh and effective facts

Each source resolves its package version first, then downloads that pinned
version. BCD and Web Features are resolved independently for the requested
context: exact element-plus-attribute key first, then the global attribute key.
A present empty or unrecognized exact Web Features override is authoritative.
The resolver does not merge unrelated element contexts.

The bundled catalog retains its genuinely common attribute fallback. A disabled
or failed refresh preserves the bundled contextual facts, including that common
fallback. A successful source load with no applicable key clears that source's
old facts. An invalid or unknown Baseline clears the old tier and stays neutral.
The server refreshes once per session; its failure policy is bundled fallback,
not an undocumented mixture with a previous session's cache.

Effective records distinguish loaded, absent, unknown, failed, and disabled
outcomes. Hover records the contributing package versions, URLs and selected
keys. Failed refreshes explicitly label retained bundled facts stale. A partial
refresh shows separate BCD and Web Features provenance, including the bundled
version for the failed source.

One complete effective record supplies the badge, dates, discouragement, browser
versions, notes, flags, removals and compatibility verdict. Fresh compatibility
reasons replace old ones; independent SVG profile restrictions remain. Lint and
completion preserve the element context through their adapters. An explicit
neutral verdict clears old warnings. Missing or unknown browser support is
displayed as unknown, separately from an explicit unsupported value.

The Rust `effective_compat::Facts` model owns refreshed strings; generated
catalog records convert into it for the shared verdict calculation.
`VerdictReason` owns prefix and removal-version strings and is no longer `Copy`.
`LintOverrides` and `VerdictOverrides` have `attribute_contexts` maps keyed by
`(element, attribute)`, taking precedence over their common-attribute maps.
`compat_sources()` exposes the bundled package identities.

The worker builds from two successfully loaded packages. It does not manufacture
a mixed-source output after a failed upstream load; the runtime fallback policy
above belongs to the language server.

## Regeneration and verification

The generator and runtime parser share `crates/svg-data/src/compat_model.rs`.
Local fixtures in `src/fixtures/web-features.json` exercise both Rust paths and
the worker's parser, JSON output, counts, and rendered dashboard. Hover tests
cover the same fixtures, and a protocol test checks both dated milestones
through the generated static catalog.

To refresh compatibility metadata from the versions already recorded in
`catalog.compat.json`, preserving specification-derived fields and BCD facts:

```sh
cargo run -p svg-data-regen -- --refresh-compat
```

The command reparses the pinned package URLs and regenerates the schemas. The
normal full regeneration command still resolves current upstream packages. This
migration keeps BCD 8.0.13, web-features 3.36.0, and the existing spec commit.
Run `just verify` before submitting changes.
