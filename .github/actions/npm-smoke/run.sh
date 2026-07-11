#!/usr/bin/env bash
# Required env: RELEASE_TAG. Optional env: HOST_PKG (default linux-x64-gnu).
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
shopt -s nullglob

expected_version="${RELEASE_TAG#v}"
targets_json="${GITHUB_WORKSPACE:-.}/distribution/npm/targets.json"
scope=$(jq -r '.scope' "${targets_json}")
bundle_name=$(jq -r '.bundle.name // empty' "${targets_json}")
host_pkg="${HOST_PKG:-linux-x64-gnu}"

# dist/ directory for a package name, npm-pack style: strip the scope's "@",
# "/" becomes "-" (mirrors npm-publish/run.sh).
dir_for() {
	local name="${1#@}"
	printf '%s' "${name//\//-}"
}

scratch=$(mktemp -d)
trap 'rm -rf "${scratch-}"' EXIT

assert_version() {
	local label="$1" out word found=''
	shift
	out=$("$@")
	# Exact whitespace-delimited token match: a substring test would accept
	# e.g. 0.1.00 or unrelated text that merely contains the version.
	for word in ${out}; do
		if [[ "${word}" == "${expected_version}" ]]; then
			found=1
			break
		fi
	done
	if [[ -z "${found}" ]]; then
		echo "error: ${label}: expected version token ${expected_version}, got: ${out}" >&2
		exit 1
	fi
	echo "ok ${label}: ${out}"
}

# name<TAB>pkg rows. Primary facades and their scoped twins are smoked in
# separate apps: twins ship identical bin names, which would collide in one
# node_modules/.bin.
primary_rows=$(jq -r '.facades[] | "\(.name)\t\(.pkg)"' "${targets_json}")
twin_rows=$(jq -r '.facades[] | .pkg as $p | (.alsoPublishAs // [])[] | "\(.)\t\($p)"' "${targets_json}")

smoke_app() {
	local app="$1" rows="$2"
	if [[ -z "${rows}" ]]; then
		return 0
	fi
	local pack_dir="${scratch}/${app}-pkgs"
	mkdir -p "${pack_dir}" "${scratch}/${app}"

	local facade_name facade_pkg facade_dir
	while IFS=$'\t' read -r facade_name facade_pkg; do
		facade_dir=$(dir_for "${facade_name}")
		(cd "distribution/npm/dist/${facade_pkg}-${host_pkg}" && npm pack --pack-destination "${pack_dir}" >/dev/null)
		(cd "distribution/npm/dist/${facade_dir}" && npm pack --pack-destination "${pack_dir}" >/dev/null)
	done <<<"${rows}"

	(cd "${scratch}/${app}" && npm install --no-audit --no-fund --ignore-scripts "${pack_dir}"/*.tgz)

	local platform_dir raw raw_bins bin_targets target
	while IFS=$'\t' read -r facade_name facade_pkg; do
		platform_dir="${scratch}/${app}/node_modules/${scope}/${facade_pkg}-${host_pkg}"

		# Raw binaries — the files whose exec bits the artifact handoff used to drop.
		raw_bins=("${platform_dir}/bin/"*)
		if [[ "${#raw_bins[@]}" -eq 0 ]]; then
			echo "error: no binaries under ${platform_dir}/bin/" >&2
			exit 1
		fi
		for raw in "${raw_bins[@]}"; do
			assert_version "${app} raw $(basename "${raw}")" "${raw}" --version
		done

		# Every bin target, whatever the bin field's shape.
		bin_targets=$(jq -r '.bin | if type == "string" then [.] else [.[]] end | .[]' "${platform_dir}/package.json")
		while IFS= read -r target; do
			assert_version "${app} bin ${facade_pkg}-${host_pkg}/${target}" "${platform_dir}/${target}" --version
		done <<<"${bin_targets}"
	done <<<"${rows}"

	# Linked bins (facade shims + platform bins).
	local linked bin
	linked=("${scratch}/${app}/node_modules/.bin/"*)
	if [[ "${#linked[@]}" -eq 0 ]]; then
		echo "error: no bins linked in ${app} scratch install" >&2
		exit 1
	fi
	for bin in "${linked[@]}"; do
		assert_version "${app} $(basename "${bin}")" "${bin}" --version
	done
}

smoke_app primary "${primary_rows}"
smoke_app twins "${twin_rows}"

# Bundle: install it with its (locally packed) facade dependencies and run
# each of its shims directly — its bin names intentionally shadow the
# facades' own, so .bin link assertions would be ambiguous here.
if [[ -n "${bundle_name}" ]]; then
	bundle_pack="${scratch}/bundle-pkgs"
	mkdir -p "${bundle_pack}" "${scratch}/bundle-app"

	bundle_src_dir=$(dir_for "${bundle_name}")
	(cd "distribution/npm/dist/${bundle_src_dir}" && npm pack --pack-destination "${bundle_pack}" >/dev/null)
	while IFS=$'\t' read -r facade_name facade_pkg; do
		facade_dir=$(dir_for "${facade_name}")
		(cd "distribution/npm/dist/${facade_pkg}-${host_pkg}" && npm pack --pack-destination "${bundle_pack}" >/dev/null)
		(cd "distribution/npm/dist/${facade_dir}" && npm pack --pack-destination "${bundle_pack}" >/dev/null)
	done <<<"${primary_rows}"

	(cd "${scratch}/bundle-app" && npm install --no-audit --no-fund --ignore-scripts "${bundle_pack}"/*.tgz)

	bundle_dir="${scratch}/bundle-app/node_modules/${bundle_name}"
	bundle_bins=$(jq -r '.bin | if type == "string" then [.] else [.[]] end | unique | .[]' "${bundle_dir}/package.json")
	while IFS= read -r target; do
		assert_version "bundle ${target}" node "${bundle_dir}/${target}" --version
	done <<<"${bundle_bins}"
fi
