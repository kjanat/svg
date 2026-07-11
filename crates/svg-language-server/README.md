# svg-language-server

[![Crates.io](https://img.shields.io/crates/v/svg-language-server?logo=rust&labelColor=B7410E&color=black)](https://crates.io/crates/svg-language-server)
[![NPM](https://img.shields.io/npm/v/svg-language-server?logo=npm&labelColor=CB3837&color=black)](https://npm.im/svg-language-server)

LSP server for SVG files — hover docs, completions, diagnostics, and color
swatches.

## Features

- **Hover** — element and attribute documentation with MDN links and baseline
  status
- **Completions** — context-aware suggestions for elements, attributes, and
  values
- **Diagnostics** — structural validation (invalid nesting, unknown elements,
  duplicate IDs, deprecated usage, missing local references)
- **Colors** — color swatches and conversions across hex, `rgb()`, `hsl()`,
  `hwb()`, `lab()`/`lch()`, `oklab()`/`oklch()`, and named colors, including
  `var()` and `color-mix()` resolution in embedded CSS
- **Formatting** — deterministic structural SVG formatting
- **Definitions** — jump to `id`, CSS class, and custom property definitions

## Install

```sh
cargo install svg-language-server
```

## Editor Setup

### Zed

Add to your Zed SVG extension's `extension.toml`:

```toml
[language_servers.svg-language-server]
languages = ["SVG"]
```

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg
