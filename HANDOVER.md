# Handover: spec-derived `svg-data` catalog

> For the next agent. You have **zero prior context** - this document is your
> complete briefing. Read it fully before touching anything. Last updated by the
> session that produced commits `7430c84`..`b5ecf9d` on the branch
> `9-derive-property-value-enums-from-spec-deterministically-and-reproducably-instead-of-hand-maintaining-shit`.

---

## 1. The one-paragraph goal

Replace the old **hand-maintained** SVG specification data in `crates/svg-data`
with a **single deterministic pipeline** that fetches the SVG spec from
**canonical upstream**, extracts everything into structured data, and surfaces
it (profile-aware, permalinked) in the language server (`svg-language-server`)
and linter (`svg-lint`). Delete the derived data and **one command rebuilds it
byte-for-byte**. No human curates the data; the spec is the source of truth.

The git branch name says it: derive property-value enums (and everything else)
from spec, deterministically and reproducibly, instead of hand-maintaining.

---

## 2. Hard rules (do NOT violate - these caused real friction when ignored)

1. **Canonical upstream only.** Fetch from the network at generation time:
   - GitHub `w3c/svgwg` at the **resolved default-branch HEAD** (resolve the
     default branch via the GitHub API - **never hardcode `main`/`master`**; it
     has been wrong before). The repo's default branch is currently `main`. The
     SVG 2 spec source lives in the `master/` **directory** (not a branch).
   - W3C `/TR/` for the SVG 1.1 / 1.0 RECs (not done yet - P4).
   - npm/CDN `@mdn/browser-compat-data` for browser compat (not done yet - P4).
2. **NEVER read the local `svgwg/` or `w3/` directories.** They are
   long-standing, gitignored, possibly-stale local clones owned by the user.
   They are **not inputs to anything**. Do not read, parse, cite, or record
   their SHA. Fetch canonical upstream over the network instead.
3. **Spec compat != browser compat.** Two separate axes, separately sourced,
   separately modeled, never blended into one verdict:
   - *Spec* axis: per-edition presence + spec lifecycle. Source: the spec.
   - *Browser* axis: baseline + per-browser support. Source: MDN bcd.
4. **Determinism.** Same upstream commit -> byte-identical output. Sort
   everything (the pipeline uses sorted `Vec`s / `BTreeMap`). No wall-clock
   dates (use the fetched commit's committer date). Verify with:
   `rm crates/svg-data/data/catalog.json && cargo run -p svg-data-regen` then
   `git diff` should be empty.
5. **No lint suppressions, ever.** No `#[allow]`, `#[expect]`, `@ts-ignore`,
   etc. Fix warnings with real code. The bar is **zero warnings**, not "exit 0".
6. **No `any`, no `!` non-null, no `as` type-asserts** (TS); no `.unwrap()` /
   `.expect()` in Rust (the workspace **denies** `unwrap_used`/`expect_used`,
   even in tests - write `fn test() -> Result<(), Box<dyn Error>>` with `?`, or
   use `let ... else { panic!() }` + `assert!`).
7. **`data/catalog.json` is write-protected for the agent.** `.claude/settings.json`
   denies the agent's Write/Edit tools on `crates/svg-data/data/**`. That's
   intentional: the **pipeline** writes it (via `cargo run`), the agent only
   `git add`s the result. Never hand-edit it.
8. **Commit discipline.** Only commit/stage when the user explicitly says so.
   `git commit` commits **all staged content**, not just what you `git add` -
   check `git status` first and beware sweeping in unrelated pre-staged files
   (this bit us - see Known Issue #2). No Claude attribution in commits. No
   "clippy/tests green" status lines in commit messages. No `git checkout --` /
   `git restore` (destroys work). No force-push / amend without instruction.
9. **No em-dashes or non-ASCII in source/comments** (user dislikes them; use
   `-` and `...`). Watch blanket find/replace not creating markdown list bullets
   in doc comments.
10. **Shell hooks are active.** A hook blocks decorative banner echoes
    (`echo "=== Title ==="`) in Bash - emit plain output. Another logs
    destructive actions to `~/.claude/logs/destructive-action-log.jsonl`.

---

## 3. Architecture / data flow

```
   (network, at regen time)                    (committed)         (build time)        (runtime)
w3c/svgwg @ HEAD --fetch--> svg-data-regen --> data/catalog.json --> build.rs --> ELEMENTS --> svg-data API
   W3C /TR/ (P4)            (extract+map)        (derived data)     (codegen)    (static)   element(), etc.
   MDN bcd  (P4)                                                                              |
                                                                              svg-lint + svg-language-server
```

Two crates do the work:

- **`crates/svg-data-regen`** (new this session; `publish = false`): the
  pipeline. A binary, run with `cargo run -p svg-data-regen`. It fetches
  canonical upstream, extracts, and **writes** `crates/svg-data/data/catalog.json`.
  The heavy fetch/parse deps (`ureq`, `quick-xml`, `tl`, `regex`, `sha1`) live
  here so the runtime crate stays lean.
- **`crates/svg-data`**: the runtime crate the LSP+linter consume. Its
  `build.rs` reads `data/catalog.json` and generates `catalog.rs` (static
  `ElementDef`/`AttributeDef`/`SnapshotMetadata` arrays) into `OUT_DIR`. The
  public types live in `src/types.rs`; the API in `src/lib.rs`. **If
  `data/catalog.json` is absent, build.rs emits an empty catalog so the crate
  still compiles.**

### `svg-data-regen` source map (`crates/svg-data-regen/src/`)

- `main.rs` - orchestration + the `report*` functions (print a summary) +
  `write_catalog` (writes `data/catalog.json`). Env knobs:
  `REGEN_SAMPLE=<name>` prints the element/property/term with that name as JSON.
  Optional CLI arg = the git ref to pin (default: resolved default-branch HEAD).
- `fetch.rs` - GitHub API default-branch + commit resolution; raw file fetch at
  a pinned SHA; `resolve_repo_path` (collapses `..`/`.` in module hrefs).
- `discover.rs` - parses `master/publish.xml` (the publication manifest) into
  the input graph: version base URLs, definitions modules, chapters, appendices.
- `extract.rs` - parses each `definitions*.xml` module into typed records:
  elements (with content-model kind, allowed categories/elements, attribute
  categories, common attrs, geometry props, interfaces, nested attrs), global
  attributes, properties, element/attribute categories, terms/symbols/interfaces.
- `chapter.rs` - parses chapter/appendix HTML (`tl` crate): `id` anchors,
  `<dfn>` term definitions, `<edit:example>` refs, `<table class="propdef">`
  **property value-definition tables** (-> value grammar + parsed enum
  keywords), and `<dl class="definitions">` term descriptions (with
  `<edit:elementcategory>` macro expansion).
- `catalog.rs` - maps the extracted records into the committed `catalog.json`
  shape (`Catalog`/`CatalogElement`/`CatalogContentModel`). **Flattens** content
  models (categories resolved to member elements + explicit elements) so the
  runtime needs no category enum.
- `provenance.rs` - typed run identity (repo, ref, SHA, commit date, base URLs).

### `svg-data` key files

- `src/types.rs` - the runtime ADTs (the contract the LSP+linter compile
  against). `ElementDef`, `AttributeDef`, `ContentModel`
  (`ChildrenSet`/`AnySvg`/`Foreign`/`Void`/`Text`/`Children{categories,elements}`),
  `SpecSnapshotId`, `BaselineStatus`, `CompatVerdict`, `ProfileLookup`, etc.
  All fields are `&'static` (baked literals emitted by build.rs).
- `src/lib.rs` - the public API: `element()`, `attribute()`, `elements()`,
  `element_for_profile()`, `attributes_for_with_profile()`,
  `allowed_children_with_profile()`, `allows_foreign_children()`,
  `compat_verdict_for_*()`, `snapshot_*()`, `resolve_profile_id()`, etc.
- `build.rs` - reads `data/catalog.json` -> emits `catalog.rs`. Currently emits
  `ELEMENTS` (populated) + `ATTRIBUTES` (EMPTY) + `SNAPSHOT_METADATA` (EMPTY).

---

## 4. Phase status (the task list - also tracked in the TaskCreate system)

| Phase  | What                                                                                 | Status                                                                                                                                                                  |
| ------ | ------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **P0** | Scaffold `svg-data` types + empty catalog so the workspace compiles                  | DONE (pre-session, commit `fcaf36e`)                                                                                                                                    |
| **P1** | Canonical fetch + provenance + `publish.xml` discovery                               | **DONE** (`7430c84`)                                                                                                                                                    |
| **P2** | Definitions extractor over all 5 modules                                             | **DONE** (`7430c84`, `2e97da6`)                                                                                                                                         |
| **P3** | Chapter/HTML extractors: anchors, examples, descriptions, **property value enums**   | **DONE** (`2e97da6`, `4eeecf8`, `8249e12`, `4fb3592`)                                                                                                                   |
| **P4** | Editions (SVG 1.1/1.0 from `/TR/`) + lifecycle (`changes.html`) + MDN browser-compat | **TODO**                                                                                                                                                                |
| **P5** | Profile-aware permalinks (`spec_url` per profile)                                    | **PARTIAL** - a single editors-draft `spec_url` is emitted now; per-profile variants TODO                                                                               |
| **P6** | build.rs codegen + determinism + repro/provenance gates                              | **IN PROGRESS** - elements wired + deterministic + repro verified (`d655c5e`); ATTRIBUTES/SNAPSHOT_METADATA still empty; no automated repro/provenance test gate yet    |
| **P7** | Surface real data in hover/lint (spec + browser kept distinct)                       | **TODO** - linter already consumes `ELEMENTS` (content models work); hover descriptions/compat not surfaced; one lint correctness fix landed (`b5ecf9d`, see section 6) |
| **P8** | One command (`just refresh-spec`) + CI auto-PR on schedule                           | **TODO** - `just regen`/`regen-sample`/`regen-test` recipes exist; no scheduled CI yet                                                                                  |

---

## 5. What actually works right now

- `cargo run -p svg-data-regen` fetches `w3c/svgwg` @ HEAD, extracts the whole
  corpus, prints a summary, and writes `data/catalog.json` (**63 elements**,
  deterministic, commit-pinned to `3c69f0f79087...`).
- `svg-data::element("circle")` returns real spec data: flattened content model
  (15 allowed children), element attrs, `global_attrs`, a `spec_url` permalink.
- `allowed_children_with_profile`, `attributes_for_with_profile` (elements side),
  `allows_foreign_children` run off the real catalog.
- The linter recognizes real SVG elements and flags genuine `InvalidChild` /
  `UnknownElement` errors from the spec content models (verified on `samples/`).
- Tests: `svg-data-regen` 9/9 green (parser fixtures), `svg-data` 3/3 green.
- `run lint` (== `just lint` == `cargo clippy --workspace --all-targets
  --all-features -- -D clippy::all`) is **0 warnings**.

Run/verify commands (the user invokes `run <recipe>` which dispatches to `just`):

- `run regen` - full pipeline (writes `data/catalog.json`).
- `run regen master` - pin to a branch/tag/SHA.
- `run regen-sample "container element"` - inspect any element/property/term by
  name as JSON (quoted, so multi-word terms work).
- `run regen-test` - offline parser tests (no network).
- `run lint` - the zero-warning gate.
- Determinism check: `rm crates/svg-data/data/catalog.json && cargo run -p
  svg-data-regen && git diff --stat crates/svg-data/data/catalog.json` (empty).

---

## 6. KNOWN ISSUES / open items (read before continuing)

### Issue #1 - 11 svg-lint tests fail (pre-existing, blocked on unwired data)

`cargo test -p svg-lint` -> **58 passed, 11 failed**. These are NOT regressions
from this session's work; they have failed since the spec data was wiped and
have nothing wired to satisfy them yet. They fall in three buckets:

- **Profile-aware attribute presence** (`unsupported_attribute_is_distinct_from_unknown`,
  `unsupported_attribute_does_not_also_emit_deprecated`,
  `multiline_tag_suppression_*`) - assert `UnsupportedInProfile` for `href` in
  SVG 1.1 / `xlink:href` in SVG 2. **CORRECTION (verified by reading the tests):
  wiring `ATTRIBUTES` alone will NOT flip these green.** `attribute_for_profile()`
  in `src/lib.rs` is stubbed (`let _ = profile;` -> always `Present`+`Stable`),
  so no profile verdict fires regardless of the catalog. These need the
  **per-edition attribute inventory (P4)**. `ATTRIBUTES` is a necessary
  prerequisite, not sufficient.
- **Browser-compat** (`compat_deprecated_attribute_emits_diagnostic`,
  `deprecated_attribute_message_surfaces_bcd_origin_under_latest_profile`,
  `partial_implementation_fires_as_info_on_color_interpolation`,
  `verdict_override_changes_attribute_advisory`) - need **MDN bcd** data (**P4**).
- **Edition / SVG-Native** (`svg_1_0_edition_accepts_element_the_svg_1_1_snapshot_dropped`,
  `baseprofile_in_svg2_fires_unsupported_not_obsolete`,
  `svg_native_flags_unsupported_element_and_attribute`) - need edition presence
  data + SVG Native profile data (**P4** / native extraction).

To turn the suite green you must wire `ATTRIBUTES` (P6) **and** the profile/edition
logic + P4 data. After wiring `ATTRIBUTES`, RUN the suite to observe what actually
flips rather than pre-promising greens. Do not "fix" these tests by weakening
assertions; they encode correct behavior.

### `AttributeDef` field mapping (for the P6 attribute slice)

Build `ATTRIBUTES` as the dedup-by-canonical-name union of: top-level global
attributes + attribute-category members + element-nested `<attribute>` children +
element `common_attributes`. Per `AttributeDef` field:

- `name`: `AttributeRef.name` via `xlink::canonical_svg_attribute_name` (xlink:href -> href).
- `spec_url`: `AttributeRef.href` joined with the module base (absolute hrefs, e.g.
  aria -> wai-aria, used as-is); same logic as elements.
- `animatable`: `AttributeRef.animatable.unwrap_or(false)` (spec-derived).
- `presentation_attribute`: `Some(name)` when the attribute is in the `presentation`
  attribute category (its name == the property name).
- `values`: for presentation attributes, the matching `PropertyValueDef` from
  `chapter.rs` (join by name) -> `AttributeValues::Enum(keywords)` / typed variant;
  otherwise `FreeText` until per-attribute value grammars are parsed. YES - the
  property value tables feed BOTH the property and its presentation attribute.
- `elements`: **empty = global** (attributes from global categories core/presentation/
  aria/conditional-processing/events reach elements via `element.global_attrs`); else
  the bearer element names. This global-vs-scoped policy is the one real design
  decision - confirm with the user.
- `deprecated`: `false` for now, BUT note the spec signal - attributes in the
  `deprecated xlink` category are spec-deprecated (a spec-axis input distinct from MDN).
- `description`/`mdn_url`/`baseline`/`browser_support`/`verdicts`/`experimental`:
  empty/`false`/`None`/`&[]` (P3-ext / P4). `value_overrides`: `&[]` (P4/P5).

### Issue #2 - commit `b5ecf9d` swept in two unrelated user files

`b5ecf9d` accidentally included `.zed/settings.json` and
`samples/validity-vs-reliability.svg` - they were pre-staged by the user (not by
the agent), and `git commit` takes all staged content. Not pushed. The user was
asked whether to split them out via `git reset --soft HEAD~1` + re-commit and
**had not answered** when this handover was written. **Ask the user** before
rewriting that commit (it contains their changes).

### Issue #3 - `spec_url` is editors-draft only (P5 not done)

Every element's `spec_url` currently points at the editors-draft render
(`https://w3c.github.io/svgwg/svg2-draft/...`). Per-profile permalinks (SVG 1.1
`/TR/` anchors, dated CR, etc.) are P5. The base URLs are already captured in
provenance / `publish.xml` `<versions>` (`cvs`/`this`/`latest`/`latestrec`).

### Issue #4 - empty `description` on elements

`ElementDef.description` is `""` for now (the runtime type requires `&str`).
Per-element prose descriptions are a P3-extension / P7 concern. Term
descriptions ARE extracted (`chapter.rs` -> `term_definitions`) but not yet
mapped onto elements in the catalog.

### Issue #5 - `foreignObject` content-model nuance (FIXED, for reference)

The spec's `contentmodel='any'` means "any elements OR character data"
(foreign/HTML content), used by `foreignObject`, `desc`, `title`, `metadata`.
This maps to `ContentModel::Foreign` (NOT `AnySvg`). The linter clears the
default namespace for a foreign host's descendants, so unprefixed HTML (`<div>`)
is not flagged as unknown SVG, while a nested explicit `<svg xmlns>` re-declares
the SVG namespace and re-enters linting. Fixed in `b5ecf9d`; regression-tested.
(Do not regress this by treating `any` as `AnySvg`.)

---

## 7. Recommended next steps (in order)

1. **Resolve Issue #2** with the user (split their files out of `b5ecf9d`, or
   leave it).
2. **Wire `ATTRIBUTES` (finish P6 element-style for attributes).** Mirror what
   `catalog.rs` + `build.rs` do for elements: collect the union of attributes
   (global + per-category + element-specific) from the extraction, dedup by
   canonical name, emit `AttributeDef` literals. `values` (value space) can come
   from the property value-definition tables already extracted in `chapter.rs`
   (the `PropertyValueDef` records: grammar + enum `keywords`); attributes
   without a propdef table get `AttributeValues::FreeText`. This alone should
   clear the "attribute-presence" test failures (Issue #1, bucket 1).
3. **Add a repro/provenance test gate (finish P6).** A committed test (or CI
   step) that fails if a fresh `cargo run -p svg-data-regen` changes
   `data/catalog.json` bytes, and that records the fetched SHA. Note: this needs
   the network, so it likely belongs in CI, not a unit test.
4. **P4: MDN browser-compat + editions.** Fetch `@mdn/browser-compat-data` SVG
   subtree -> `BaselineStatus` + `BrowserSupport` + `CompatVerdict`. Fetch SVG
   1.1/1.0 from W3C `/TR/` for per-edition presence + `changes.html` lifecycle.
   Keep the spec and browser axes SEPARATE (see Hard Rule #3). This clears
   Issue #1 buckets 2 and 3.
5. **P5: per-profile permalinks.** Resolve `spec_url` per `SpecSnapshotId` using
   the per-edition base URLs.
6. **P7: hover.** Surface description + spec line + browser-support line
   (distinct) in `crates/svg-language-server/src/hover.rs`.
7. **P8: `just refresh-spec` + scheduled CI** that runs the pipeline and opens a
   PR when the derived data changes.

---

## 8. Session commit log (this branch, newest first)

```
b5ecf9d fix(lint): treat foreignObject content as foreign, not SVG
d655c5e feat(svg-data): wire extracted elements into the catalog
8708fc1 chore(just): add spec-regen recipes
94a4dda refactor(svg-data-regen): unify the sample selector
4fb3592 fix(svg-data-regen): expand category macros in descriptions
5f1f4a3 test(svg-data-regen): cover the parsers against fixtures
8249e12 feat(svg-data-regen): term definitions with descriptions
4eeecf8 feat(svg-data-regen): derive property value enums
2e97da6 feat(svg-data-regen): content model + anchors
7430c84 feat(svg-data-regen): fetch spec, extract definitions
```

(`fcaf36e`/`e472f3d` predate this session: the greenfield scaffold + provenance
hardening.) The branch is ahead of origin; nothing was pushed by the agent.

---

## 9. Reference facts (saves you a fetch)

- Pinned commit at handover time: `3c69f0f79087ec4ac37a7653e25dbe2ebc7f04d1`
  (committer date `2026-06-15T13:33:39Z`).
- `publish.xml` input graph: **5** definitions modules, **16** chapters,
  **11** appendices.
- Extraction tallies: 63 elements, 117 properties, 17 element categories, 15
  attribute categories, 1155 chapter anchors, 244 dfns, 31 examples, 31 property
  value-definition tables, 53 term definitions.
- The 4 `contentmodel='any'` (foreign) elements: `desc`, `foreignObject`,
  `metadata`, `title`.
- Dep API notes (versions pinned in the workspace): `ureq` 3.3
  (`get(url).header(k,v).call()?` then `body_mut().with_config().limit(n).read_to_string()`);
  `quick-xml` 0.40 (`Reader::from_str`, `read_event()`, `local_name()`, text via
  `xml10_content()`, attrs via `normalized_value(XmlVersion::default())`);
  `tl` 0.7 (`tl::parse`, `dom.nodes()`, `node.as_tag()`/`as_raw()`,
  `tag.children().top()`, `tag.query_selector(parser, sel)`).
- The user runs tasks via a custom `run` CLI that dispatches to the `justfile`
  (`run lint` == `just lint`). Don't fabricate commands they didn't run.
