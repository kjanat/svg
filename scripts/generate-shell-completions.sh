#!/usr/bin/env bash
# Generate portable completion scripts from already built native executables.
set -euo pipefail

bin_dir="${1:?usage: generate-shell-completions.sh BIN_DIR OUTPUT_DIR}"
output="${2:?usage: generate-shell-completions.sh BIN_DIR OUTPUT_DIR}"
mkdir -p "${output}/"{bash,zsh,fish,powershell}

{
	for bin in svg-format svg-lint svg-language-server; do
		exe="${bin_dir}/${bin}"
		[[ -x "${exe}" ]] || exe="${exe}.exe"
		"${exe}" --completions bash >"${output}/bash/${bin}"
		"${exe}" --completions zsh >"${output}/zsh/_${bin}"
		"${exe}" --completions fish >"${output}/fish/${bin}.fish"
		"${exe}" --completions powershell
	done
} >"${output}/powershell/_svg.ps1"
