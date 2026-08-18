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

On Linux, the binary is picked by the host's libc: musl hosts get the musl
build, glibc hosts get the glibc build. `npm`, `pnpm` and `yarn` honour the
`libc` field and install only the right one; `bun` and `deno` install both, so
detection settles it at launch. Set `SVG_LIBC=musl` or `SVG_LIBC=glibc` to
override if detection is ever wrong.

Formatting behavior and options: <https://github.com/kjanat/svg#readme>
