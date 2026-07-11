#!/usr/bin/env bash
# Required env: RELEASE_TAG, GITHUB_REPOSITORY, GH_TOKEN.
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
GITHUB_REPOSITORY="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY required}"
GH_TOKEN="${GH_TOKEN:?GH_TOKEN required}"
shopt -s nullglob

# Scrub before fetch: stale files from a previous tag would pass
# checksum verification but be wrong-version. Hosted runners get fresh
# workspaces; self-hosted runners and local invocations don't.
rm -rf distribution/npm/downloads
mkdir -p distribution/npm/downloads
gh release download "${RELEASE_TAG}" \
	--repo "${GITHUB_REPOSITORY}" \
	--pattern 'svg-*-*.tar.gz' \
	--pattern 'svg-*-*.sha256' \
	--dir distribution/npm/downloads
ls -la distribution/npm/downloads

cd distribution/npm/downloads

tarballs=(*.tar.gz)
sums=(*.sha256)

if [[ "${#tarballs[@]}" -eq 0 ]]; then
	echo "error: no tarballs downloaded for ${RELEASE_TAG}" >&2
	exit 1
fi
for t in "${tarballs[@]}"; do
	expected="${t%.tar.gz}.sha256"
	if [[ ! -f "${expected}" ]]; then
		echo "error: tarball ${t} has no matching ${expected}" >&2
		exit 1
	fi
done

# Each .sha256 must reference a tarball matching its own basename —
# defends against a swapped reference leaving a tarball unchecked.
for s in "${sums[@]}"; do
	inner=$(awk '{sub(/^\*/, "", $2); print $2}' "${s}")
	expected="${s%.sha256}.tar.gz"
	if [[ ! -f "${expected}" ]]; then
		echo "error: checksum file ${s} has no matching ${expected}" >&2
		exit 1
	fi
	if [[ "${inner}" != "${expected}" ]]; then
		echo "error: ${s} references '${inner}', expected '${expected}'" >&2
		exit 1
	fi
done

for sum in "${sums[@]}"; do
	sha256sum -c --status "${sum}"
done
