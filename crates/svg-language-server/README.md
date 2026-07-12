# svg-language-server

[![Crates.io](https://img.shields.io/crates/v/svg-language-server?logo=rust&labelColor=B7410E&color=black)](https://crates.io/crates/svg-language-server)
[![NPM](https://img.shields.io/npm/v/svg-language-server?logo=npm&labelColor=CB3837&color=black)](https://npm.im/svg-language-server)

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="https://raw.githubusercontent.com/kjanat/svg/b7c6611efa83adfb4cccc6f8054940fa6491c3b1/docs/assets/editor-hover-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="https://raw.githubusercontent.com/kjanat/svg/b7c6611efa83adfb4cccc6f8054940fa6491c3b1/docs/assets/editor-hover.png">
  <img alt="svg-language-server in Zed: hover docs with browser support, deprecated/experimental diagnostics, and missing-reference hints" src="https://raw.githubusercontent.com/kjanat/svg/b7c6611efa83adfb4cccc6f8054940fa6491c3b1/docs/assets/editor-hover.png" width="100%">
</picture>

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

## Configuration

All settings go in the LSP `initializationOptions`, under an `svg` key:

```jsonc
{
	"svg": {
		"profile": "svg2draft", // spec snapshot to validate against
		"force_profile": false, // ignore the document's version attribute
		"edition": "svg11", // or { "series": "svg2", "editors_draft": true }
		"runtime_compat": true, // live MDN BCD + web-features refresh at startup
		"svgwg_drift_check": false // opt-in staleness probe against W3C/svgwg
	}
}
```

| Option                  | Type             | Default | Effect                                                                                                                                                                                                                                                    |
| ----------------------- | ---------------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `svg.profile`           | string           | derived | Spec snapshot used for element/attribute validation and hover docs. Accepts snapshot ids and aliases (`svg11`, `svg2`, `svg2draft`, `svg-native`, ...). Without it, the document's `version` attribute decides, falling back to the SVG 2 editor's draft. |
| `svg.force_profile`     | bool             | `false` | Apply `svg.profile` even when the document declares a conflicting `version` attribute.                                                                                                                                                                    |
| `svg.edition`           | string or object | unset   | Pin an exact spec edition. String form resolves aliases (`svg11`, `svg2draft`); object form is `{ "series": "svg10"\|"svg11"\|"svg2", "date": "YYYYMMDD" }` or `{ "series": ..., "editors_draft": true }`. Takes precedence over `svg.profile`.           |
| `svg.runtime_compat`    | bool             | `true`  | Fetch fresh MDN browser-compat-data + web-features at startup and overlay them on the baked catalog. Set `false` for fully offline/private sessions (baked data is still used).                                                                           |
| `svg.svgwg_drift_check` | bool             | `false` | Opt-in: probe `api.w3.org`/`api.github.com` once at startup and warn when the baked spec catalog has drifted from the live specs.                                                                                                                         |

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg
