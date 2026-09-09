# Packaged runtime smoke plan

Design: [spec](../specs/2026-09-09-packaged-runtime-smoke.md).

- [x] Add runtime metadata and schema constraints to the target manifest.
- [x] Replace the Linux-only smoke script with platform-aware Node checks.
- [x] Add shared runtime matrix, archive handoff, and evidence reporting.
- [x] Gate GitHub release and every npm publication entry point.
- [x] Add PR validation using the shared workflow and existing released
      archives.
- [x] Add focused regressions, release documentation, and changelog entries.
- [x] Review the full diff and run `just verify` plus the real Windows smoke.

The PR workflow runs all eight remote runtime jobs. Their workflow summaries and
JSON reports record the actual validation results.
