#!/bin/bash
# tests/census-leaves-west-rows-tests.sh -- issue 2390
#
# `fixtures-manifest.py census-leaves` is the census pre-pass's list of leaves
# to take a census for. `west` rows are opt-in for every OTHER listing
# (phase-350 W1), and before issue 2390 census-leaves inherited that, so no
# Zephyr leaf was ever listed: the pre-pass took no Zephyr census, exported
# `NROS_CENSUS_PREPASS=done` (which turns configure's own take off), and every
# Zephyr C/C++ leaf that is counted by its census refused at configure on a
# fresh tree. Checked against the real manifest:
#
#   1. with no filter, a Zephyr west leaf (`examples/zephyr/cpp/talker`) is listed;
#   2. narrowed to a lane coordinate only a west row has
#      (`zephyr-cortex-m,cpp,zenoh`), that row's leaf is listed;
#   3. each leaf is listed once, however many rows build it;
#   4. the plain `list` command still keeps west rows out (the opt-in stands
#      for the cargo/cmake consumers).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
manifest_py="$ROOT/scripts/build/fixtures-manifest.py"
# shellcheck source=scripts/lib/grep-q.sh
source "$ROOT/scripts/lib/grep-q.sh"
fail=0

# 0 when the exact line is listed, 1 when it is not; a grep that could not
# run exits the script (nros_grep_q), never reads as "not listed".
listed() {
    local rc=0
    nros_grep_q -x -- "$1" <<< "$2" || rc=$?
    return "$rc"
}

all="$(python3 "$manifest_py" census-leaves)"
if listed 'examples/zephyr/cpp/talker' "$all"; then
    echo "ok   1: census-leaves lists the Zephyr west leaf examples/zephyr/cpp/talker"
else
    echo "FAIL 1: census-leaves does not list examples/zephyr/cpp/talker (west rows left out):" >&2
    sed 's/^/       /' <<< "$all" >&2
    fail=1
fi

printf 'zephyr-cortex-m,cpp,zenoh\n' > "$WORK/coords.txt"
narrow="$(python3 "$manifest_py" census-leaves --coords-from "$WORK/coords.txt")"
if listed 'examples/zephyr/cpp/talker' "$narrow"; then
    echo "ok   2: narrowed to zephyr-cortex-m,cpp,zenoh, the west row's leaf is listed"
else
    echo "FAIL 2: narrowed to zephyr-cortex-m,cpp,zenoh, census-leaves listed:" >&2
    sed 's/^/       /' <<< "${narrow:-<nothing>}" >&2
    fail=1
fi

dups="$(sort <<< "$all" | uniq -d)"
if [ -z "$dups" ]; then
    echo "ok   3: every leaf is listed once"
else
    echo "FAIL 3: leaves listed more than once:" >&2
    sed 's/^/       /' <<< "$dups" >&2
    fail=1
fi

rows="$(python3 "$manifest_py" list)"
if nros_grep_q -F -- 'examples/zephyr/cpp/talker' <<< "$rows"; then
    echo "FAIL 4: \`list\` now hands a west row to the cargo/cmake consumers" >&2
    fail=1
else
    echo "ok   4: \`list\` still keeps west rows out"
fi

exit "$fail"
