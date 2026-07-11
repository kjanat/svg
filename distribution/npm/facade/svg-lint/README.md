# @svg-toolkit/svg-lint

[![NPM](https://img.shields.io/npm/v/%40svg-toolkit%2Fsvg-lint?logo=npm&labelColor=CB3837&color=black)](https://npm.im/@svg-toolkit/svg-lint)

Structural linter for SVG documents.

```sh
npm install -g @svg-toolkit/svg-lint
svg-lint icon.svg
npx -p @svg-toolkit/svg-lint svg-lint icon.svg
```

There is no unscoped `svg-lint` package: npm's typosquat filter refuses the name
because it's "too similar" to the unrelated existing `svglint`. Fucking great,
thanks npm. The binary is still called plain `svg-lint`, and it also ships in
the [`@kjanat/svg-toolkit`](https://npm.im/@kjanat/svg-toolkit) bundle.

The package resolves a prebuilt native binary for your platform via
`optionalDependencies` — no postinstall step, no network access at runtime.

Rules, suppression comments, and JSON output:
<https://github.com/kjanat/svg#readme>
