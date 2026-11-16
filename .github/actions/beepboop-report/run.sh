#!/usr/bin/env bash
# Required env: MANAGER, PACKAGE_NAME, VERSION, RESULT, SKIPPED, REQUIRED, GITHUB_OUTPUT.
set -euo pipefail

MANAGER="${MANAGER:?MANAGER required}"
PACKAGE_NAME="${PACKAGE_NAME:?PACKAGE_NAME required}"
VERSION="${VERSION:?VERSION required}"
VERSION="${VERSION#v}"
RESULT="${RESULT:?RESULT required}"
SKIPPED="${SKIPPED:?SKIPPED required}"
REQUIRED="${REQUIRED:?REQUIRED required}"
GITHUB_OUTPUT="${GITHUB_OUTPUT:?GITHUB_OUTPUT required}"

case "${MANAGER}" in
	cargo | npm) ;;
	*)
		echo "error: unsupported manager '${MANAGER}'" >&2
		exit 1
		;;
esac
case "${REQUIRED}" in
	true | false) ;;
	*)
		echo "error: required must be true or false, got '${REQUIRED}'" >&2
		exit 1
		;;
esac

if [[ "${SKIPPED}" == "true" && "${REQUIRED}" == "false" ]]; then
	outcome=skipped
elif [[ "${SKIPPED}" == "false" && "${RESULT}" == "success" ]]; then
	outcome=succeeded
else
	outcome=failed
fi

publications=$(jq -nc \
	--arg manager "${MANAGER}" \
	--arg name "${PACKAGE_NAME}" \
	--arg version "${VERSION}" \
	--arg outcome "${outcome}" \
	--argjson required "${REQUIRED}" \
	'[{
		id: ("pkg:" + $manager + "/" + ($name | sub("^@"; "%40"))),
		manager: $manager,
		name: $name,
		version: $version,
		outcome: $outcome,
		required: $required
	}]')
echo "publications=${publications}" >>"${GITHUB_OUTPUT}"
printf '%s\n' "${publications}"
