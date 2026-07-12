# svg-format

[![NPM](https://img.shields.io/npm/v/svg-format?logo=npm&labelColor=CB3837&color=black)](https://npm.im/svg-format)

Structural formatter for SVG documents.

```sh
npm install -g svg-format                 # unscoped alias
npm install -g @svg-toolkit/svg-format    # canonical package
svg-format icon.svg
npx svg-format icon.svg
```

`@svg-toolkit/svg-format` is the canonical package; unscoped `svg-format` is a
thin alias that depends on it. Either install gives you the same bin. Or grab
the whole toolkit at once with
[`@kjanat/svg-toolkit`](https://npm.im/@kjanat/svg-toolkit).

The package resolves a prebuilt native binary for your platform via
`optionalDependencies` — no postinstall step, no network access at runtime.

Formatting behavior and options: <https://github.com/kjanat/svg#readme>
