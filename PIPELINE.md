# SVG data pipeline

`svg-data-regen` fetches upstream specifications, extracts facts, and writes the
split catalog under `crates/svg-data/data/`. `svg-data/build.rs` compiles those
committed files into static Rust data. Normal Cargo builds do not fetch specs.

## Regeneration

```sh
# Refresh from the upstream default branch and current npm packages.
just regen

# Reprocess a specific SVGWG commit while retaining recorded npm versions.
cargo run -p svg-data-regen -- <svgwg-commit> --recorded-packages

# Reparse only compatibility data at the recorded package versions.
cargo run -p svg-data-regen -- --refresh-compat
```

`--recorded-packages` pins BCD, Web Features, and `@webref/css` to the versions
already recorded in the catalog. External CSS and ARIA specification URLs still
follow their declared sources; this option does not make those documents
immutable. The SVGWG commit and dated SVG publications identify SVG sources.

## Inputs and extraction

| Input                                                        | Extraction                               | Result                                                                                          |
| ------------------------------------------------------------ | ---------------------------------------- | ----------------------------------------------------------------------------------------------- |
| SVGWG `publish.xml` and definitions XML at a resolved commit | `discover.rs`, `extract.rs`              | Publication graph, elements, categories, attributes and content models                          |
| Chapter HTML at the same commit                              | `chapter.rs`, `paths.rs`                 | Descriptions, definitions, property values and path grammar                                     |
| Dated SVG 1.1 and SVG 2 indexes                              | `inventory.rs`, `legacy.rs`              | Per-edition membership and historical property values                                           |
| Dated chapters and pinned draft chapters                     | `spec_lifecycle.rs`                      | Explicit deprecated, obsolete and removed declarations with source anchors and attribute owners |
| CSS, SVG module and ARIA definitions                         | `css.rs`, `aria.rs`, `treesitter.rs`     | Referenced property grammars, value spaces and parser inputs                                    |
| BCD and Web Features npm data                                | `compat.rs`, shared compatibility models | Browser statements, BCD flags, Baseline milestones and WebDX advice                             |

The four supported snapshots are SVG 1.1 First Edition (2003), SVG 1.1 Second
Edition (2011), SVG 2 CR (2018), and the rolling SVG 2 editor's draft. Their
structural and grammar coverage is still being expanded in [#51].

## Catalog projection and validation

`catalog.rs` assembles the extracted data, resolves applicability and content
models, projects value grammars, and derives the relationship graph. Lifecycle
derivation combines cross-edition membership with explicit declarations. A
retained obsolete feature stays present; a removed feature does not. Attribute
declarations can be restricted to one bearer, such as `style/type`.

Lifecycle validation rejects conflicting declarations, unknown subjects, missing
source anchors, and retained attributes without a matching definition. It checks
scoped declarations against their bearer. BCD deprecation and WebDX
discouragement remain independent facts; disagreement with spec status is not
itself an extraction failure. See the [lifecycle audit](docs/spec-lifecycle.md).

The generator emits these artifacts and their schemas:

| Artifact                   | Contents                                                               |
| -------------------------- | ---------------------------------------------------------------------- |
| `catalog.json`             | Manifest, SVGWG commit and split-file references                       |
| `catalog.core.json`        | Element and attribute definitions, values and compatibility records    |
| `catalog.compat.json`      | npm provenance and compatibility subfeatures                           |
| `catalog.graph.json`       | Derived relationships                                                  |
| `catalog.tree-sitter.json` | Parser projection and its source provenance                            |
| `snapshots/*.json`         | Edition inventory, lifecycle declarations, aliases and value overrides |

There are no manual verdict tables, BCD/spec exception allowlists, or separate
reconciliation files. Fix extraction or projection code and regenerate.

## Runtime use

`svg-data/build.rs` reads the manifest, snapshots and split documents and emits
the Rust catalog into Cargo's `OUT_DIR`. Profile lookups expose membership,
lifecycle and source declarations; attribute lookups accept an optional bearer
for exact context.

The LSP reuses its document's parsed tree for hover, completion and diagnostics.
Lint and completion share `svg_data::effective_compat::lifecycle`: explicit spec
lifecycle wins, runtime flags replace bundled flags, and historical profiles do
not inherit bundled current-web warning flags. Hover and lint keep the spec
source separate from BCD and WebDX information. Runtime compatibility refreshes
cannot replace specification declarations.

The Deno worker produces browser compatibility data from npm sources. It is not
an independent source of SVG specification lifecycle classifications.

## Verification

```sh
cargo test -p svg-data-regen
# After changes to the grammar projection:
cd grammars/tree-sitter-svg && bun run codegen
# From the repository root:
just verify
```

The generator tests exercise extraction and validation with HTML fixtures.
Runtime and protocol tests cover historical profiles, retained obsolete
features, removed features, exact attribute context and runtime replacement.
`coverage_gate.rs` separately gates unresolved value spaces; `begin` and `end`
remain explicit grammar follow-ups in [#51].

[#51]: https://github.com/kjanat/svg/issues/51
