#!/usr/bin/env bash
# Required env: CARGO_REGISTRY_TOKEN, VERSION.
# Optional env: PUBLISH_INTERVAL (seconds between publishes, default 5),
# RETRY_WAIT (seconds after a 429, default 630), MAX_RETRIES (per crate, default 8).
set -euo pipefail

CARGO_REGISTRY_TOKEN="${CARGO_REGISTRY_TOKEN:?CARGO_REGISTRY_TOKEN required}"
VERSION="${VERSION:?VERSION required}"
export CARGO_REGISTRY_TOKEN

PUBLISH_INTERVAL="${PUBLISH_INTERVAL:-5}"
# crates.io refills the new-crate-name allowance on the order of one per ten
# minutes; anything shorter than that just burns a retry.
RETRY_WAIT="${RETRY_WAIT:-630}"
MAX_RETRIES="${MAX_RETRIES:-8}"

# cargo's own dry run resolves the intra-workspace dependency DAG; its upload
# order is the publish order. No registry interaction happens on a dry run.
order_list=$(cargo publish --workspace --locked --dry-run --no-verify 2>&1 | sed -n 's/^[[:space:]]*Uploading \([a-z0-9_-]\{1,\}\) v.*/\1/p')
if [[ -z "${order_list}" ]]; then
	echo "error: could not derive publish order from cargo publish --dry-run" >&2
	exit 1
fi
mapfile -t order <<<"${order_list}"
echo "publish order:"
printf '  %s\n' "${order[@]}"

# Prints "yes" when name@VERSION is already on crates.io (sparse index),
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

for crate in "${order[@]}"; do
	published=$(published_state "${crate}")
	if [[ "${published}" == yes ]]; then
		echo "skip ${crate}@${VERSION}: already on crates.io"
		continue
	fi

	attempt=0
	while true; do
		attempt=$((attempt + 1))
		# Workspace-level verify (clippy + tests + build) gates the release
		# before any publish job runs; per-crate verify builds would add ~11
		# redundant builds inside the rate-limit window.
		status=0
		output=$(cargo publish -p "${crate}" --locked --no-verify 2>&1) || status=$?
		if [[ "${status}" -eq 0 ]]; then
			printf '%s\n' "${output}" | tail -2
			echo "published ${crate}@${VERSION}"
			break
		fi
		if grep -Eiq '429|too many crates|rate limit' <<<"${output}"; then
			if [[ "${attempt}" -ge "${MAX_RETRIES}" ]]; then
				printf '%s\n' "${output}" >&2
				echo "error: ${crate} still rate limited after ${MAX_RETRIES} attempts" >&2
				exit 1
			fi
			echo "rate limited on ${crate} (attempt ${attempt}/${MAX_RETRIES}); waiting ${RETRY_WAIT}s"
			sleep "${RETRY_WAIT}"
			continue
		fi
		# Lost race with a concurrent/partial publish of the same version.
		if grep -Eiq 'already uploaded|already exists' <<<"${output}"; then
			echo "skip ${crate}@${VERSION}: already published (race)"
			break
		fi
		printf '%s\n' "${output}" >&2
		echo "error: publishing ${crate} failed" >&2
		exit 1
	done

	sleep "${PUBLISH_INTERVAL}"
done

echo "all crates published or already present at ${VERSION}"
