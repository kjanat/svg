#!/usr/bin/env bash
# Exercise generated registrations in each supported shell.
set -euo pipefail

bin_dir="$(cd "${1:?usage: test-shell-completions.sh BIN_DIR}" && pwd)"
scratch=$(mktemp -d)
trap 'rm -rf "${scratch}"' EXIT
SVG_COMPLETION_TEST_DIR="$(cd "${2:?usage: test-shell-completions.sh BIN_DIR COMPLETIONS_DIR}" && pwd)"
export SVG_COMPLETION_TEST_DIR
export PATH="${bin_dir}:${PATH}"

for bin in svg-format svg-lint svg-language-server; do
	SHELL=/usr/bin/zsh PSModulePath=/example "${bin}" --completions >"${scratch}/generated"
	cmp "${scratch}/generated" "${SVG_COMPLETION_TEST_DIR}/zsh/_${bin}"
	SHELL=/usr/bin/ksh PSModulePath=/example "${bin}" --completions >"${scratch}/detected-pwsh"
	SHELL=/usr/bin/fish "${bin}" --completions 'C:\Program Files\PowerShell\pwsh.exe' >"${scratch}/explicit-pwsh"
	cmp "${scratch}/detected-pwsh" "${scratch}/explicit-pwsh"
	if env -u SHELL -u PSModulePath "${bin}" --completions >"${scratch}/missing" 2>&1; then
		echo "${bin}: --completions unexpectedly succeeded without a shell" >&2
		exit 1
	fi
	grep -F 'could not detect shell' "${scratch}/missing"
done

cd "${scratch}"
touch 'sample file.svg'

bash --noprofile --norc <<'BASH'
set -eo pipefail
for bin in svg-format svg-lint svg-language-server; do
	source "${SVG_COMPLETION_TEST_DIR}/bash/${bin}"
done

check() {
	local expected="$1"
	shift
	COMP_WORDS=("$@")
	COMP_CWORD=$(("${#COMP_WORDS[@]}" - 1))
	COMP_TYPE=9
	local command="$1" word="${COMP_WORDS[COMP_CWORD]}"
	local registration
	registration=$(complete -p "${command}")
	local function="${registration#* -F }"
	function="${function%% *}"
	"${function}" "${command}" "${word}" "${COMP_WORDS[COMP_CWORD-1]}"
	printf '%s\n' "${COMPREPLY[@]}" | grep -Fx -- "${expected}"
}
check --indent-width svg-format --indent-w
check alphabetical svg-format --attribute-sort al
check 'sample file.svg' svg-format sam
check --force-profile svg-lint --force-p
check json svg-lint --format j
check svg2 svg-lint --profile svg2
check 'sample file.svg' svg-lint sam
check --stdio svg-language-server --st
check --completions svg-language-server --comp
check powershell svg-language-server --completions pow
BASH

fish --no-config <<'FISH'
for bin in svg-format svg-lint svg-language-server
    source "$SVG_COMPLETION_TEST_DIR/fish/$bin.fish"; or exit 1
end
function check --argument-names expected line
    complete -C "$line" | string split --fields 1 \t | grep -Fx -- "$expected"; or exit 1
end
check --indent-width 'svg-format --indent-w'
check alphabetical 'svg-format --attribute-sort al'
check 'sample file.svg' 'svg-format sam'
check --force-profile 'svg-lint --force-p'
check json 'svg-lint --format j'
check svg2 'svg-lint --profile svg2'
check 'sample file.svg' 'svg-lint sam'
check --stdio 'svg-language-server --st'
FISH

expect <<'EXPECT'
set timeout 10
log_user 0
proc wait_for {value} {
    expect {
        -exact $value {}
        timeout { error "timed out waiting for $value" }
        eof { error "zsh stopped before $value" }
    }
}
spawn -noecho env TERM=dumb zsh -f
send -- {fpath=("$SVG_COMPLETION_TEST_DIR/zsh" $fpath); autoload -Uz compinit; compinit -D -i; svg_test_buffer() { print -r -- "[svg-buffer:$BUFFER]"; zle kill-whole-line; }; zle -N svg_test_buffer; bindkey '^X^T' svg_test_buffer; print -r -- "[svg-ready]"}
send -- "\r"
wait_for "\r\n\[svg-ready\]"
proc check {line expected} {
    send -- "$line\t\030\024"
    wait_for "\[svg-buffer:$expected"
}
check "svg-format --indent-w" "svg-format --indent-width"
check "svg-format --attribute-sort al" "svg-format --attribute-sort alphabetical"
check "svg-format sam" "svg-format sample\\ file.svg"
check "svg-lint --force-p" "svg-lint --force-profile"
check "svg-lint --format j" "svg-lint --format json"
check "svg-lint --profile Svg2E" "svg-lint --profile Svg2EditorsDraft"
check "svg-lint sam" "svg-lint sample\\ file.svg"
check "svg-language-server --st" "svg-language-server --stdio"
send -- "exit\r"
expect {
    eof {}
    timeout { error "zsh failed to exit" }
}
lassign [wait] pid spawn_id os_error code
if {$os_error != 0 || $code != 0} { error "zsh returned $code" }
EXPECT

pwsh -NoLogo -NoProfile -NonInteractive -Command '
    $ErrorActionPreference = "Stop"
    . (Join-Path $env:SVG_COMPLETION_TEST_DIR "powershell/_svg.ps1")
    function Assert-Completion($line, $expected) {
        $result = TabExpansion2 -inputScript $line -cursorColumn $line.Length
        if ($expected -notin $result.CompletionMatches.CompletionText) {
            throw "Missing completion [$expected] for [$line]: $($result.CompletionMatches.CompletionText)"
        }
    }
    Assert-Completion "svg-format --indent-w" "--indent-width"
    Assert-Completion "svg-format --attribute-sort al" "alphabetical"
    Assert-Completion "svg-format sam" ("{0}sample file.svg{0}" -f [char]39)
    Assert-Completion "svg-lint --force-p" "--force-profile"
    Assert-Completion "svg-lint --format j" "json"
    Assert-Completion "svg-lint --format=j" "--format=json"
    Assert-Completion "svg-lint --profile svg2" "svg2"
    Assert-Completion "svg-lint sam" ("{0}sample file.svg{0}" -f [char]39)
    Assert-Completion "svg-language-server --st" "--stdio"
'
