#!/usr/bin/env bash
# Required env: RELEASE_TAG. Optional env: HOST_PKG (default linux-x64-gnu).
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
shopt -s nullglob

expected_version="${RELEASE_TAG#v}"
targets_json="${GITHUB_WORKSPACE:-.}/distribution/npm/targets.json"
scope=$(jq -r '.scope' "${targets_json}")
# name<TAB>pkg per facade; each facade has its own <pkg>-<platform> sub-package.
facade_rows=$(jq -r '.facades[] | "\(.name)\t\(.pkg)"' "${targets_json}")
host_pkg="${HOST_PKG:-linux-x64-gnu}"

scratch=$(mktemp -d)
trap 'rm -rf "${scratch-}"' EXIT

while IFS=$'\t' read -r facade_name facade_pkg; do
	(cd "distribution/npm/dist/${facade_pkg}-${host_pkg}" && npm pack --pack-destination "${scratch}" >/dev/null)
	(cd "distribution/npm/dist/${facade_name}" && npm pack --pack-destination "${scratch}" >/dev/null)
done <<<"${facade_rows}"

mkdir "${scratch}/app"
(cd "${scratch}/app" && npm install --no-audit --no-fund --ignore-scripts "${scratch}"/*.tgz)

assert_version() {
	local label="$1" out
	shift
	out=$("$@")
	if [[ "${out}" != *"${expected_version}"* ]]; then
		echo "error: ${label}: expected ${expected_version}, got: ${out}" >&2
		exit 1
	fi
	echo "ok ${label}: ${out}"
}

while IFS=$'\t' read -r facade_name facade_pkg; do
	platform_dir="${scratch}/app/node_modules/${scope}/${facade_pkg}-${host_pkg}"

	# Raw binaries — the files whose exec bits the artifact handoff used to drop.
	raw_bins=("${platform_dir}/bin/"*)
	if [[ "${#raw_bins[@]}" -eq 0 ]]; then
		echo "error: no binaries under ${platform_dir}/bin/" >&2
		exit 1
	fi
	for raw in "${raw_bins[@]}"; do
		assert_version "raw $(basename "${raw}")" "${raw}" --version
	done

	# Every bin target, whatever the bin field's shape.
	bin_targets=$(jq -r '.bin | if type == "string" then [.] else [.[]] end | .[]' "${platform_dir}/package.json")
	while IFS= read -r target; do
		assert_version "bin ${facade_pkg}-${host_pkg}/${target}" "${platform_dir}/${target}" --version
	done <<<"${bin_targets}"
done <<<"${facade_rows}"

# Linked bins (facade shims + platform bins).
linked=("${scratch}/app/node_modules/.bin/"*)
if [[ "${#linked[@]}" -eq 0 ]]; then
	echo "error: no bins linked in scratch install" >&2
	exit 1
fi
for bin in "${linked[@]}"; do
	assert_version "$(basename "${bin}")" "${bin}" --version
done
