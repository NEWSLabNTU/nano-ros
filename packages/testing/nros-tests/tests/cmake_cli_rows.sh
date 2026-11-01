#!/usr/bin/env bash
#
# issue 2200 -- a configure reads the `nros` CLI's stdout line by line, never
# as a cmake list.
#
# THE DEFECT
#
# `nros ws leaf-system` prints `NROS_LEAF_CONF=<board>/prj.conf;<leaf>/x.conf`
# when a board package carries its own `prj.conf` AND the image names an
# `os_config` fragment. The Zephyr module hook read the output with
# `string(REPLACE "\n" ";" ...)`, so the `;` inside the value split it: the row
# kept the board's conf, the leaf's fragment became a bare line that matched no
# row, and it was dropped. The configure succeeded and the image linked with no
# zenoh link at all (`# CONFIG_NROS_ZENOH_LINK_SERIAL is not set`), found on an
# out-of-tree xiao-rp2350 board on 2026-11-01. The in-tree pair with the same
# shape is `mps2-an385-zephyr` (its `prj.conf`) + the
# `cpp-port-minimal-publisher/zephyr` leaf (`os_config = ["prj-zenoh.conf"]`),
# whose POSIX/pthread fragment was being dropped the same way.
#
# WHAT THIS PINS
#
#   T1  `nros_cli_rows` keeps a line whole whatever it holds: `;`, an
#       unbalanced `[` (which, as a list, swallows every line after it), `\`,
#       and a CRLF ending; empty lines are skipped.
#   T2  REQUIRE refuses a line outside the grammar instead of skipping it.
#   T3  the hook's own reader (`_nros_image_kconfig_leaf_rows`) given the
#       in-tree board + leaf answer keeps BOTH conf files, and both snippets.
#   T4  ... and refuses the pre-fix shape (the fragment on a line of its own),
#       which is what a split value looks like to it.
#   T5  ... and refuses a conf element that is not an existing file.
#   T6  no cmake module reads CLI output through `string(REPLACE "\n" ";")`
#       any more -- the sweep, so a new reader cannot bring the class back.
#
# Buildless: `cmake -P`, no compiler, no cargo, no CLI, no fixtures.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
ROWS="$ROOT/cmake/NanoRosCliRows.cmake"
HOOK="$ROOT/zephyr/cmake/nros_image_kconfig.cmake"
BOARD_CONF="$ROOT/packages/boards/nros-board-mps2-an385-zephyr/prj.conf"
LEAF="$ROOT/examples/templates/cpp-port-minimal-publisher/zephyr"
LEAF_CONF="$LEAF/prj-zenoh.conf"

for f in "$ROWS" "$HOOK" "$BOARD_CONF" "$LEAF_CONF"; do
    [ -f "$f" ] || {
        echo "FAIL: $f is missing -- the case this gate reads moved; repoint it" >&2
        exit 1
    }
done

# Issue 0726 -- a grep that FAILS must not read as "the refusal named nothing".
# shellcheck source=../../../../scripts/lib/grep-q.sh
. "$ROOT/scripts/lib/grep-q.sh"

mkdir -p "$ROOT/tmp"
WORK="$(mktemp -d "$ROOT/tmp/cmake-cli-rows.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

fail=0
pass() { echo "  ok   $1"; }
bad() {
    echo "  FAIL $1" >&2
    fail=1
}

# Runs a script that must SUCCEED; its FATAL_ERRORs are the assertions.
run_ok() {
    local name="$1" script="$2" log="$WORK/$1.log"
    if cmake -P "$script" >"$log" 2>&1; then
        pass "$name"
    else
        bad "$name"
        sed 's/^/       /' "$log" >&2
    fi
}

# Runs a script that must FAIL with <want> in its output.
run_refused() {
    local name="$1" script="$2" want="$3" log="$WORK/$1.log"
    if cmake -P "$script" >"$log" 2>&1; then
        bad "$name (configured; expected a refusal)"
        sed 's/^/       /' "$log" >&2
    elif nros_grep_q -F -- "$want" "$log"; then
        pass "$name"
    else
        bad "$name (refused, but without '$want')"
        sed 's/^/       /' "$log" >&2
    fi
}

# --- T1 -------------------------------------------------------------------
cat >"$WORK/t1.cmake" <<EOF
include("$ROWS")
string(ASCII 13 CR)
set(text "A=x;y\n\nB=[open\nC=z]\nD=back\\\\slash\${CR}\nE=")
nros_cli_rows(r "\${text}")
list(LENGTH r_INDICES n)
if(NOT n EQUAL 5)
    message(FATAL_ERROR "expected 5 rows, got \${n}: \${r_INDICES}")
endif()
foreach(pair "0|A=x;y" "1|B=[open" "2|C=z]" "3|D=back\\\\slash" "4|E=")
    string(FIND "\${pair}" "|" bar)
    string(SUBSTRING "\${pair}" 0 \${bar} i)
    math(EXPR from "\${bar} + 1")
    string(SUBSTRING "\${pair}" \${from} -1 want)
    if(NOT "\${r_\${i}}" STREQUAL "\${want}")
        message(FATAL_ERROR "row \${i}: [\${r_\${i}}] != [\${want}]")
    endif()
endforeach()
nros_cli_rows(e "")
if(NOT "\${e_INDICES}" STREQUAL "")
    message(FATAL_ERROR "empty text gave rows: \${e_INDICES}")
endif()
EOF
run_ok "T1 lines survive ; [ ] \\ and CRLF" "$WORK/t1.cmake"

# --- T2 -------------------------------------------------------------------
cat >"$WORK/t2.cmake" <<EOF
include("$ROWS")
nros_cli_rows(r "NROS_LEAF_A=1\n/stray/path.conf" REQUIRE "^NROS_LEAF_[A-Z0-9_]+=" CONTEXT "t2")
EOF
run_refused "T2 REQUIRE refuses a stray line" "$WORK/t2.cmake" "issue 2200"

# --- T3..T5: the hook's reader ----------------------------------------------
reader() {
    # $1 = file, $2 = the CLI stdout to read (cmake string syntax), $3 = asserts
    cat >"$1" <<EOF
include("$HOOK")
_nros_image_kconfig_leaf_rows("$2" "$LEAF" "zephyr" "mps2/an385")
$3
EOF
}

reader "$WORK/t3.cmake" \
    "NROS_LEAF_IMAGE=zephyr\nNROS_LEAF_CONF=$BOARD_CONF;$LEAF_CONF\nNROS_LEAF_SNIPPET=nros-a;nros-b\nNROS_LEAF_BOARD_NOT_APPLIED=\nNROS_LEAF_KCONFIG=$WORK/zephyr.conf" \
    "list(LENGTH _image_conf n)
if(NOT n EQUAL 2)
    message(FATAL_ERROR \"NROS_LEAF_CONF lost an element: [\${_image_conf}]\")
endif()
list(GET _image_conf 0 c0)
list(GET _image_conf 1 c1)
if(NOT c0 STREQUAL \"$BOARD_CONF\" OR NOT c1 STREQUAL \"$LEAF_CONF\")
    message(FATAL_ERROR \"order or content changed: [\${_image_conf}]\")
endif()
list(LENGTH _board_snippets s)
if(NOT s EQUAL 2)
    message(FATAL_ERROR \"NROS_LEAF_SNIPPET lost an element: [\${_board_snippets}]\")
endif()
if(NOT _kconfig STREQUAL \"$WORK/zephyr.conf\")
    message(FATAL_ERROR \"NROS_LEAF_KCONFIG: [\${_kconfig}]\")
endif()"
run_ok "T3 board prj.conf + leaf os_config: both kept" "$WORK/t3.cmake"

reader "$WORK/t4.cmake" \
    "NROS_LEAF_CONF=$BOARD_CONF\n$LEAF_CONF\nNROS_LEAF_SNIPPET=" ""
run_refused "T4 a value split across lines is refused" "$WORK/t4.cmake" "issue 2200"

reader "$WORK/t5.cmake" \
    "NROS_LEAF_CONF=$BOARD_CONF;$LEAF/no-such.conf" ""
run_refused "T5 a conf element that is no file is refused" "$WORK/t5.cmake" "not an existing conf file"

# --- T6: the sweep ----------------------------------------------------------
# `cmake/bootstrap.cmake` reads `git submodule status`, not the nros CLI, and
# matches only a leading `-`; it is the one deliberate exception.
# `NanoRosCliRows.cmake` quotes the retired spelling in its header.
# `nros_git_grep` exits 2 itself when git did not search (issue 1816), so an
# empty $hits means "searched, none".
nros_git_grep hits -C "$ROOT" -nE 'REPLACE +"\\n" +";"' -- '*.cmake' '*CMakeLists.txt' \
    ':!third-party' ':!cmake/bootstrap.cmake' ':!cmake/NanoRosCliRows.cmake'
if [ -z "$hits" ]; then
    pass "T6 no CLI reader splits output through a cmake list"
else
    bad "T6 a reader splits output through a cmake list -- use nros_cli_rows (cmake/NanoRosCliRows.cmake):"
    printf '       %s\n' "$hits" >&2
fi

if [ "$fail" -ne 0 ]; then
    echo "check-cmake-cli-rows: FAILED" >&2
    exit 1
fi
echo "check-cmake-cli-rows: OK (rows whole, stray lines refused, board+leaf conf kept, sweep clean)"
