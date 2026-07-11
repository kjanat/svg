# svg-language-server

LSP server for SVG: diagnostics, completions, hover, formatting, and references.

```sh
npm install -g svg-language-server
svg-language-server --version
```

The package resolves a prebuilt native binary for your platform via
`optionalDependencies` — no postinstall step, no network access at runtime. The
server speaks LSP over stdio; point your editor's LSP client at the
`svg-language-server` executable.

Editor setup, supported diagnostics, and configuration options:
<https://github.com/kjanat/svg#readme>
