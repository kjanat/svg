# Compatibility metadata v2

Issue [#41](https://github.com/kjanat/svg/issues/41) changes the public Rust and
JSON compatibility contracts. This is a breaking change for consumers of the 0.2
API and must ship in the next minor release on the 0.x line. Package versions
remain unchanged in this PR.

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

Runtime overlay and broader attribute-context reconciliation are tracked
separately in [#42](https://github.com/kjanat/svg/issues/42).

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
