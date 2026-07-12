#!/usr/bin/env bash
# Required env: GITHUB_OUTPUT.
set -euo pipefail

GITHUB_OUTPUT="${GITHUB_OUTPUT:?GITHUB_OUTPUT required}"

# One platform package per facade × target: <facade.pkg>-<target.pkg>.
platforms=$(jq -c '[.facades[] as $f | .targets[] | {pkg: ($f.pkg + "-" + .pkg), experimental: (.experimental // false)}]' distribution/npm/targets.json)
# One publish job per facade publish name (scoped twins included).
facades=$(jq -c '[.facades[] | .name, (.alsoPublishAs // [])[] | {pkg: .}]' distribution/npm/targets.json)
# Unscoped alias shims publish after their canonical facades.
shims=$(jq -c '[.facades[] | select(.shim) | {pkg: .shim}]' distribution/npm/targets.json)
bundle=$(jq -r '.bundle.name // empty' distribution/npm/targets.json)
# Tree-sitter grammar node packages, published from the tag's grammars/ tree.
grammars=$(jq -c '[(.grammars // [])[] | {pkg: .}]' distribution/npm/targets.json)

{
	echo "platforms=${platforms}"
	echo "facades=${facades}"
	echo "shims=${shims}"
	echo "bundle=${bundle}"
	echo "grammars=${grammars}"
} | tee -a "${GITHUB_OUTPUT}"
