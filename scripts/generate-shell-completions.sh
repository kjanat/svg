#!/usr/bin/env bash
# Generate portable completion scripts from already built native executables.
set -euo pipefail

bin_dir="${1:?usage: generate-shell-completions.sh BIN_DIR OUTPUT_DIR}"
output="${2:?usage: generate-shell-completions.sh BIN_DIR OUTPUT_DIR}"
mkdir -p "${output}/"{bash,zsh,fish,powershell}

for bin in svg-format svg-lint svg-language-server; do
	exe="${bin_dir}/${bin}"
	[[ -x "${exe}" ]] || exe="${exe}.exe"
	SVG_COMPLETE=bash "${exe}" >"${output}/bash/${bin}"
	{
		SVG_COMPLETE=zsh "${exe}"
		# A file loaded via fpath must complete the first invocation as well
		# as registering the dispatcher for subsequent invocations.
		printf '\nif [[ $funcstack[1] == _%s ]]; then "${_comps[%s]}" "$@"; fi\n' "${bin}" "${bin}"
	} >"${output}/zsh/_${bin}"
	SVG_COMPLETE=fish "${exe}" >"${output}/fish/${bin}.fish"
done

cp "$(dirname "${BASH_SOURCE[0]}")/svg-completion.ps1" "${output}/powershell/_svg.ps1"
