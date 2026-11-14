# Discoveries

> This is a memory file for AI agents. It contains notes on quirks, gotchas, and
> other non-obvious details discovered during development. It is not intended
> for human consumption, but may be useful for debugging or future reference.

## tree-sitter-svg Consumer Gotchas

- `descendant_for_byte_range` returns **anonymous leaf nodes** whose `kind()` is
  the literal text (e.g. `"font-size"`) instead of the named grammar node kind
  (`"length_attribute_name"`). Must walk to `parent()` with `is_named()` check
  to get the typed node for attribute dispatch.

- Typed attribute names (`paint_attribute_name`, `length_attribute_name`,
  `transform_attribute_name`, `viewbox_attribute_name`, `id_attribute_name`)
  each have their own node kind. Generic/unrecognized attributes use
  `attribute_name`. Hover/completion handlers must check **all** attribute name
  kinds, not just the typed ones.

- `attribute` is a tree-sitter supertype, not a stable visible CST wrapper.
  Concrete trees expose `generic_attribute` or typed `*_attribute` nodes (for
  example `href_attribute`, `duration_attribute`, `id_attribute`). Consumers
  must use `svg_tree::is_attribute_node_kind`, not `kind() == "attribute"`, or
  completions/lints silently miss typed attributes.

- Element tag names live inside `start_tag`/`self_closing_tag`/`end_tag` as a
  child `"name"` node. The element node itself is `element`, `svg_element`,
  `path_element`, `style_element`, or `script_element` — you need to descend
  into the tag to get the actual element name text.

- `style_element` and `script_element` have `raw_text` content (opaque text
  blob). Their inner content is **not** parsed as XML — injections handle CSS/JS
  separately. Don't try to walk child elements inside these.

- Path data sub-grammar nodes (`path_command`, `coordinate_pair`,
  `arc_argument`, etc.) are children of `path_data` inside a `path_element`'s
  `d` attribute value. The `d` attribute uses `path_attribute_name` node kind,
  not `attribute_name`.

- Generated exact attribute-name buckets can still conflict lexically. If a name
  moves between generated buckets (for example timing attrs from
  `keyword_attribute` to `css_text_attribute`), branch order in `grammar.js`
  matters: put the safer opaque branch before the stricter branch or the parser
  may still choose the old typed node and error on richer values.

## LSP / tower-lsp-server

- `tower-lsp-server` 0.23 uses `ls_types` (not `lsp_types`), `Uri` (not `Url`),
  native async (no `#[async_trait]`), `Color` fields are `f32`

- Position conversion between tree-sitter (byte offsets, 0-based rows) and LSP
  (UTF-16 code units, 0-based lines) requires explicit `byte_col_to_utf16()` /
  `utf16_to_byte_col()` helpers

- `textDocument.hover.contentFormat` and
  `textDocument.completion.completionItem.documentationFormat` are **ordered by
  client preference**, not sets. A client advertising
  `["plaintext", "markdown"]` understands both and would rather have text, so
  asking whether Markdown is a member picks the wrong one. Take the first
  advertised format the server can produce, and negotiate the two independently
  — a client may prefer different kinds for hover and for completion docs.

- Hover text is generated Markdown whose escaping is deliberate, and a consumer
  that renders it down must undo the generator's choice rather than guess from
  appearance: `escape_metadata` backslashes **every** ASCII punctuation
  character, `metadata_code` wraps its content in a backtick run longer than any
  inside it, `metadata_link` and `format_discouraged` write CommonMark autolinks
  (`<https://...>`), the definition preview opens a fence longer than any
  backtick run in the CSS it wraps, and `format_verdict_headline` emits a block
  quote. Reading the output as text instead of as the generator's intent is how
  a hover ends up naming a different symbol than the one under the cursor.

- A line-initial run of three or more backticks opens a fenced block only when
  the rest of that line carries no backtick — CommonMark forbids backticks in a
  backtick fence's info string. Without that rule a long inline code span at the
  start of a line is indistinguishable from a fence opener.

- Anything that walks hover text runs on the request path over user-authored CSS
  from the open document, so per-character scans that restart must be bounded.
  Memoising "nothing ahead can close this" is only sound for a scan that
  actually ran to the end and found nothing; a candidate rejected for another
  reason (an emphasis delimiter sitting inside a word) says nothing about the
  candidates after it, and folding the two together silently disables the
  feature for the rest of the input.

## Toolchain

- Floating `channel = "nightly"` in `rust-toolchain.toml` broke CI: nightly
  rustc built 2026-08-21 (`nightly-2026-08-22` via `rustup toolchain install`)
  introduced a clippy ICE — "unexpected rigid alias in layout_of after
  normalization" inside `clippy::large_futures`'s `layout_of` query — that
  panics specifically on `svg-language-server`'s async LSP handlers (opaque
  RPITIT/async-fn types from `tower-lsp-server`/`tokio`). Bisected with
  `rustup toolchain install nightly-YYYY-MM-DD` +
  `cargo +nightly-YYYY-MM-DD clippy -p svg-language-server --lib`:
  `nightly-2026-08-21` (rustc built 2026-08-20) is clean, `nightly-2026-08-22`
  (rustc built 2026-08-21) ICEs. Reproduces identically on unmodified `master`,
  so it's a toolchain regression, not a code bug. Pinned `rust-toolchain.toml`
  to `nightly-2026-08-21` until upstream fixes it; re-bisect forward
  periodically to find a fixed nightly and un-pin.

## svg-data Catalog

- `<style>` and `<script>` are "never-rendered" elements not in any traditional
  SVG element category (Container, Shape, etc.). Must be explicitly categorized
  or they'll be rejected as invalid children by content-model lint rules.

- BCD (`@mdn/browser-compat-data`) stores attribute compat under both
  `svg.elements.{el}.{attr}` (element-specific) and
  `svg.global_attributes.{attr}` (presentation attributes). Must check both
  paths for complete coverage.

- BCD `tags` array (e.g. `["web-features:svg"]`) cross-references into
  `web-features` package's `features[id].status` for baseline data. The tag
  prefix `web-features:` must be stripped before lookup.

- Curated catalog covers ~56 attributes but SVG has hundreds. BCD-only
  attributes need auto-generated entries at build time or they get no
  hover/diagnostics.

- `font-stretch` is a known source-of-truth trap. Current repo data keeps it as
  a normal SVG presentation attribute
  (`Svg2EditorsDraft20250914/element_attribute_matrix.json` includes it;
  generated catalog currently says `spec_lifecycle: Stable`,
  `deprecated: false`), while newer MDN / CSS Fonts 4 prose says it was renamed
  to `font-width` and retained as a legacy alias. Useful quotes: SVG 2 styling
  lists `font-stretch` among presentation attributes and required CSS properties
  (`https://svgwg.org/svg2-draft/styling.html`); CSS Fonts 3 calls it
  `Font width: the font-stretch property`
  (`https://www.w3.org/TR/css-fonts-3/#font-stretch-prop`); CSS Fonts 4 says
  `For historical reasons, a font-stretch property exists ... and functions in the identical way to the font-width.`
  Do not mark `font-stretch` deprecated from MDN prose alone; first decide
  whether the repo should follow pinned SVG 2 / CSS Fonts 3 data, or add
  first-class alias/replacement metadata.

## svgwg Upstream Layout

- On 2026-10-14 (w3c/svgwg@09d59ab, PR #1199) SVG 2 moved to Bikeshed:
  `master/publish.xml` and the `master/*.html` chapter sources with `edit:*`
  elements were removed, and the default branch became `main` (the old `master`
  branch is frozen). `master/*.html` files that remain are stub pages of removed
  chapters. The manifest is now `tools/svg2-build/config/svg2.json` plus
  `svg2-split.json`; chapters are `master/<name>.bs`.

- The conversion is checkable: regenerating with the old pipeline at
  w3c/svgwg@8b6274e (last HTML commit) and with the new one at @09d59ab (the
  converted sources, same content) gives the same catalog apart from provenance
  URLs and six descriptions that lose old-source link artifacts
  (`'style element' element` became `'style' element`). Repeat that comparison
  after any change to `bikeshed.rs`.

- Bikeshed renders property links without quotes (`'fill'` shorthand and
  `<a class="property">fill</a>`), and the converted source wraps element and
  attribute links in curly quotes (`‘<{rect}>’`). Lowering puts straight quotes
  around every such name; the lifecycle extractor's `'name' property` patterns
  depend on that.

- The converter added `data-dfn-for` to attribute definitions scoped to an
  attribute category (`conditional processing`, `aria`). Treat those as
  unscoped, or the category name becomes a bogus bearer element in
  `element_values`. Older non-element bearers (`core-attributes`,
  `filter-primitive`, comma-joined element lists) come from the external
  filter/animation specs and predate this.

- Chapter-page ids and single-page ids differ for some headings: the `.bs`
  heading carries the single-page id in `id` and the chapter-page id in
  `data-multipage-id`. Permalinks use chapter pages, so lowering swaps them.
  Empty `<span class="bs-old-id">` aliases exist only for the single page.

- Lifecycle declaration pages are read in name order (as for dated editions);
  the first of two agreeing declarations is recorded, so manifest order would
  change the recorded source (`XLinkRefAttrs` vs `types.html`).

## svg-lint Value Checks

- Typed attribute rules in tree-sitter-svg reject invalid values outright, so
  the value lint never sees them: `id="a b"`,
  `preserveAspectRatio="xMidYMid crop"` and `x="1 two"` parse as `ERROR` nodes.
  Recovery can spill out of the attribute (`<text x="1 two">` reports an unknown
  `<two>` element). Only values the grammar accepts structurally reach
  `values::invalid_reason_for`.
- Some typed attributes wrap the node that carries the `value` field:
  `animate_motion_attribute` contains a `d_attribute`. Read the value field one
  level down, or every `<animateMotion>` attribute goes unchecked.
- The catalog's `d` value space is the `d` property grammar `none | <string>`,
  but the presentation attribute holds bare path data (paths.bs #TheDProperty).
  `values::invalid_reason_for` routes `d` to the path data check.
- The catalog keys the transfer function attributes (`type`, `slope`,
  `intercept`, `amplitude`, `exponent`, `offset`) on `feComponentTransfer` (from
  the Filter Effects anchor ids) instead of on `feFuncR/G/B/A`.
  `values::values_for` maps the children to that key until the regen fixes it.
- The catalog gives `path` no `<stop>` bearer (mesh gradients were deferred),
  yet the svgwg example images still use `<stop path="c ...">`. Value checks run
  only where the attribute's applicability includes the element.
- `AttributeValues::Boolean` covers both HTML boolean attributes (`async`,
  `defer`: empty or the attribute name, never `true`/`false`) and ARIA
  `true`/`false` states, and ARIA allows `""` for every non-required state.
  Checks that share a kind across HTML, ARIA and SVG accept the union and leave
  per-attribute rules to `values/constraints.rs`.
- Catalog enum lists for presentation attributes are narrower than the CSS
  grammars (`display` lacks `flex`, `grid`, `contents`, ...). The keyword check
  must not treat the catalog list as the whole value space for a CSS property.
- `rules/mod.rs` skips `xml:*` attributes (except `xml:space`) before the value
  lint, so the `xml:lang` language tag check never runs.
- CSS Cascade 5 adds a sixth CSS-wide keyword, `revert-rule`; CSS Values 5 adds
  math functions (`progress()`, `random()`, `calc-mix()`) and `<whole-value>`
  notations (`first-valid()`, `cycle()`) that every property accepts. SVG 2
  `wsp` includes U+000C, unlike XML `S`.
- A stale or truncated `svg-data` build output (here after a container restart
  mid-build) makes every element unknown, `<svg>` included, and fails about 150
  svg-lint tests at once. `touch crates/svg-data/build.rs` regenerates it.
- Worktrees sharing one `CARGO_TARGET_DIR` overwrite each other's
  `target/debug/svg-lint`; copy the binary out right after building before a
  corpus sweep.
