# Baseline assets and user guidance plan

Design: [matching spec](../specs/2026-09-09-baseline-assets-and-guidance.md).

- [x] Verify issues and current implementation against official upstream
      guidance.
- [x] Vendor pinned, unchanged light/dark SVGs and record hashes and
      attribution.
- [x] Replace hand-maintained copies with generated worker copies and add
      checks.
- [x] Integrate theme selection and proportional sizing in both display paths.
- [x] Add user guidance to README, LSP docs, dashboard, CLI, and Rust docs.
- [x] Verify icon bytes, status mapping, serving, theme rendering, and
      packaging.
- [x] Run `just verify`, review the complete diff, and prepare one PR from
      master.

## File map

- `crates/svg-language-server/assets/`: canonical icons, manifest, notices.
- `scripts/baseline-icons.ts`, `justfile`, `.github/workflows/ci.yml`: copying
  and verification.
- `crates/svg-language-server/src/hover.rs`: LSP image container and regression.
- `workers/svg-compat/static/badges/`, `src/components/`, `src/render.tsx`:
  generated assets, theme selection, and accessible explanation.
- `README.md`, `docs/baseline.md`, crate READMEs, `THIRD_PARTY_NOTICES.md`,
  worker CLI/help: consistent semantics and provenance.

## Checks

Local `just verify` passed, including 94 worker/CLI tests, the Rust workspace
and protocol suites, doctests, formatting, typechecks, and release configuration
checks. The changed CI workflow also passed `actionlint` directly. The new
regressions cover served bytes, all status/theme mappings, unchanged embedded
artwork, and rejection of altered, missing, or unexpected copies.

`just baseline-icons-check`, `just baseline-icons-upstream`, `just verify`,
Cargo archive inspection, focused rendering inspection, and PR CI.

The actual language-server `.crate` contains all six icons with matching hashes,
the source manifest, and the asset notice. This asset packaging check used
`cargo package --no-verify`; it does not claim a separate package build or
upload. Chromium checks of the dashboard and an image captured from the real LSP
confirm 18 by 10 sizing and the official palettes in light and dark themes. The
LSP fixture also exercises native-size and enlarged rendering. Clients without
theme media-query support retain the light variant.
