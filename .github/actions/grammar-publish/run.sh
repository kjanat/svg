#!/usr/bin/env bash
# Required env: RELEASE_TAG, GRAMMAR, REGISTRY, DIST_TAG, DRY_RUN, SOURCE_DIR.
# Optional env: NPM_TOKEN (empty relies on OIDC trusted publishing).
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
GRAMMAR="${GRAMMAR:?GRAMMAR required}"
REGISTRY="${REGISTRY:?REGISTRY required}"
DIST_TAG="${DIST_TAG:?DIST_TAG required}"
DRY_RUN="${DRY_RUN:?DRY_RUN required}"
SOURCE_DIR="${SOURCE_DIR:?SOURCE_DIR required}"

# setup-node's .npmrc references NODE_AUTH_TOKEN; only export it when a
# bootstrap token was actually provided, so a tokenless run falls through
# to OIDC trusted publishing instead of sending an empty _authToken.
if [[ -n "${NPM_TOKEN-}" ]]; then
	export NODE_AUTH_TOKEN="${NPM_TOKEN}"
fi

EXPECTED_VERSION="${RELEASE_TAG#v}"
grammar_dir="${SOURCE_DIR}/grammars/${GRAMMAR}"
catalog_json="${SOURCE_DIR}/package.json"

if [[ ! -f "${grammar_dir}/package.json" ]]; then
	echo "error: ${grammar_dir}/package.json missing" >&2
	exit 1
fi

# The grammar must be the package it claims to be, and only from the
# allowlist in targets.json — a tampered checkout must not publish under an
# arbitrary name.
actual_name=$(jq -r .name "${grammar_dir}/package.json")
if [[ "${actual_name}" != "${GRAMMAR}" ]]; then
	echo "error: ${grammar_dir}/package.json declares name '${actual_name}', expected '${GRAMMAR}'" >&2
	exit 1
fi
if ! jq -e --arg g "${GRAMMAR}" '(.grammars // []) | index($g) != null' distribution/npm/targets.json >/dev/null; then
	echo "error: grammar '${GRAMMAR}' is not listed in targets.json grammars[]" >&2
	exit 1
fi
if [[ -e "${grammar_dir}/.npmrc" ]]; then
	echo "error: ${grammar_dir}/.npmrc is forbidden (could redirect publish)" >&2
	exit 1
fi
if jq -e 'has("publishConfig")' "${grammar_dir}/package.json" >/dev/null; then
	echo "error: ${grammar_dir}/package.json has publishConfig (could redirect publish)" >&2
	exit 1
fi

# Stamp the release version and resolve Bun `catalog:` dependency references
# against the workspace root catalog — npm publishes the manifest verbatim,
# and a literal "catalog:" range breaks every consumer install.
resolved=$(jq --arg v "${EXPECTED_VERSION}" --slurpfile root "${catalog_json}" '
	($root[0].catalog // {}) as $catalog
	| .version = $v
	| reduce ("dependencies", "devDependencies", "peerDependencies", "optionalDependencies") as $field (.;
		if has($field) then
			.[$field] |= with_entries(
				if .value == "catalog:" then
					.value = ($catalog[.key] // error("no catalog entry for " + .key))
				else . end
			)
		else . end
	)
' "${grammar_dir}/package.json")
printf '%s\n' "${resolved}" >"${grammar_dir}/package.json"

if grep -q '"catalog:"' "${grammar_dir}/package.json"; then
	echo "error: unresolved catalog: reference survived in ${GRAMMAR}/package.json" >&2
	exit 1
fi

echo "package-url=https://npm.im/package/${GRAMMAR}/v/${EXPECTED_VERSION}" >>"${GITHUB_OUTPUT:-/dev/null}"

# Skip if already published — npm versions are immutable, so reruns after a
# partial publish would otherwise fail here.
view_status=0
published=$(timeout 120s npm view "${GRAMMAR}@${EXPECTED_VERSION}" --registry "${REGISTRY}" version 2>/dev/null) || view_status=$?
if [[ ${view_status} -eq 124 ]]; then
	echo "error: 'npm view ${GRAMMAR}@${EXPECTED_VERSION}' timed out after 120s" >&2
	exit 1
fi
if [[ "${published}" == "${EXPECTED_VERSION}" ]]; then
	echo "skip ${GRAMMAR}@${EXPECTED_VERSION}: already published"
	exit 0
fi

# npm@11 pinned: npm@12 currently fails any publish with provenance.
# --ignore-scripts skips the repo-side prepare/postinstall hooks; the
# consumer-side install scripts in the manifest ship untouched.
args=(publish --registry "${REGISTRY}" --access public --tag "${DIST_TAG}" --ignore-scripts --provenance)
if [[ "${DRY_RUN}" == "true" ]]; then args+=(--dry-run); fi
echo "+ npx -y npm@11 ${args[*]}  (cwd: ${grammar_dir})"
status=0
output=$(cd "${grammar_dir}" && timeout 300s npx -y npm@11 "${args[@]}" 2>&1) || status=$?
if [[ "${status}" -eq 124 ]]; then
	printf '%s\n' "${output}" >&2
	echo "error: publish of ${GRAMMAR}@${EXPECTED_VERSION} timed out" >&2
	exit 1
fi
if [[ "${status}" -ne 0 ]]; then
	printf '%s\n' "${output}" >&2
	if grep -Eiq 'EPUBLISHCONFLICT|cannot publish over the previously published versions' <<<"${output}"; then
		echo "skip ${GRAMMAR}@${EXPECTED_VERSION}: already published (race with concurrent publisher)"
		exit 0
	fi
	exit "${status}"
fi
printf '%s\n' "${output}"
