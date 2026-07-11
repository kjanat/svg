#!/usr/bin/env bash
# Required env: GITHUB_OUTPUT.
set -euo pipefail

GITHUB_OUTPUT="${GITHUB_OUTPUT:?GITHUB_OUTPUT required}"

# cargo's own dry run resolves the intra-workspace dependency DAG; its upload
# order is the publish order. No registry interaction happens on a dry run.
order_list=$(cargo publish --workspace --locked --dry-run --no-verify 2>&1 | sed -n 's/^[[:space:]]*Uploading \([a-z0-9_-]\{1,\}\) v.*/\1/p')
if [[ -z "${order_list}" ]]; then
	echo "error: could not derive publish order from cargo publish --dry-run" >&2
	exit 1
fi

crates=$(jq -c -R '[., inputs] | map({crate: .})' <<<"${order_list}")
echo "publish order: ${crates}"
echo "crates=${crates}" >>"${GITHUB_OUTPUT}"
