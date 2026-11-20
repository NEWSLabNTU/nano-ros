#!/usr/bin/env bash
# Gate: `<Tab>` completes `nros` names through the real zsh and fish glue
# (phase-569 W4b, RFC-0133 D7).
#
# The glue is what `COMPLETE=<shell> nros` prints, exactly what the line
# `nros completions <shell>` prints sources. Each shell sources it and asks
# for `nros board show mps2-an385-free<Tab>` and `nros rmw show zen<Tab>`, so
# a pass covers the glue, the binary's completion engine and the completer
# behind each argument. bash is covered by `nros-cli/tests/completion_glue.rs`,
# which needs no capability.
#
# zsh runs its completion functions only inside its line editor, so the glue
# is driven directly: `compdef` (which only registers) is a no-op and
# `_describe` (which only displays) prints its candidates; the function that
# asks `nros` is the glue's own.
set -euo pipefail

REPO="$(cd -P "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
# This checkout's CLI, or nothing (issue 1850): the glue under test is what
# this tree builds, never a store or PATH `nros`.
# shellcheck source=scripts/build/cargo.sh
source "$REPO/scripts/build/cargo.sh"
if ! NROS="$(NROS_CLI= nros_cli_bin --checkout-only)"; then
    echo "check-completion-glue: this checkout's nros is not built; run \`just setup-cli\`" >&2
    exit 1
fi
bin="$(mktemp -d)"
trap 'rm -rf "$bin"' EXIT
ln -s "$NROS" "$bin/nros"
export PATH="$bin:$PATH" NROS_WORKSPACE_ROOT="$REPO"
cd "$REPO"

failures=0

# expect <shell> <candidate> <output>: the candidate is one line of the output
# (fish appends a tab and a description, which is dropped).
expect() {
    local shell="$1" want="$2" out="$3" line
    while IFS= read -r line; do
        [ "${line%%$'\t'*}" = "$want" ] && return 0
    done <<<"$out"
    echo "check-completion-glue: $shell did not offer \`$want\`; it offered:" >&2
    printf '  %s\n' "$out" >&2
    failures=$((failures + 1))
}

# The negative control: a name no completer knows is not found in output that
# lacks it, or `expect` would pass on any output. Runs on every call.
selftest() {
    local before=$failures
    expect selftest not-a-board-anywhere "mps2-an385-freertos" 2>/dev/null
    if [ "$failures" -eq "$before" ]; then
        echo "check-completion-glue: the negative control passed; expect() checks nothing" >&2
        exit 1
    fi
    failures=$before
}
selftest

fish_board="$(fish -c 'COMPLETE=fish nros | source; complete -C "nros board show mps2-an385-free"')"
fish_rmw="$(fish -c 'COMPLETE=fish nros | source; complete -C "nros rmw show zen"')"
expect fish mps2-an385-freertos "$fish_board"
expect fish zenoh "$fish_rmw"

zsh_drive='compdef() { :; }
source <(COMPLETE=zsh nros)
_describe() { print -l -- ${(P)2}; }
words=(nros "$1" show "$2"); CURRENT=4
_clap_dynamic_completer_nros'
zsh_board="$(zsh -fc "$zsh_drive" zsh board mps2-an385-free)"
zsh_rmw="$(zsh -fc "$zsh_drive" zsh rmw zen)"
expect zsh mps2-an385-freertos "$zsh_board"
expect zsh zenoh "$zsh_rmw"

if [ "$failures" -ne 0 ]; then
    exit 1
fi
echo "check-completion-glue: zsh and fish complete board keys and RMW names through their glue"
