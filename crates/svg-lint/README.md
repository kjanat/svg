# svg-lint

[![Crates.io](https://img.shields.io/crates/v/svg-lint?logo=rust&labelColor=B7410E&color=black)](https://crates.io/crates/svg-lint)
[![NPM](https://img.shields.io/npm/v/%40svg-toolkit%2Fsvg-lint?logo=npm&labelColor=CB3837&color=black)](https://npm.im/@svg-toolkit/svg-lint)

Structural linting for SVG documents — validates element nesting, attribute
usage, and ID uniqueness against the SVG spec.

## Rules

| Code                         | Severity | Description                                       |
| ---------------------------- | -------- | ------------------------------------------------- |
| `UnknownElement`             | Warning  | Element not in the SVG spec                       |
| `InvalidChild`               | Warning  | Child in void element or wrong-category nesting   |
| `DuplicateId`                | Warning  | Duplicate `id` attribute value                    |
| `UnknownAttribute`           | Warning  | Attribute not recognized for a given element      |
| `DeprecatedElement`          | Warning  | Element marked deprecated in the SVG/BCD catalog  |
| `DeprecatedAttribute`        | Warning  | Attribute marked deprecated (including `xlink:*`) |
| `ExperimentalElement`        | Hint     | Element marked experimental                       |
| `ExperimentalAttribute`      | Hint     | Attribute marked experimental                     |
| `MissingReferenceDefinition` | Warning  | `url(#id)` target has no matching definition      |
| `UnusedSuppression`          | Warning  | Suppression comment did not suppress anything     |

## Install

```sh
cargo install svg-lint                       # from crates.io
npm install --global @svg-toolkit/svg-lint   # prebuilt binary via npm
```

npm refuses the unscoped name `svg-lint` ("too similar" to the unrelated
`svglint` — fucking great, thanks npm), so the npm package lives under the
`@svg-toolkit` scope; the binary is still plain `svg-lint`. It also ships in the
[`@kjanat/svg-toolkit`](https://npm.im/@kjanat/svg-toolkit) bundle.

## API

```rust
use svg_lint::{lint, lint_tree, LintOverrides, CompatFlags};

// Parse and lint in one call
let diagnostics = lint(source);

// Lint an already-parsed tree
let diagnostics = lint_tree(source, &tree, None);

// Lint with runtime compat overrides (e.g. from live BCD data)
let overrides = LintOverrides {
    elements: [("font".into(), CompatFlags { deprecated: true, experimental: false })]
        .into_iter().collect(),
    attributes: Default::default(),
};
let diagnostics = lint_tree(source, &tree, Some(&overrides));
```

Each diagnostic includes a `DiagnosticCode`, `Severity`, message, and source
location (byte range + row/col).

## Suppression Comments

```xml
<!-- svg-lint-disable-next-line DuplicateId -->
<rect id="dup" />

<!-- svg-lint-disable MissingReferenceDefinition -->
```

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg
