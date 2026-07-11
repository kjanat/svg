#!/usr/bin/env bash
# Required env: RELEASE_TAG, META_PACKAGE, GITHUB_OUTPUT.
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
META_PACKAGE="${META_PACKAGE:?META_PACKAGE required}"
GITHUB_OUTPUT="${GITHUB_OUTPUT:?GITHUB_OUTPUT required}"

cd "${SOURCE_DIR:-.}"

tag_version="${RELEASE_TAG#v}"
manifest_version=$(cargo metadata --no-deps --format-version 1 \
	| jq -r --arg pkg "${META_PACKAGE}" '.packages[] | select(.name == $pkg) | .version')
if [[ "${tag_version}" != "${manifest_version}" ]]; then
	echo "::error file=Cargo.toml::version does not match release tag"
	echo "error: tag ${RELEASE_TAG} (${tag_version}) does not match Cargo.toml version ${manifest_version}" >&2
	exit 1
fi
echo "ok: tag ${RELEASE_TAG} matches Cargo.toml version ${manifest_version}"
echo "version=${manifest_version}" >>"${GITHUB_OUTPUT}"
