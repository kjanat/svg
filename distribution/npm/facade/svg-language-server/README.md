# svg-language-server

LSP server for SVG: diagnostics, completions, hover, formatting, and references.

```sh
npm install -g svg-language-server
svg-language-server --version
```

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
