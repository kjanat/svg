#!/usr/bin/env bash
# Required env: RELEASE_TAG, DIST_TAG, DRY_RUN, REGISTRY, GITHUB_OUTPUT.
# Optional env: ONLY_PACKAGE (single-package mode for matrix jobs),
# NPM_TOKEN (bootstrap auth; empty relies on OIDC trusted publishing).
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
DIST_TAG="${DIST_TAG:?DIST_TAG required}"
DRY_RUN="${DRY_RUN:?DRY_RUN required}"
REGISTRY="${REGISTRY:?REGISTRY required}"
GITHUB_OUTPUT="${GITHUB_OUTPUT:?GITHUB_OUTPUT required}"
shopt -s nullglob

ONLY_PACKAGE="${ONLY_PACKAGE-}"

# setup-node's .npmrc references NODE_AUTH_TOKEN; only export it when a
# bootstrap token was actually provided, so a tokenless run falls through
# to OIDC trusted publishing instead of sending an empty _authToken.
if [[ -n "${NPM_TOKEN-}" ]]; then
	export NODE_AUTH_TOKEN="${NPM_TOKEN}"
fi

TARGETS_JSON="${GITHUB_WORKSPACE:-.}/distribution/npm/targets.json"
SCOPE=$(jq -r '.scope' "${TARGETS_JSON}")
# Assign before mapfile so a jq failure aborts under `set -e`; guard
# empty output, which `<<<` would turn into a phantom "" element.
facade_list=$(jq -r '.facades[].name' "${TARGETS_JSON}")
# One platform package per facade × target: <facade.pkg>-<target.pkg>.
required_list=$(jq -r '.facades[] as $f | .targets[] | select((.experimental // false) | not) | $f.pkg + "-" + .pkg' "${TARGETS_JSON}")
optional_list=$(jq -r '.facades[] as $f | .targets[] | select(.experimental // false) | $f.pkg + "-" + .pkg' "${TARGETS_JSON}")
FACADES=()
REQUIRED_PLATFORMS=()
OPTIONAL_PLATFORMS=()
[[ -n "${facade_list}" ]] && mapfile -t FACADES <<<"${facade_list}"
[[ -n "${required_list}" ]] && mapfile -t REQUIRED_PLATFORMS <<<"${required_list}"
[[ -n "${optional_list}" ]] && mapfile -t OPTIONAL_PLATFORMS <<<"${optional_list}"
EXPECTED_VERSION="${RELEASE_TAG#v}"

# publish_allowed publishes a single package from the artifact when it exists
# and its package.json matches the expected name and version, skips optional
# or already-published packages, and exits on integrity or policy failures.
publish_allowed() {
	local dir="$1" expected_name="$2" required="$3"
	local actual_name version published

	if [[ ! -d "${dir}" ]]; then
		if [[ "${required}" == "true" ]]; then
			echo "error: required package ${expected_name} missing from artifact" >&2
			exit 1
		fi
		echo "skip ${expected_name}: not in artifact (optional / experimental platform)"
		return 0
	fi
	if [[ ! -f "${dir}/package.json" ]]; then
		echo "error: ${dir}/package.json missing" >&2
		exit 1
	fi

	# Reject per-package registry overrides. A malicious build could
	# drop a .npmrc or set publishConfig in package.json to redirect
	# the publish to an attacker-controlled registry.
	# CLI --registry does NOT override scoped publishConfig.registry,
	# so the rejection here is the primary defense; the explicit
	# --registry flag below is belt-and-suspenders for non-scoped
	# overrides.
	if [[ -e "${dir}/.npmrc" ]]; then
		echo "error: ${dir}/.npmrc is forbidden (could redirect publish)" >&2
		exit 1
	fi
	if jq -e 'has("publishConfig")' "${dir}/package.json" >/dev/null; then
		echo "error: ${dir}/package.json has publishConfig (could redirect publish)" >&2
		exit 1
	fi

	actual_name=$(jq -r .name "${dir}/package.json")
	if [[ "${actual_name}" != "${expected_name}" ]]; then
		echo "error: ${dir}/package.json declares name '${actual_name}', expected '${expected_name}'" >&2
		exit 1
	fi
	version=$(jq -r .version "${dir}/package.json")
	if [[ "${version}" != "${EXPECTED_VERSION}" ]]; then
		echo "error: ${dir}/package.json declares version '${version}', expected '${EXPECTED_VERSION}' (from tag ${RELEASE_TAG})" >&2
		exit 1
	fi

	# optionalDependencies validation. Facades are the only packages that
	# legitimately ship optionalDependencies (one entry per built platform
	# package, all pinned to EXPECTED_VERSION). Platform packages must have
	# none — a tampered platform package could otherwise smuggle attacker-
	# controlled deps that npm would happily install transitively.
	if [[ " ${FACADES[*]} " == *" ${expected_name} "* ]]; then
		# A facade may only reference ITS OWN tool's platform packages
		# (prefix <facade.pkg>-), never a sibling tool's.
		local facade_pkg
		facade_pkg=$(jq -r --arg n "${expected_name}" '.facades[] | select(.name == $n) | .pkg' "${TARGETS_JSON}")
		if [[ -z "${facade_pkg}" ]]; then
			echo "error: facade ${expected_name} has no pkg prefix in targets.json" >&2
			exit 1
		fi

		local dep_name dep_version platform dep_entries expected_dep_set=" ${REQUIRED_PLATFORMS[*]} ${OPTIONAL_PLATFORMS[*]} "
		dep_entries=$(jq -r '(.optionalDependencies // {}) | to_entries[] | "\(.key)\t\(.value)"' "${dir}/package.json")
		while IFS=$'\t' read -r dep_name dep_version; do
			[[ -z "${dep_name}" ]] && continue
			if [[ "${dep_name}" != "${SCOPE}/${facade_pkg}-"* ]]; then
				echo "error: facade optionalDependencies entry '${dep_name}' not under '${SCOPE}/${facade_pkg}-'" >&2
				exit 1
			fi
			platform="${dep_name#"${SCOPE}/"}"
			if [[ "${expected_dep_set}" != *" ${platform} "* ]]; then
				echo "error: facade optionalDependencies references unexpected package '${dep_name}'" >&2
				exit 1
			fi
			if [[ "${dep_version}" != "${EXPECTED_VERSION}" ]]; then
				echo "error: facade optionalDependencies['${dep_name}'] = '${dep_version}', expected '${EXPECTED_VERSION}'" >&2
				exit 1
			fi
		done <<<"${dep_entries}"

		# This facade's required platforms must all be referenced.
		for platform in "${REQUIRED_PLATFORMS[@]}"; do
			[[ "${platform}" == "${facade_pkg}-"* ]] || continue
			if ! jq -e --arg dep "${SCOPE}/${platform}" '(.optionalDependencies // {}) | has($dep)' "${dir}/package.json" >/dev/null; then
				echo "error: facade optionalDependencies missing required package '${SCOPE}/${platform}'" >&2
				exit 1
			fi
		done
	else
		if jq -e '(.optionalDependencies // {}) | length > 0' "${dir}/package.json" >/dev/null; then
			echo "error: ${dir}/package.json has optionalDependencies; only facades may declare any" >&2
			exit 1
		fi
	fi

	# Surface the package URL to the workflow. Repeated writes to the
	# same key resolve last-wins in GITHUB_OUTPUT; in single-package
	# (matrix) mode each job writes exactly one.
	echo "package-url=https://npm.im/package/${actual_name}/v/${version}" >>"${GITHUB_OUTPUT}"

	# Skip if already published — npm versions are immutable, so reruns
	# after a partial publish would otherwise fail on the first
	# sub-package that already published. Bound the probe at 120s so a
	# hung registry can't stall the whole publish job. Non-timeout
	# failures (e.g. E404 when the version isn't published yet) drop
	# through to the publish step, which surfaces real errors.
	local view_status=0
	published=$(timeout 120s npm view "${actual_name}@${version}" --registry "${REGISTRY}" version 2>/dev/null) || view_status=$?
	if [[ ${view_status} -eq 124 ]]; then
		echo "error: 'npm view ${actual_name}@${version}' timed out after 120s" >&2
		return 1
	fi
	if [[ "${published}" == "${version}" ]]; then
		echo "skip ${actual_name}@${version}: already published"
		return 0
	fi

	# npm@11 pinned: npm@12 currently fails any publish with provenance.
	local args=(publish --registry "${REGISTRY}" --access public --tag "${DIST_TAG}" --ignore-scripts --provenance)
	if [[ "${DRY_RUN}" == "true" ]]; then args+=(--dry-run); fi
	echo "+ npx -y npm@11 ${args[*]}  (cwd: ${dir})"
	# Tolerate the TOCTOU race between the npm view check above and
	# this publish: if another actor publishes the same version in
	# the gap, npm exits with EPUBLISHCONFLICT and we treat that as a
	# no-op.
	#
	# The `|| status=$?` form is required: under `set -e`,
	# `output=$(cmd); status=$?` would exit on a failing cmd before
	# status was captured, and `if ! output=$(cmd); then status=$?`
	# captures the negation status (always 0), not npm's real exit
	# code — silently masking real publish failures.
	local output status=0
	output=$(cd "${dir}" && timeout 120s npx -y npm@11 "${args[@]}" 2>&1) || status=$?
	if [[ "${status}" -eq 124 ]]; then
		printf '%s\n' "${output}" >&2
		echo "error: 'npx -y npm@11 publish' for ${actual_name}@${version} timed out after 120s" >&2
		return 1
	fi
	if [[ "${status}" -ne 0 ]]; then
		printf '%s\n' "${output}" >&2
		if grep -Eiq 'EPUBLISHCONFLICT|cannot publish over the previously published versions' <<<"${output}"; then
			echo "skip ${actual_name}@${version}: already published (race with concurrent publisher)"
			return 0
		fi
		return "${status}"
	fi
	printf '%s\n' "${output}"
}

# Refuse to proceed if the artifact contains anything outside the
# allowlist — that's either a misconfiguration or an attack.
allowed_set=" ${FACADES[*]} ${REQUIRED_PLATFORMS[*]} ${OPTIONAL_PLATFORMS[*]} "
for dir in distribution/npm/dist/*/; do
	base=$(basename "${dir%/}")
	if [[ "${allowed_set}" != *" ${base} "* ]]; then
		echo "error: artifact contains unexpected directory '${base}' (not in allowlist)" >&2
		exit 1
	fi
done

# 0644 binaries EACCES at spawn — fail loud before publishing.
for platform in "${REQUIRED_PLATFORMS[@]}" "${OPTIONAL_PLATFORMS[@]}"; do
	for bin in "distribution/npm/dist/${platform}/bin/"*; do
		if [[ ! -x "${bin}" ]]; then
			echo "error: ${bin} lost its executable bit in the artifact handoff" >&2
			exit 1
		fi
	done
done

if [[ -n "${ONLY_PACKAGE}" ]]; then
	if [[ " ${FACADES[*]} " == *" ${ONLY_PACKAGE} "* ]]; then
		publish_allowed "distribution/npm/dist/${ONLY_PACKAGE}" "${ONLY_PACKAGE}" true
	elif [[ " ${REQUIRED_PLATFORMS[*]} " == *" ${ONLY_PACKAGE} "* ]]; then
		publish_allowed "distribution/npm/dist/${ONLY_PACKAGE}" "${SCOPE}/${ONLY_PACKAGE}" true
	elif [[ " ${OPTIONAL_PLATFORMS[*]} " == *" ${ONLY_PACKAGE} "* ]]; then
		publish_allowed "distribution/npm/dist/${ONLY_PACKAGE}" "${SCOPE}/${ONLY_PACKAGE}" false
	else
		echo "error: ONLY_PACKAGE '${ONLY_PACKAGE}' is not a facade or a known platform" >&2
		exit 1
	fi
	exit 0
fi

# Tier-1/2 are always required: the artifact is built by release.yml's
# build-dist (where missing tier-1/2 tarballs already fail loud), so a
# missing dir here means the artifact was tampered with or the build
# silently dropped a target — either case warrants a hard fail.
# Sub-packages first so the facades' optionalDependencies resolve on install.
for platform in "${REQUIRED_PLATFORMS[@]}"; do
	publish_allowed "distribution/npm/dist/${platform}" "${SCOPE}/${platform}" true
done
for platform in "${OPTIONAL_PLATFORMS[@]}"; do
	publish_allowed "distribution/npm/dist/${platform}" "${SCOPE}/${platform}" false
done

# Facades are mandatory either way — no point publishing a half-empty
# set of platform packages with no entry points.
for facade in "${FACADES[@]}"; do
	publish_allowed "distribution/npm/dist/${facade}" "${facade}" true
done
