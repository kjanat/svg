# Crate release verification implementation

Design:
[verified crate releases](../specs/2026-09-08-crate-release-verification.md).

- [x] Confirm current workflow behavior and Cargo's unpublished-sibling staging.
- [x] Replace order-only preparation with package verification and an archive
      report.
- [x] Pin helpers across jobs and record helper/source/toolchain identities.
- [x] Gate publication on the verified package contents while retaining retries.
- [x] Add real packaging regressions, workflow checks and no-upload CI coverage.
- [x] Update release/recovery documentation and changelog.
- [x] Review the complete change and run repository verification.

Deliver one PR closing #43 and #44, and verify its remote checks.
