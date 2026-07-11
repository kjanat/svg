# svg-data

Structured SVG specification data — a typed, profile-aware catalog of elements,
attributes, content models, and browser-compat verdicts, generated from
canonical upstream sources (the W3C SVG specs, MDN browser-compat-data, and
web-features).

## Features

- **Element/attribute lookups** — `element("svg")`, `attribute("viewBox")`, with
  content models, categories, deprecation/lifecycle state, and spec permalinks
- **Profile-aware views** — every lookup can be scoped to a spec snapshot: SVG
  1.1 (2003 REC), SVG 1.1 Second Edition (2011 REC), SVG 2 (2018 CR), or the SVG
  2 Editor's Draft
- **Content-model queries** — `allowed_children_with_profile`,
  `attributes_for_with_profile`, foreign-content rules
- **Compat verdicts** — per element, attribute, attribute-on-element, and
  subfeature, backed by baked MDN BCD + web-features data
- **`version` attribute mapping** — resolve `version="1.1"` to the matching
  snapshot and edition

The catalog is baked at build time from the JSON data in `data/`; regeneration
from live upstream is handled by the `svg-data-regen` crate in the same
workspace. Consumers never touch the network.

## API

```rust
use svg_data::{SpecSnapshotId, allowed_children_with_profile, compat_verdict_for_element, element};

// Look up an element definition
let svg = element("svg").expect("svg element");

// What may nest inside <clipPath> under SVG 2?
let children = allowed_children_with_profile(SpecSnapshotId::Svg2EditorsDraft, "clipPath");

// Is <font> safe to use?
let font = element("font").expect("font element");
let verdict = compat_verdict_for_element(font, SpecSnapshotId::Svg2EditorsDraft);
```

## Drift Sentinel

The optional `drift-cli` feature builds the `svgwg-drift` binary, a CI sentinel
that compares the baked catalog against live upstream (`w3c/svgwg` and the
compat sources) and exits non-zero when the shipped data has drifted:

```sh
cargo run -p svg-data --features drift-cli --bin svgwg-drift -- --json
cargo run -p svg-data --features drift-cli --bin svgwg-drift -- --compat-drift
```

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg
