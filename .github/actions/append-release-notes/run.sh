#!/usr/bin/env bash
# Required env: RELEASE_TAG, GITHUB_REPOSITORY, GH_TOKEN (contents: write).
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
GITHUB_REPOSITORY="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY required}"
GH_TOKEN="${GH_TOKEN:?GH_TOKEN required}"

# The generated section lives between stable delimiters so a retried or
# re-dispatched run replaces it instead of appending a duplicate copy.
start_marker='<!-- generated-notes-start -->'
end_marker='<!-- generated-notes-end -->'

our_body="$(gh release view "${RELEASE_TAG}" \
	--repo "${GITHUB_REPOSITORY}" --json body --jq .body)"
gen_body="$(gh api \
	"repos/${GITHUB_REPOSITORY}/releases/generate-notes" \
	-f tag_name="${RELEASE_TAG}" --jq .body)"

# Strip a previous generated section (delimiters included) before inserting
# the current one.
manual_body=$(awk -v start="${start_marker}" -v end="${end_marker}" '
	$0 == start { skipping = 1; next }
	$0 == end { skipping = 0; next }
	!skipping { print }
' <<<"${our_body}")

combined="$(mktemp)"
printf '%s\n\n%s\n%s\n%s\n' "${manual_body}" "${start_marker}" "${gen_body}" "${end_marker}" >"${combined}"
gh release edit "${RELEASE_TAG}" \
	--repo "${GITHUB_REPOSITORY}" --notes-file "${combined}"
rm -f "${combined}"
