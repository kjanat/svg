#!/usr/bin/env bash
# Required env: GITHUB_OUTPUT.
set -euo pipefail

GITHUB_OUTPUT="${GITHUB_OUTPUT:?GITHUB_OUTPUT required}"

cd "${SOURCE_DIR:-.}"

# cargo's own dry run resolves the intra-workspace dependency DAG; its upload
# order is the publish order. No registry interaction happens on a dry run.
# Run and scrape separately: a pipe would eat cargo's stderr on failure.
dry_run_output=$(cargo publish --workspace --locked --dry-run --no-verify 2>&1) || {
	printf '%s\n' "${dry_run_output}" >&2
	echo "error: cargo publish --dry-run failed" >&2
	exit 1
}
order_list=$(sed -n 's/^[[:space:]]*Uploading \([a-z0-9_-]\{1,\}\) v.*/\1/p' <<<"${dry_run_output}")
if [[ -z "${order_list}" ]]; then
	printf '%s\n' "${dry_run_output}" >&2
	echo "error: could not derive publish order from cargo publish --dry-run output above" >&2
	exit 1
fi

crates=$(jq -c -R '[., inputs] | map({crate: .})' <<<"${order_list}")
echo "publish order: ${crates}"
echo "crates=${crates}" >>"${GITHUB_OUTPUT}"
