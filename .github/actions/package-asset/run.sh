#!/usr/bin/env bash
# Required env: RELEASE_TAG, TARGET, BIN_DIR, GH_TOKEN (contents: write).
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
TARGET="${TARGET:?TARGET required}"
BIN_DIR="${BIN_DIR:?BIN_DIR required}"
GH_TOKEN="${GH_TOKEN:?GH_TOKEN required}"

# Defensive: this path doesn't handle .exe binaries. Bail loudly if a
# future matrix config routes a Windows target here.
if [[ "${TARGET}" == *windows* ]]; then
	echo "error: package-asset does not handle Windows targets (.exe naming)" >&2
	exit 1
fi

archive_basename="svg-${RELEASE_TAG}-${TARGET}"
archive="${archive_basename}.tar.gz"
# `<basename>.sha256`, NOT `<basename>.tar.gz.sha256`. Matches the
# convention `taiki-e/upload-rust-binary-action` uses, which is what
# download-archives enforces (`expected="${t%.tar.gz}.sha256"`).
checksum="${archive_basename}.sha256"

bins=$(jq -r '.binaries[]' distribution/npm/targets.json)

staging=$(mktemp -d)
trap 'rm -rf "${staging-}"' EXIT

# Lay out the contents the way upload-rust-binary-action does with
# `leading_dir: false` and `include: README.md,LICENSE`: every file at
# the tarball root, no wrapper directory.
staged=()
while IFS= read -r bin; do
	src="${BIN_DIR}/${bin}"
	if [[ ! -f "${src}" ]]; then
		echo "error: ${src} not found — build step did not produce ${bin}" >&2
		exit 1
	fi
	cp "${src}" "${staging}/${bin}"
	chmod +x "${staging}/${bin}"
	staged+=("${bin}")
done <<<"${bins}"
cp README.md LICENSE "${staging}/"

tar -C "${staging}" -czf "${archive}" "${staged[@]}" README.md LICENSE
sha256sum "${archive}" >"${checksum}"

gh release upload "${RELEASE_TAG}" "${archive}" "${checksum}" --clobber
