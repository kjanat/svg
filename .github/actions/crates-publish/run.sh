#!/usr/bin/env bash
# Required env: CARGO_REGISTRY_TOKEN, CRATE, VERSION.
# Optional env: RETRY_WAIT (seconds after a 429, default 630),
# MAX_RETRIES (default 8).
set -euo pipefail

CARGO_REGISTRY_TOKEN="${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN required}"
CRATE="${CRATE:?CRATE required}"
VERSION="${VERSION:?VERSION required}"
export CARGO_REGISTRY_TOKEN

# crates.io refills the new-crate-name allowance on the order of one per ten
# minutes; anything shorter than that just burns a retry.
RETRY_WAIT="${RETRY_WAIT:-630}"
# Index propagation of a just-published dependency is seconds, not minutes.
PROPAGATION_WAIT="${PROPAGATION_WAIT:-30}"
MAX_RETRIES="${MAX_RETRIES:-8}"

# Prints "yes" when CRATE@VERSION is already on crates.io (sparse index),
# "no" otherwise; always exits 0. 404 = crate never published. Names here
# are all >= 4 chars, so the index path is <name[0..2]>/<name[2..4]>/<name>.
published_state() {
	local name="$1" body
	if ! body=$(curl -fs "https://index.crates.io/${name:0:2}/${name:2:2}/${name}"); then
		echo no
		return 0
	fi
	if jq -e --arg v "${VERSION}" 'select(.vers == $v)' <<<"${body}" >/dev/null 2>&1; then
		echo yes
	else
		echo no
	fi
}

published=$(published_state "${CRATE}")
if [[ "${published}" == yes ]]; then
	echo "skip ${CRATE}@${VERSION}: already on crates.io"
	exit 0
fi

attempt=0
while true; do
	attempt=$((attempt + 1))
	# Workspace-level verify (clippy + tests + build) gates the release
	# before any publish job runs; a per-crate verify build would add a
	# redundant build inside the rate-limit window.
	status=0
	output=$(cargo publish -p "${CRATE}" --locked --no-verify 2>&1) || status=$?
	if [[ "${status}" -eq 0 ]]; then
		printf '%s\n' "${output}" | tail -2
		echo "published ${CRATE}@${VERSION}"
		exit 0
	fi
	if grep -Eiq '429|too many crates|rate limit' <<<"${output}"; then
		if [[ "${attempt}" -ge "${MAX_RETRIES}" ]]; then
			printf '%s\n' "${output}" >&2
			echo "error: ${CRATE} still rate limited after ${MAX_RETRIES} attempts" >&2
			exit 1
		fi
		echo "rate limited on ${CRATE} (attempt ${attempt}/${MAX_RETRIES}); waiting ${RETRY_WAIT}s"
		sleep "${RETRY_WAIT}"
		continue
	fi
	# A just-published dependency can lag index propagation: the previous
	# job's visibility poll may have timed out while the upload succeeded.
	if grep -Eiq 'no matching package named' <<<"${output}"; then
		if [[ "${attempt}" -ge "${MAX_RETRIES}" ]]; then
			printf '%s\n' "${output}" >&2
			echo "error: ${CRATE} dependencies still unresolvable after ${MAX_RETRIES} attempts" >&2
			exit 1
		fi
		echo "dependency not in index yet for ${CRATE} (attempt ${attempt}/${MAX_RETRIES}); waiting ${PROPAGATION_WAIT}s"
		sleep "${PROPAGATION_WAIT}"
		continue
	fi
	# Lost race with a concurrent/partial publish of the same version.
	if grep -Eiq 'already uploaded|already exists' <<<"${output}"; then
		echo "skip ${CRATE}@${VERSION}: already published (race)"
		exit 0
	fi
	printf '%s\n' "${output}" >&2
	echo "error: publishing ${CRATE} failed" >&2
	exit 1
done
