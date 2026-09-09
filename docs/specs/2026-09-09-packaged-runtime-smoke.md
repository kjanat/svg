# Packaged runtime coverage

## Goal

Close #45 by executing the distributed archives and npm packages on the eight
tier-1 targets, after artifact transfer, without reducing the cross-build
matrix.

## Design

- Add runtime runner/container metadata to the existing target manifest. Targets
  without it have build/package coverage only. Every configured runtime is a
  required gate; experimental targets remain build-only and non-blocking.
- Reuse one workflow from both the release and npm publication entry points.
  Download the producer's `dist` artifact and the corresponding release archive;
  never compile inside a smoke job. Verify checksums, executable modes, exact
  versions, and binary identity through extraction, npm packing, and
  installation.
- Use native Node on Windows/macOS/GNU runners and Node inside Alpine for musl.
  Assert the actual OS, architecture, and libc before accepting runtime
  evidence.
- Install platform packages, facades, aliases, and the bundle in separate clean
  projects to avoid ambiguous command collisions. Exercise native executables,
  direct JavaScript launchers, and npm-generated command links or Windows shims.
- Keep built, packaged, installed, and executed evidence separate. Missing,
  failed, skipped, and unconfigured stages never count as runtime success.
- Add a PR workflow that builds npm trees from an existing published release's
  archives and transfers them before running the same runtime workflow. It does
  not create releases, publish packages, or rebuild the binaries.

## Validation

Regression tests cover manifest policy, version mismatches, permissions, archive
identity, platform selection, incomplete stage reporting, and publication gates.
Run the real Windows smoke locally, `just verify`, and all eight remote runtime
jobs. Reuse the existing GNU/musl resolver tests from #19.

## Non-goals

No new cross-build targets, registry publication, version bump, resolver
rewrite, or claim of runtime coverage on the twelve targets without a configured
runner.
