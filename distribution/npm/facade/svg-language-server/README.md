# svg-language-server

[![NPM](https://img.shields.io/npm/v/svg-language-server?logo=npm&labelColor=CB3837&color=black)](https://npm.im/svg-language-server)

LSP server for SVG: diagnostics, completions, hover, formatting, and references.

<img alt="svg-language-server in Zed: hover docs with browser support, deprecated/experimental diagnostics, and missing-reference hints" src="https://raw.githubusercontent.com/kjanat/svg/master/docs/assets/editor-hover.png" width="100%">

```sh
npm install -g svg-language-server                 # unscoped alias
npm install -g @svg-toolkit/svg-language-server    # canonical package
svg-language-server --version                      # `svg-ls` works too
```

`@svg-toolkit/svg-language-server` is the canonical package; unscoped
`svg-language-server` is a thin alias that depends on it. Either install gives
you the same bins. Or grab the whole toolkit at once with
[`@kjanat/svg-toolkit`](https://npm.im/@kjanat/svg-toolkit).

The package resolves a prebuilt native binary for your platform via
`optionalDependencies` — no postinstall step, no install-time network access.
The server speaks LSP over stdio; point your editor's LSP client at the
`svg-language-server` executable.

At runtime the server fetches fresh browser-compat data on startup by default
(degrading cleanly to the baked catalog when offline). Set the
`svg.runtime_compat: false` initialization option to keep sessions fully
offline.

Editor setup, supported diagnostics, and configuration options:
<https://github.com/kjanat/svg#readme>
