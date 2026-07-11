#!/usr/bin/env bash
# Required env: GITHUB_OUTPUT.
set -euo pipefail

GITHUB_OUTPUT="${GITHUB_OUTPUT:?GITHUB_OUTPUT required}"

# `target: .rust` is mandatory — taiki-e and build-packages.ts both derive
# the asset filename `svg-<tag>-<rust>.tar.gz` from it. `vm`-typed targets
# need a dedicated job; filtered out here.
include=$(jq -c '[.targets[] | select(.build != "vm") | {
	target: .rust,
	runner: .runner,
	"build-tool": .build,
	experimental: (.experimental // false)
}]' distribution/npm/targets.json)
echo "include=${include}" >>"${GITHUB_OUTPUT}"
