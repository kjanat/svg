# svg-language-server

[![NPM](https://img.shields.io/npm/v/svg-language-server?logo=npm&labelColor=CB3837&color=black)](https://npm.im/svg-language-server)

LSP server for SVG: diagnostics, completions, hover, formatting, and references.

```sh
npm install -g svg-language-server                 # unscoped
npm install -g @svg-toolkit/svg-language-server    # same package, scoped
svg-language-server --version                      # `svg-ls` works too
```

Both names ship byte-identical content; pick whichever you like. Or grab the
whole toolkit at once with
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
