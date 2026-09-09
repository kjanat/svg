# GitHub workflow knowledge base

The release workflows and composite actions are hand-maintained. The release
target/package catalog is `distribution/npm/targets.json`; the procedure is
`docs/releasing.md`.

## Where to look

| Task                                   | Location                                                                          |
| -------------------------------------- | --------------------------------------------------------------------------------- |
| Release archives and workflow handoffs | `workflows/release.yml`                                                           |
| npm artifact verification/publication  | `workflows/npm-release.yml`, `actions/npm-*`                                      |
| Crate package verification/publication | `workflows/crates-release.yml`, `actions/crates-verify`, `actions/crates-publish` |
| Ordinary PR checks                     | `workflows/ci.yml`                                                                |
| Release regression tests               | `tests/`                                                                          |

## Conventions

- Preserve trusted-publisher workflow paths, credential isolation and declared
  permissions when changing publication.
- Crate helper code and release source code are separate checkouts. Setup
  resolves default-branch helpers once; publishers reuse that SHA. Sources stay
  on the requested tag and must match setup's recorded source SHA.
- Every publishable crate must pass Cargo's extracted-package build before any
  crate upload. Preserve the verified archives and report across jobs.
- `--no-verify` is allowed for archive comparison and final uploads only after
  the complete verification gate. Never describe workspace builds or an
  order-only dry run as verification of packaged contents.
- Use the Python shell for Python-only workflow steps. Keep Python entry points
  with shebangs executable in Git.
- `just release-config-check` checks workflow/script syntax;
  `just release-package-test` checks behavior. `just verify` includes both.
- Keep workflow selection separate from helper selection in recovery docs.
  Changing default-branch helpers does not replace an old run's workflow YAML.
