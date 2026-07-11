#!/usr/bin/env bash
# Required env: RELEASE_TAG, TARGET, GITHUB_REPOSITORY, GH_TOKEN (contents: read).
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
TARGET="${TARGET:?TARGET required}"
GITHUB_REPOSITORY="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY required}"
GH_TOKEN="${GH_TOKEN:?GH_TOKEN required}"

archive_basename="svg-${RELEASE_TAG}-${TARGET}"
archive="${archive_basename}.tar.gz"
checksum="${archive_basename}.sha256"

# Retry the listing a few times: a transient network blip on the read
# shouldn't fail a leg that genuinely uploaded.
missing=()
for attempt in 1 2 3; do
	# Read line-by-line rather than `mapfile`: macOS runners ship
	# bash 3.2, which predates the builtin.
	assets=()
	while IFS= read -r asset; do
		assets+=("${asset}")
	done < <(
		# `|| true`: a failed listing leaves `assets` empty, which the
		# retry loop treats as missing and re-fetches.
		gh release view "${RELEASE_TAG}" \
			--repo "${GITHUB_REPOSITORY}" \
			--json assets \
			--jq '.assets[].name' || true
	)

	missing=()
	for want in "${archive}" "${checksum}"; do
		found=''
		for have in ${assets[@]+"${assets[@]}"}; do
			if [[ "${have}" == "${want}" ]]; then
				found=1
				break
			fi
		done
		if [[ -z "${found}" ]]; then
			missing+=("${want}")
		fi
	done

	if [[ "${#missing[@]}" -eq 0 ]]; then
		break
	fi
	if [[ "${attempt}" -lt 3 ]]; then
		sleep "$((attempt * 2))"
	fi
done

if [[ "${#missing[@]}" -gt 0 ]]; then
	echo "error: build+upload for ${TARGET} reported success but these assets are not on release ${RELEASE_TAG}:" >&2
	printf '  - %s\n' "${missing[@]}" >&2
	echo "the build step produced no artifact — inspect its log for a silent no-op." >&2
	exit 1
fi

echo "verified ${archive} and ${checksum} on release ${RELEASE_TAG}"
