#!/usr/bin/env bash
# Required env: RELEASE_TAG, GITHUB_REPOSITORY, GH_TOKEN (contents: write).
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
GITHUB_REPOSITORY="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY required}"
GH_TOKEN="${GH_TOKEN:?GH_TOKEN required}"

our_body="$(gh release view "${RELEASE_TAG}" \
	--repo "${GITHUB_REPOSITORY}" --json body --jq .body)"
gen_body="$(gh api \
	"repos/${GITHUB_REPOSITORY}/releases/generate-notes" \
	-f tag_name="${RELEASE_TAG}" --jq .body)"
combined="$(mktemp)"
printf '%s\n\n%s\n' "${our_body}" "${gen_body}" >"${combined}"
gh release edit "${RELEASE_TAG}" \
	--repo "${GITHUB_REPOSITORY}" --notes-file "${combined}"
rm -f "${combined}"
