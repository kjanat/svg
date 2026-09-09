#!/usr/bin/env bash
# Exercise the real retry loop with fake registry/Cargo clients. Never uploads.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
temp_parent=$(cd -- "${TMPDIR:-/tmp}" && pwd -P)
test_dir=$(mktemp -d "${temp_parent}/svg-release-tests.XXXXXX")
case "${test_dir}" in
	"${temp_parent}"/svg-release-tests.*) ;;
	*) exit 1 ;;
esac
trap 'rm -rf -- "${test_dir}"' EXIT
mkdir -p "${test_dir}/bin" "${test_dir}/proof" "${test_dir}/source"

cat >"${test_dir}/bin/python3" <<'EOF'
#!/usr/bin/env bash
set -eu
mode=$2
echo "${mode}" >> "${CASE_DIR}/checks"
if [[ "${SCENARIO}" == invalid && "${mode}" == inputs ]]; then
  echo "verification mismatch" >&2; exit 1
fi
if [[ "${mode}" == package ]]; then
  count=$(grep -c '^package$' "${CASE_DIR}/checks")
  if [[ "${SCENARIO}" == changed ]]; then
    echo "upload package differs from the verified archive" >&2; exit 1
  fi
  if [[ "${SCENARIO}" == propagation && "${count}" == 1 ]]; then
    echo "no matching package named sibling" >&2; exit 1
  fi
fi
EOF
cat >"${test_dir}/bin/cargo" <<'EOF'
#!/usr/bin/env bash
set -eu
echo "$*" >> "${CASE_DIR}/uploads"
count=$(wc -l < "${CASE_DIR}/uploads")
case "${SCENARIO}" in
  rate) if [[ "${count}" == 1 ]]; then echo '429 too many crates' >&2; exit 101; fi ;;
  exhausted) echo '429 too many crates' >&2; exit 101 ;;
  fatal) echo 'invalid package' >&2; exit 101 ;;
  race) echo 'already uploaded' >&2; exit 101 ;;
  index) if [[ "${count}" == 1 ]]; then echo 'no matching package named sibling' >&2; exit 101; fi ;;
esac
echo 'Published fixture'
EOF
cat >"${test_dir}/bin/curl" <<'EOF'
#!/usr/bin/env bash
if [[ "${SCENARIO}" == existing ]]; then echo '{"vers":"0.0.0-test"}'; else exit 22; fi
EOF
cat >"${test_dir}/bin/sleep" <<'EOF'
#!/usr/bin/env bash
echo "$*" >> "${CASE_DIR}/waits"
EOF
chmod +x "${test_dir}"/bin/*

export PATH="${test_dir}/bin:${PATH}"
export SOURCE_DIR="${test_dir}/source" PROOF_DIR="${test_dir}/proof"
export CARGO_REGISTRY_TOKEN=unused CRATE=svg-release-fixture VERSION=0.0.0-test
export RELEASE_TAG=v0.0.0-test HELPER_SHA=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
export RETRY_WAIT=1 PROPAGATION_WAIT=1 MAX_RETRIES=2

run_case() {
	local scenario="$1" expected_status="$2" expected_uploads="$3" expected_packages="$4" status=0 uploads packages
	export SCENARIO="${scenario}" CASE_DIR="${test_dir}/${scenario}"
	mkdir "${CASE_DIR}"
	: >"${CASE_DIR}/uploads"
	: >"${CASE_DIR}/checks"
	bash "${root}/.github/actions/crates-publish/run.sh" >"${CASE_DIR}/output" 2>&1 || status=$?
	uploads=$(wc -l <"${CASE_DIR}/uploads")
	packages=$(grep -c '^package$' "${CASE_DIR}/checks" || true)
	if [[ "${status}" -ne "${expected_status}" || "${uploads}" -ne "${expected_uploads}" || "${packages}" -ne "${expected_packages}" ]]; then
		cat "${CASE_DIR}/output"
		echo "FAIL ${scenario}: status=${status}, uploads=${uploads}, package checks=${packages}" >&2
		exit 1
	fi
	if [[ "${uploads}" -gt 0 ]]; then
		grep -q -- '--locked --all-features --registry crates-io --no-verify' "${CASE_DIR}/uploads"
	fi
	echo "PASS ${scenario}: uploads=${uploads}, package checks=${packages}"
}

run_case success 0 1 1
run_case invalid 1 0 0
run_case changed 1 0 1
run_case existing 0 0 0
run_case rate 0 2 1
run_case propagation 0 1 2
run_case index 0 2 1
run_case exhausted 1 2 1
run_case fatal 1 1 1
run_case race 0 1 1
