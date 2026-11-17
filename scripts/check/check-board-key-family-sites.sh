#!/usr/bin/env bash
# check-board-key-family-sites: a board KEY reaches its family through ONE
# CLI function (RFC-0135 D2, phase-577 W8).
#
# `nros_entry_lower::BOARD_KEYS` is compiled into nano-ros, so it can hold
# only nano-ros's own boards. A board package outside nano-ros is reached
# through the board catalog, and every nros-cli-core site that asked the
# table alone refused it (`codegen entry-pack`: "unknown board key").
# `orchestration::board_key::board_family` asks the table, then the catalog.
# This refuses a new direct call in nros-cli-core/src, outside the files that
# legitimately hold the table's own keys:
#   - orchestration/board_key.rs     -- the one function;
#   - sizing_descriptor.rs           -- reads a SCHED-TASK key ("freertos"),
#                                       which is a family word, not a board key;
#   - codegen/entry/emit/tests_cpp.rs, codegen/entry/pack.rs's test module
#                                    -- tests over in-tree keys.
# Selftest: a planted direct call in a scratch copy is refused.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
src="packages/cli/nros-cli-core/src"
say() { echo "check-board-key-family-sites: $*"; }

PATTERN='nros_entry_lower::(board_family|entry_shape_for_key)\('
# shellcheck source=../lib/grep-q.sh
. "$repo/scripts/lib/grep-q.sh"

scan() { # <root> <file:line:text hits>: prints the offending ones
    printf '%s\n' "$2" | grep -v '^$' \
        | grep -v -E "^$src/orchestration/board_key\.rs:" \
        | grep -v -E "^$src/sizing_descriptor\.rs:" \
        | grep -v -E "^$src/codegen/entry/emit/tests_cpp\.rs:" \
        | grep -v -E '^[^:]+:[0-9]+:[[:space:]]*//' \
        | while IFS= read -r hit; do
            file="${hit%%:*}"; rest="${hit#*:}"; line="${rest%%:*}"
            # A line inside a `#[cfg(test)]` module of its file is a test.
            first_test="$(grep -n -m1 '^#\[cfg(test)\]' "$1/$file" | cut -d: -f1 || true)"
            if [ -n "$first_test" ] && [ "$line" -gt "$first_test" ]; then
                continue
            fi
            echo "$hit"
        done || true  # every hit exempt is the GOOD outcome, not an error
}

fail=0
nros_git_grep tracked_hits -C "$repo" -n -E "$PATTERN" -- "$src"
hits="$(scan "$repo" "$tracked_hits")"
if [ -n "$hits" ]; then
    say "a board key reaches nros_entry_lower's compiled-in table directly; ask crate::orchestration::board_key::board_family (or entry_shape), which also knows board packages outside nano-ros:"
    echo "$hits" | sed 's/^/    /'
    fail=1
fi

selftest() {
    local tmp; tmp="$(mktemp -d "$repo/tmp/board-key-sites.XXXXXX")"
    mkdir -p "$tmp/$src/cmd"
    printf 'fn f() { let _ = nros_entry_lower::board_family("x"); }\n' > "$tmp/$src/cmd/planted.rs"
    local raw got
    raw="$(cd "$tmp" && grep -n -E "$PATTERN" "$src/cmd/planted.rs" | sed "s|^|$src/cmd/planted.rs:|")" || true
    got="$(scan "$tmp" "$raw")"; rm -rf "$tmp"
    [ -n "$got" ] || { say "selftest: a planted direct call was not refused"; return 1; }
}
mkdir -p "$repo/tmp"
selftest || fail=1
[ "$fail" -eq 0 ] || exit 1
say "OK -- every nros-cli-core board-key family lookup goes through orchestration::board_key; selftest: a planted direct call is refused"
