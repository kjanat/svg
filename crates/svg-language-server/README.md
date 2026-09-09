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
  values, including the matching open tag after `</`
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

### Hover presentation

`svg.hover` controls SVG element and attribute hovers. It is read during
initialization and on `workspace/didChangeConfiguration`, without restarting.
For example, show mobile browser support with selected details:

```json
{
	"svg": {
		"hover": {
			"browsers": ["chrome_android", "safari_ios"],
			"sections": [
				"description",
				"baseline",
				"discouraged",
				"browsers",
				"browser_details"
			],
			"browser_details": [
				"notes",
				"flags",
				"version_removed",
				"version_last",
				"implementation_links"
			],
			"browser_history": false
		}
	}
}
```

| Setting                     | Default                                                                                     | Behavior                                                                                                                                                             |
| --------------------------- | ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `svg.hover.browsers`        | `["chrome", "edge", "firefox", "safari"]`                                                   | BCD product IDs, in display order. `[]` hides browser-specific information. Duplicate IDs display once.                                                              |
| `svg.hover.sections`        | All sections below except `web_features_support`                                            | Choose which information blocks appear. Blocks retain the normal reading order. Element and attribute names stay visible.                                            |
| `svg.hover.browser_details` | `notes`, `partial_implementation`, `prefix`, `alternative_name`, `flags`, `version_removed` | Fields shown in `browser_details`. Also accepts `version_last` and `implementation_links`.                                                                           |
| `svg.hover.browser_history` | `false`                                                                                     | Show all original support statements in `browser_details`, including historical and conditional implementations. Otherwise show the selected current implementation. |

Available sections: `description`, `status`, `values`, `baseline`,
`discouraged`, `browsers`, `browser_details`, `web_features_support`, `sources`,
and `links`. When both are enabled, discouragement takes the place of the
Baseline badge. `web_features_support` shows that package's independently
resolved browser versions; the normal browser row uses BCD.

Product IDs also include `firefox_android`, `samsunginternet_android`,
`webview_android`, `webview_ios`, `opera`, `opera_android`, `ie`, `oculus`,
`bun`, `deno`, and `nodejs`. New upstream IDs can be selected without changing
the data model. An explicitly selected product with no data displays unknown.

Omitted fields use defaults. Removing `svg.hover` restores all defaults. Invalid
field types, section names or detail names produce a warning and keep the
previous valid hover settings.

These are presentation preferences. All browser data remains available in the
catalog and runtime records. Diagnostics keep their existing four-browser
policy, and completion documentation keeps its default presentation. Templates
and user-supplied HTML are outside this settings contract.

## Understanding compatibility

Baseline is imported from Web Features. Newly Available means a feature has
reached WebDX's core browser set; Widely Available adds upstream criteria for
longer-established availability. Limited means non-Baseline, including
discouraged features. Missing or unrecognized status stays unknown, without a
badge. Both milestone dates are optional: Newly in 2020 and Widely in 2022
describe different events. Hover details label each date separately.

Browser rows use MDN BCD and default to four desktop products. The wider
Baseline set includes mobile browsers; change `svg.hover.browsers` to inspect
other products. The Status line identifies svg's assessment: Caution/Avoid is
project advice, separate from upstream Baseline and specification validity.

Startup refresh is enabled by default. `svg.runtime_compat: false` keeps the
bundled snapshot. A failed source refresh retains its bundled facts and labels
them stale; a successful source load with no facts clears that source's old
values. The `sources` hover section identifies versions, contexts, and outcomes
for BCD and Web Features independently. The server refreshes once per session.

See the
[compatibility guide](https://github.com/kjanat/svg/blob/master/docs/baseline.md)
for limitations, context precedence, aggregation, and upstream references.
Official Baseline artwork is © Google LLC, licensed under
[CC BY-ND 4.0](https://creativecommons.org/licenses/by-nd/4.0/); Baseline and
its logos are Google trademarks. The [asset notice](assets/BASELINE.md) records
the unmodified source files and their hashes.

## Part of [svg-language-server]

[svg-language-server]: https://github.com/kjanat/svg
