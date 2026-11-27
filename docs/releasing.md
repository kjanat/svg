# Releasing

## Routine release flow

1. Refresh the catalog with `just regen [ref]`. This updates the catalog,
   grammar inputs, parsers and corpus, then validates the grammar consumers and
   Rust workspace. `just generate-grammars` regenerates from the recorded
   catalog offline; `just check-grammars` checks that output without rewriting.
2. Start version preparation with `beepboop release minor` (or an explicit
   version). Beepboop derives the next version from the shared package
   manifests. The preparation workflow updates every version carrier and
   lockfile, regenerates grammar metadata, and runs `just verify` before
   submitting its version PR. Review the release notes and merge that PR
   manually.
3. Approve publication in Beepboop. `release.yml` checks GitHub's cryptographic
   verification of the exact release commit, then asks Beepboop to sign and
   publish its version tag.
4. The dispatched release operation verifies sources, creates its signed tag,
   drafts the release, builds the target matrix, packages the archives and npm
   artifacts, and runs the distributed runtime checks. Successful validation
   publishes the GitHub release and starts the npm and crates.io publishers.

For a manually prepared release PR, update the shared carriers in
`.github/actions/version-check/carriers.mjs`, refresh the lockfiles, run
`just generate-grammars` and `just verify`, and include its changelog section.
`scripts/release-prepare.ts` remains a local convenience that also commits and
creates a signed tag; use it only when preparing a local tag is intended.

### Beepboop activation

The release stream lives in `.github/beepboop.json`. It preserves the required
publication set and optional experimental targets. Version PRs use manual
merges, and merging the version PR publishes the release once its checks pass.

After merging the configuration, refresh the repository inventory with
`beepboop repositories discover kjanat/svg`. Authorize these runtime grants with
your enrolled security key. Backfill grants are pinned to the dispatch workflow
file, so grant them again when switching from the old publisher bindings:

```sh
beepboop secrets grant kjanat/svg crates-io --stream main --operation promote --environment crates-io
beepboop secrets grant kjanat/svg npm-token --stream main --operation promote --environment npm
beepboop secrets grant kjanat/svg crates-io --stream main --operation crates-backfill --environment crates-io
beepboop secrets grant kjanat/svg npm-token --stream main --operation npm-backfill --environment npm
```

The normal release grants apply to the approved `promote` operation in
`release.yml` on `refs/heads/master`. The parent workflow explicitly calls npm
publishing; App-created release events therefore cannot start a duplicate
publication run. Backfills run through dedicated Beepboop operations on the
default branch. Every publisher retrieves its credential inside its existing
GitHub environment, after the package and runtime verification gates. Dry runs
preserve verification while skipping registry credential retrieval.

The `crates-io` alias must permit first publication of `svg-path-preview` as
well as updates of the existing crates. The `npm-token` alias supplies the
existing npm publishing commands. Beepboop stores and scopes these credentials;
GitHub Actions exchanges OIDC identities for the authorized runtime values. Spec
refresh uses `actions/token` and `actions/pull-request` with repository scopes
for contents, pull requests and issues.

## Verify release archive provenance

The release workflow generates a GitHub build attestation for each binary
archive before publishing the release. Verify a downloaded archive with:

```sh
gh attestation verify "$archive" --repo kjanat/svg --signer-workflow kjanat/svg/.github/workflows/release.yml
```

The attestation identifies the build workflow invocation. Beepboop supplies the
release commit separately, and the build checks out that commit. Archives
published before the attestation step was added may have only checksums.

## Backfill a published version

Find the promotion execution with `beepboop runs list`. Start a backfill for
that release using its execution ID:

```sh
beepboop runs backfill PROMOTION_ID npm-backfill
beepboop runs backfill PROMOTION_ID crates-backfill
```

Beepboop checks that the published tag still points to the recorded commit, then
dispatches the configured workflow. npm rebuilds the package archive from the
published binaries and runs the normal runtime checks before publishing to the
`backfill` dist-tag. If an operation fails, retry it with
`beepboop runs recover BACKFILL_ID --action retry`.

## Pipeline layout

- `distribution/npm/targets.json` — single source of truth: build targets
  (runner, build tool, tier), runtime environments, npm platform packages,
  facades, binaries. Schema in `distribution/npm/targets.schema.json`.
- `.github/workflows/release.yml` — verify → draft release → target matrix →
  build-dist → runtime checks → publish → npm handoff.
- `.github/workflows/npm-release.yml` — smoke-tests the dist artifact, then
  publishes platform packages and facades. Manual dispatch runs verification
  only.
- `.github/workflows/runtime-smoke.yml` — shared required runtime checks used
  before GitHub release publication and again by every npm entry point.
- `.github/workflows/runtime-smoke-ci.yml` — validates that same artifact
  handoff on PRs and `master`, using existing released binaries and the current
  npm templates. It does not compile binaries or publish anything.
- `.github/workflows/crates-release.yml` — publishes the 12 publishable
  workspace crates to crates.io in dependency order
  (`cargo publish --workspace`). Authentication uses the Beepboop `crates-io`
  runtime alias through the environment-protected publishing jobs.
- `.github/actions/*/action.yml` — composite subactions holding all multi-step
  logic (asset packaging/verification, archive download, npm smoke/derive/
  publish, matrix generation).
- `distribution/npm/scripts/build-packages.ts` — builds the npm package trees
  from release tarballs and packs each with `bun pm pack`. Bun resolves
  workspace catalog references using `bun.lock`.
- `distribution/npm/facade/<name>/` — checked-in facade templates;
  `distribution/npm/facade/lib/` holds the shared binary-resolver used by every
  facade.

Tier 3 targets are `experimental: true` and run with `continue-on-error`; their
absence never blocks a release. Tier 1/2 targets are release-blocking.

Cross jobs install their tools from `mise.release.toml`. `Cross.toml` pins the
FreeBSD ARM64 and LoongArch GNU container images. FreeBSD ARM64 loads the shared
`.cargo/build-std.toml` with nightly Rust. NetBSD x86-64 uses the host cross
toolchain because the Cross image lacks `libexecinfo`.

The Linux x86-64 musl leg runs all three built executables through an
`actions-shells` Cross smoke step before uploading the archive.

## Distributed runtime coverage

Runtime environments are explicit `runtime` objects on the existing entries in
`distribution/npm/targets.json`. All configured runtime checks must pass before
the GitHub release is published. Every npm trigger, including Beepboop
backfills, also runs the checks before either the grammar or platform
publication wave. Failure prevents subsequent facade, alias, and bundle
publication.

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

The `dist` artifact contains a tarred `dist/` and `downloads/`: the npm package
trees, Bun-packed `.tgz` files, and original release archives with checksums.
Runtime jobs install these `.tgz` files, and publication uploads the same files.
Checks verify:

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

`just release-package-test` covers policy, archive integrity, version matching,
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
- Source checkouts use the recorded release commit for backfills and the
  requested release tag for normal releases and verification, in `source/`. The
  commit must match setup, and the tag version must match all publishable
  packages. The toolchain comes from that source's `rust-toolchain.toml`.
- The helper selection does **not** change which workflow YAML GitHub evaluates.
  An old tag or rerun can retain an old workflow definition. To use an updated
  definition for an existing release, start a Beepboop backfill. Manually
  dispatch `crates-release.yml` with the existing tag to verify without uploads.

Manual dispatch always verifies without uploading. A reusable workflow call can
also set `dry-run: true`. Normal registry index and dependency downloads can
occur during verification. If verification artifacts expire, start a fresh
preparation run rather than bypassing the checks.

`just release-package-test` exercises real unpublished-sibling packaging, an
excluded required input, archive/source mismatches, workflow wiring and the
publication retry loop. It also runs as part of `just verify`. CI separately
verifies the repository's actual packages on every PR and push to `master`,
without uploading them.

References:
[Cargo package verification](https://doc.rust-lang.org/cargo/commands/cargo-package.html)
and [checkout revision selection](https://github.com/actions/checkout#usage).

## Registry authentication

npm publishing uses the Beepboop `npm-token` alias and retains
`npm publish --provenance`. crates.io publishing uses the `crates-io` alias,
including the first `svg-path-preview` publication. Both aliases are delivered
at runtime through the configured operation/ref/environment grants above.

The npm platform scope is set in `distribution/npm/targets.json`. Preserve
`.github/workflows/npm-release.yml` and the crates publishing workflow paths
when changing registry integration.

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
