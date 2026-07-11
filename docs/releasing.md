# Releasing

## Routine release flow

1. Ensure `bun` is installed locally.
2. Run `just release-local <version>`.
3. Review the generated commit and local tag.
4. Push the branch and tag:
   - `git push origin <branch>`
   - `git push origin v<version>`
5. GitHub Actions (`release.yml`) verifies (clippy + tests), drafts the GitHub
   Release, builds every target in `distribution/npm/targets.json`, uploads
   `svg-<tag>-<target>.tar.gz` archives with `.sha256` checksums, builds the npm
   package trees, publishes the release, then hands off to `npm-release.yml` to
   publish the platform packages and the facades (`svg-language-server`,
   `svg-lint`, `svg-format`).

## Pipeline layout

- `distribution/npm/targets.json` — single source of truth: build targets
  (runner, build tool, tier), npm platform packages, facades, binaries. Schema
  in `distribution/npm/targets.schema.json`.
- `.github/workflows/release.yml` — verify → draft release → target matrix →
  build-dist → publish → npm handoff.
- `.github/workflows/npm-release.yml` — smoke-tests the dist artifact, then
  publishes platform packages and facades. Also runnable via `workflow_dispatch`
  for backfills and dry runs.
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

## npm bootstrap

The long-term path is trusted publishing from GitHub Actions using OIDC
(`npm publish --provenance`, `id-token: write`).

Because npm trusted publishers are configured per existing package, the first
publish of each new package name may require a temporary `NPM_TOKEN` secret in
GitHub Actions. Once the first publish exists:

1. Configure trusted publishers for the facades (`svg-language-server`,
   `svg-lint`, `svg-format`) and every `@kjanat/*` platform package.
2. Point each package at this repository and the stable workflow file
   `.github/workflows/npm-release.yml`.
3. Remove the temporary `NPM_TOKEN` secret so later releases rely on OIDC only.

The platform-package scope is set in `distribution/npm/targets.json` (`scope`);
it must be a scope the publishing npm account owns.

## Notes

- `just release-local <version>` updates the workspace version in `Cargo.toml`,
  runs local checks, creates the release commit, and creates the local
  `v<version>` tag. It depends on `bun` for the helper script.
- `just release-config-check` validates `distribution/npm/targets.json`
  invariants and syntax-checks the workflow scripts; `just release-preview`
  prints the build matrix.
- Do not rename `.github/workflows/npm-release.yml` after trusted publishers are
  configured unless you also update npm's trusted-publisher settings.
