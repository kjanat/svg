#!/usr/bin/env bash
# Required env: RELEASE_TAG, GITHUB_REPOSITORY, GITHUB_OUTPUT, GH_TOKEN (actions: read).
# Optional env: INPUT_RUN_ID.
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
GITHUB_REPOSITORY="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY required}"
GITHUB_OUTPUT="${GITHUB_OUTPUT:?GITHUB_OUTPUT required}"
GH_TOKEN="${GH_TOKEN:?GH_TOKEN required}"

if [[ -n "${INPUT_RUN_ID-}" ]]; then
	run_id="${INPUT_RUN_ID}"
else
	run_id=$(gh run list \
		--repo "${GITHUB_REPOSITORY}" \
		--workflow=release.yml \
		--branch="${RELEASE_TAG}" \
		--status=success \
		--limit=1 \
		--json databaseId \
		--jq '.[0].databaseId')
fi
if [[ -z "${run_id}" || "${run_id}" == "null" ]]; then
	echo "error: no run-id resolvable for ${RELEASE_TAG}" >&2
	exit 1
fi
# Numeric guard — blocks GITHUB_OUTPUT injection.
if ! [[ "${run_id}" =~ ^[0-9]+$ ]]; then
	echo "error: run-id '${run_id}' is not a positive integer" >&2
	exit 1
fi
echo "run-id=${run_id}" >>"${GITHUB_OUTPUT}"
