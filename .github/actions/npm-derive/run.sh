#!/usr/bin/env bash
# Required env: RELEASE_TAG, EVENT_NAME, GITHUB_OUTPUT.
# Optional env: INPUT_DIST_TAG, INPUT_DRY_RUN.
set -euo pipefail

RELEASE_TAG="${RELEASE_TAG:?RELEASE_TAG required}"
EVENT_NAME="${EVENT_NAME:?EVENT_NAME required}"
GITHUB_OUTPUT="${GITHUB_OUTPUT:?GITHUB_OUTPUT required}"

input_dist_tag="${INPUT_DIST_TAG-}"
input_dry_run="${INPUT_DRY_RUN-false}"

if [[ -n "${input_dist_tag}" ]]; then
	# Manual override always wins. Validate shape so a malformed input
	# can't slip flag-like or whitespace values into `npm publish --tag`.
	if [[ ! "${input_dist_tag}" =~ ^[A-Za-z][A-Za-z0-9._-]*$ ]]; then
		echo "error: INPUT_DIST_TAG '${input_dist_tag}' is not a valid npm dist-tag (^[A-Za-z][A-Za-z0-9._-]*$)" >&2
		exit 1
	fi
	dist_tag="${input_dist_tag}"
else
	# Infer from the tag: prerelease (e.g. v1.0.0-rc.1) → next, else latest.
	case "${RELEASE_TAG}" in
		*-*) dist_tag=next ;;
		*) dist_tag=latest ;;
	esac
fi

if [[ "${EVENT_NAME}" == "workflow_dispatch" ]]; then
	# Normalize to strict true/false so downstream string compares
	# aren't fooled by "True"/"1"/"yes" silently meaning false.
	case "${input_dry_run,,}" in
		true) dry_run=true ;;
		false | "") dry_run=false ;;
		*)
			echo "error: INPUT_DRY_RUN '${input_dry_run}' must be 'true' or 'false'" >&2
			exit 1
			;;
	esac
else
	dry_run=false
fi

{
	echo "dist-tag=${dist_tag}"
	echo "dry-run=${dry_run}"
} | tee -a "${GITHUB_OUTPUT}"
