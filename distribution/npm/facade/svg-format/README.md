# svg-format

[![NPM](https://img.shields.io/npm/v/svg-format?logo=npm&labelColor=CB3837&color=black)](https://npm.im/svg-format)

Structural formatter for SVG documents.

```sh
npm install -g svg-format                 # unscoped
npm install -g @svg-toolkit/svg-format    # same package, scoped
svg-format icon.svg
npx svg-format icon.svg
```

Both names ship byte-identical content; pick whichever you like. Or grab the
whole toolkit at once with
[`@kjanat/svg-toolkit`](https://npm.im/@kjanat/svg-toolkit).

The package resolves a prebuilt native binary for your platform via
`optionalDependencies` — no postinstall step, no network access at runtime.

Formatting behavior and options: <https://github.com/kjanat/svg#readme>
