#!/usr/bin/env bash
# Required env: GITHUB_OUTPUT.
set -euo pipefail

GITHUB_OUTPUT="${GITHUB_OUTPUT:?GITHUB_OUTPUT required}"

# One platform package per facade × target: <facade.pkg>-<target.pkg>.
platforms=$(jq -c '[.facades[] as $f | .targets[] | {pkg: ($f.pkg + "-" + .pkg), experimental: (.experimental // false)}]' distribution/npm/targets.json)
facades=$(jq -c '[.facades[] | {pkg: .name}]' distribution/npm/targets.json)

{
	echo "platforms=${platforms}"
	echo "facades=${facades}"
} | tee -a "${GITHUB_OUTPUT}"
