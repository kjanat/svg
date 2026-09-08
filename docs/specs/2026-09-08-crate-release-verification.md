# Verified crate releases

Issues #43 and #44 require package verification before publication and a stable
separation between release sources and repairable helper code.

The setup job checks out helpers from the default branch once and records that
commit. All publishing jobs use that exact helper commit. Sources still come
from the requested release tag; publishing checks their commit against setup.
The source toolchain is installed explicitly and recorded with both revisions.

Cargo's workspace packaging command builds every publishable crate with all
features, using its temporary registry for unpublished sibling versions. Its
packaging order puts dependencies first. Only then does setup produce a
verification report and upload the verified `.crate` archives. Both reusable and
manually dispatched runs take this path; dry runs never enter publishing.

Publishing requires the report, matching clean sources, lockfile, toolchain,
helper revision and archive checksums. Before the first upload attempt for a
crate, Cargo repackages it without building and its archive must match the
verified archive exactly. The subsequent upload uses the same package options.
Existing already-published handling, rate-limit retries and index-propagation
retries remain; verification builds are never repeated in retry loops.

Tests exercise real Cargo packaging of unpublished siblings, an excluded input
that breaks only the packaged build, source/archive mismatches, retry behavior
and workflow wiring. CI also verifies this repository's actual packages without
uploading. Recovery documentation distinguishes helper revisions from the
workflow YAML GitHub selected for a run.

No release, registry upload, tag change, target-matrix expansion or npm pipeline
rewrite is part of this work.
