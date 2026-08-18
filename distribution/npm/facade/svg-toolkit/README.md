# @kjanat/svg-toolkit

Every svg-toolkit CLI in one install:

```sh
npm install -g @kjanat/svg-toolkit
```

puts `svg-language-server` (alias `svg-ls`), `svg-lint`, and `svg-format` on
your PATH, via exact-pinned dependencies on the individual packages.

## Why not just `svg-toolkit`?

Because npm. The unscoped name `svg-toolkit` has been squatted since 2019 by an
abandoned "base 3d" package, and npm's name police block anything that even
smells like an existing name — which is also why there is no unscoped `svg-lint`
(npm considers it "too similar" to `svglint`; fucking great, thanks npm). So:
scoped it is.

## Prefer the individual tools?

| Tool       | Unscoped alias        | Canonical                          |
| ---------- | --------------------- | ---------------------------------- |
| LSP server | `svg-language-server` | `@svg-toolkit/svg-language-server` |
| Linter     | — (blocked by npm)    | `@svg-toolkit/svg-lint`            |
| Formatter  | `svg-format`          | `@svg-toolkit/svg-format`          |

Each resolves a prebuilt native binary for your platform via
`optionalDependencies` on `@svg-toolkit/*` platform packages — no postinstall
step, no install-time network access outside npm.

On Linux, the binary is picked by the host's libc: musl hosts get the musl
build, glibc hosts get the glibc build. `npm`, `pnpm` and `yarn` honour the
`libc` field and install only the right one; `bun` and `deno` install both, so
detection settles it at launch. Set `SVG_LIBC=musl` or `SVG_LIBC=glibc` to
override if detection is ever wrong.

Docs: <https://github.com/kjanat/svg#readme>
