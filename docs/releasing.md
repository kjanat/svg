# Releasing

## Routine release flow

1. Ensure `bun` and `uv` are installed locally.
2. After a catalog refresh, run `bun run codegen` in `grammars/tree-sitter-svg`
   so `grammar.json` matches `catalog.tree-sitter.json`.
3. Run `just release-local <version>`.
4. Review the generated commit and local tag.
5. Push the branch and tag:
   - `git push origin <branch>`
   - `git push origin v<version>`
6. GitHub Actions (`release.yml`) verifies (clippy + tests), drafts the GitHub
   Release, builds every target in `distribution/npm/targets.json`, uploads
   `svg-<tag>-<target>.tar.gz` archives with `.sha256` checksums, builds the npm
   package trees, transfers and runtime-tests the artifacts, publishes the
   release, then hands off to `npm-release.yml` to publish the platform packages
   and the facades (`svg-language-server`, `svg-lint`, `svg-format`).

## Pipeline layout

- `distribution/npm/targets.json` — single source of truth: build targets
  (runner, build tool, tier), runtime environments, npm platform packages,
  facades, binaries. Schema in `distribution/npm/targets.schema.json`.
- `.github/workflows/release.yml` — verify → draft release → target matrix →
  build-dist → runtime checks → publish → npm handoff.
- `.github/workflows/npm-release.yml` — smoke-tests the dist artifact, then
  publishes platform packages and facades. Also runnable via `workflow_dispatch`
  for backfills and dry runs.
- `.github/workflows/runtime-smoke.yml` — shared required runtime checks used
  before GitHub release publication and again by every npm entry point.
- `.github/workflows/runtime-smoke-ci.yml` — validates that same artifact
  handoff on PRs and `master`, using existing released binaries and the current
  npm templates. It does not compile binaries or publish anything.
- `.github/workflows/crates-release.yml` — publishes the 11 publishable
  workspace crates to crates.io in dependency order
  (`cargo publish
  --workspace`). Bootstrap auth via the `CARGO_REGISTRY_TOKEN`
  secret in the `crates-io` environment; delete it once every crate has a
  trusted publisher configured and OIDC takes over.
- `.github/actions/*/action.yml` — composite subactions holding all multi-step
  logic (asset packaging/verification, archive download, npm smoke/derive/
  publish, matrix generation).
- `distribution/npm/scripts/build-packages.ts` — builds the npm package trees
  from release tarballs.
- `distribution/npm/facade/<name>/` — checked-in facade templates;
  `distribution/npm/facade/lib/` holds the shared binary-resolver used by every
  facade.

Tier 3 targets are `experimental: true` and run with `continue-on-error`; their
absence never blocks a release. Tier 1/2 targets are release-blocking.

## Distributed runtime coverage

Runtime environments are explicit `runtime` objects on the existing entries in
`distribution/npm/targets.json`. All configured runtime checks must pass before
the GitHub release is published. Every npm trigger, including manual backfills,
also runs the checks before either the grammar or platform publication wave.
Failure prevents subsequent facade, alias, and bundle publication.

| Targets                | Runtime environment                                          | Required         |
| ---------------------- | ------------------------------------------------------------ | ---------------- |
| Linux GNU x64 / ARM64  | Native Ubuntu runners                                        | Yes              |
| Linux musl x64 / ARM64 | Official Node Alpine containers on matching Ubuntu runners   | Yes              |
| macOS x64 / ARM64      | Native Intel / Apple Silicon runners                         | Yes              |
| Windows x64 / ARM64    | Native Windows runners with matching Node architecture       | Yes              |
| Other twelve targets   | Build/package checks only, no runtime environment configured | No runtime claim |

Build-only targets retain their existing policy: tier 1/2 build or packaging
failures block release; experimental tier 3 failures remain non-blocking. Adding
a runtime object creates a required gate. Experimental targets cannot declare
one under this policy. There is no emulated architecture coverage implied by a
successful cross-build.

The `dist` artifact contains a tarred `dist/` and `downloads/`: the exact npm
trees and original release archives with checksums. Runtime jobs download and
extract this artifact. They do not rebuild from a checkout. Checks verify:

- The actual runtime OS, Node architecture, and detected libc match the selected
  manifest entry. The existing #19 resolver regressions run in Alpine too.
- Original archive checksums and one copy of each of the three CLIs; executable
  permission bits on Unix before npm has a chance to repair them.
- Binary hashes match through archive extraction, npm packaging, and install.
- Clean installs of each platform package, facade (and any twins), alias, and
  the bundle. A temporary local registry serves the unmodified tarballs and
  platform metadata, so npm itself selects the optional dependencies. Ordinary
  external JavaScript dependencies still come from the public npm registry.
- All raw CLIs, direct launchers, declared aliases, and npm command links report
  the exact expected version. Windows runs the actual `.exe` files and generated
  `.cmd` shims; command links must resolve to an installed artifact entry point.
  Direct launcher checks cover facade/alias/bundle commands even when npm links
  a dependency's identically named command.
- Installed facade resolvers select the expected package and libc, and npm did
  not install an incompatible platform package.

The producer summary reports **built** (verified producer archive evidence) and
**packaged** (all three npm packages present). It leaves **installed** and
**executed** as `not run`. Each runtime job records its own separate stages and
retains a JSON report for 14 days. Failed, absent, skipped, or unconfigured
checks never count as successful runtime execution. These are startup/version
checks, not a claim of full application testing on every platform.

`just release-runtime-test` covers policy, archive integrity, version matching,
permissions, stage reporting, and workflow gates. It is included in
`just verify`. PR runtime validation builds npm trees from the latest published
release's binaries using current templates, then transfers them to all eight
environments. This validates packaging and execution without claiming a new
binary build.

For an existing release whose old `dist` artifact lacks the original archives,
dispatch `runtime-smoke-ci.yml` from the updated default branch with its `tag`
input. A successful run produces a fresh `dist` artifact and runtime evidence
without uploads. That run ID can supply a subsequent `npm-release.yml` dry run
for the same tag. Always review template changes when regenerating an old
version's packages. An old workflow rerun keeps its old YAML; choose the updated
workflow explicitly to use these checks.

Runner availability:
[GitHub-hosted runners](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
Container details:
[official Node Docker images](https://github.com/nodejs/docker-node).

## Crates.io verification and recovery

Every `crates-release.yml` entry point verifies the complete package set before
the first crates.io upload. Setup uses `cargo package` with the locked
dependencies and all features, excluding non-publishable workspace members.
Cargo builds the extracted archives and stages unpublished sibling versions in
its temporary registry. This checks the distributed sources on the runner's
host; it is separate from cross-platform binary runtime testing.

The workflow retains the `.crate` archives and a verification report for 14
days. The report lists each verified crate, its archive checksum, the release
tag, source commit, helper commit, lockfile checksum and Rust/Cargo versions.
Cargo's packaging order supplies the dependency-first publication matrix.

Before uploading a crate, the publishing job checks the report against its clean
source checkout, toolchain and helper revision. It then repackages the crate
without building and requires the archive checksum to match the verified
archive. Upload uses the same package options. Rate-limit retries reuse this
verification; index-propagation retries can repeat packaging, but never the
verification builds. Already-published versions remain resumable.

Helpers and release sources have deliberately separate identities:

- Setup checks out `.github/actions` from the default branch and records its
  exact commit. Every publishing job checks out that recorded helper commit,
  even if the default branch advances during the release.
- Each source checkout still uses the requested release tag in `source/`. Its
  commit must match setup, and the tag version must match all publishable
  packages. The toolchain comes from that source's `rust-toolchain.toml`.
- The helper selection does **not** change which workflow YAML GitHub evaluates.
  An old tag or rerun can retain an old workflow definition. To use an updated
  definition for an existing release, manually dispatch `crates-release.yml`
  from the updated default branch and supply the existing release tag. Start
  with `dry-run: true` to verify packages without uploads.

Both `workflow_dispatch` and `workflow_call` accept `dry-run: true`. Normal
registry index and dependency downloads can occur during verification; no
publishing job runs in this mode. If verification artifacts expire, start a
fresh preparation run rather than bypassing the checks.

`just release-package-test` exercises real unpublished-sibling packaging, an
excluded required input, archive/source mismatches, workflow wiring and the
publication retry loop. It also runs as part of `just verify`. CI separately
verifies the repository's actual packages on every PR and push to `master`,
without uploading them.

References:
[Cargo package verification](https://doc.rust-lang.org/cargo/commands/cargo-package.html)
and [checkout revision selection](https://github.com/actions/checkout#usage).

## npm bootstrap

The long-term path is trusted publishing from GitHub Actions using OIDC
(`npm publish --provenance`, `id-token: write`).

Because npm trusted publishers are configured per existing package, the first
publish of each new package name may require a temporary `NPM_TOKEN` secret in
GitHub Actions. Once the first publish exists:

1. Configure trusted publishers for the facades (`svg-language-server`,
   `svg-lint`, `svg-format`) and every `@svg-toolkit/*` platform package.
2. Point each package at this repository and the stable workflow file
   `.github/workflows/npm-release.yml`.
3. Remove the temporary `NPM_TOKEN` secret so later releases rely on OIDC only.

The platform-package scope is set in `distribution/npm/targets.json` (`scope`);
it must be a scope the publishing npm account owns.

## Notes

- `just release-local <version>` updates the workspace version in `Cargo.toml`,
  refreshes `Cargo.lock`, `bun.lock`, and `uv.lock`, runs local checks, creates
  the release commit, and creates the local `v<version>` tag. It depends on
  `bun` for the helper script and `uv` for the Python workspace lockfile.
- `just release-config-check` validates `distribution/npm/targets.json`
  invariants and syntax-checks the workflow scripts; `just release-preview`
  prints the build matrix.
- Do not rename `.github/workflows/npm-release.yml` after trusted publishers are
  configured unless you also update npm's trusted-publisher settings.
