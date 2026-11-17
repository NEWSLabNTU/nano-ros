#!/usr/bin/env bash
# check-board-module-path: a board's CMake module is located in ONE place
# (RFC-0135 D2, phase-577 W8).
#
# A board package outside nano-ros ships `cmake/nano-ros-board-<key>.cmake`,
# and `nros build` names it as `-DNROS_BOARD_MODULE=<file>`. Five platform
# files used to build the in-tree path by hand, so an outside board's module
# could not be found by any of them. `nros_board_module_path()`
# (cmake/NanoRosBoardModule.cmake) is now the one lookup:
#   1. no CMake file under cmake/ or zephyr/ other than that module spells
#      `nano-ros-board-${NROS_BOARD}.cmake` on a code line;
#   2. the lookup behaves (script mode, no build): an NROS_BOARD_MODULE is used,
#      an in-tree board falls back to cmake/board/, an outside board with no
#      NROS_BOARD_MODULE is refused naming NROS_BOARD_MODULE, and OPTIONAL
#      returns empty instead.
# The behaviour cases are this gate's selftest, run on every invocation.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
mkdir -p "$repo/tmp"
work="$(mktemp -d "$repo/tmp/board-module-path.XXXXXX")"
trap 'rm -rf "$work"' EXIT
fail=0
say() { echo "check-board-module-path: $*"; }
# shellcheck source=../lib/grep-q.sh
. "$repo/scripts/lib/grep-q.sh"

# 1. One spelling.
nros_git_grep raw -C "$repo" -n -E 'nano-ros-board-\$\{' -- \
    'cmake/*.cmake' 'cmake/**/*.cmake' 'zephyr/*.cmake' 'zephyr/**/*.cmake'
hits="$(grep -v -E '^cmake/NanoRosBoardModule\.cmake:|^[^:]+:[0-9]+:[[:space:]]*#' <<<"$raw" || true)"
if [ -n "$hits" ]; then
    say "a board module path is built outside nros_board_module_path():"
    echo "$hits" | sed 's/^/    /'
    fail=1
fi

# 2. The lookup's behaviour.
probe() { # <name> <cmake -D args...>; prints `RESULT=<path>` or the error
    local name=$1; shift
    cat > "$work/$name.cmake" <<EOF
include("$repo/cmake/NanoRosBoardModule.cmake")
nros_board_module_path(_m probe \${PROBE_FLAGS})
message(STATUS "RESULT=\${_m}")
EOF
    cmake "$@" -P "$work/$name.cmake" > "$work/$name.out" 2>&1 || true
}
selftest() {
    local ok=0 intree mod
    intree="$(git -C "$repo" ls-files 'cmake/board/nano-ros-board-*.cmake' | head -1)"
    intree="${intree#cmake/board/nano-ros-board-}"; intree="${intree%.cmake}"
    probe intree -DNROS_BOARD="$intree"
    nros_grep_q "RESULT=.*/cmake/board/nano-ros-board-$intree.cmake" "$work/intree.out" \
        || { say "selftest: in-tree board \`$intree\` did not fall back to cmake/board/"; ok=1; }
    mod="$work/outside/cmake/nano-ros-board-outside.cmake"
    mkdir -p "$(dirname "$mod")"; echo "# outside" > "$mod"
    probe given -DNROS_BOARD=outside -DNROS_BOARD_MODULE="$mod"
    nros_grep_q "RESULT=$mod" "$work/given.out" \
        || { say "selftest: NROS_BOARD_MODULE was not used"; ok=1; }
    probe refused -DNROS_BOARD=no-such-outside-board
    if nros_grep_q "RESULT=" "$work/refused.out" || ! nros_grep_q "NROS_BOARD_MODULE" "$work/refused.out"; then
        say "selftest: an outside board with no NROS_BOARD_MODULE was not refused by name"
        sed 's/^/    /' "$work/refused.out" | head -8
        ok=1
    fi
    probe optional -DNROS_BOARD=no-such-outside-board "-DPROBE_FLAGS=OPTIONAL"
    nros_grep_q "RESULT=$" "$work/optional.out" \
        || { say "selftest: OPTIONAL did not return empty"; ok=1; }
    probe missing -DNROS_BOARD=outside -DNROS_BOARD_MODULE="$work/absent.cmake"
    nros_grep_q "does not exist" "$work/missing.out" \
        || { say "selftest: a NROS_BOARD_MODULE naming no file was not refused"; ok=1; }
    return $ok
}
selftest || fail=1

if [ "$fail" -ne 0 ]; then
    exit 1
fi
say "OK — one lookup; selftest: NROS_BOARD_MODULE used, in-tree fallback, outside board refused by name, OPTIONAL empty, a missing module refused"
