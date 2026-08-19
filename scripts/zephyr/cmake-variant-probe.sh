#!/usr/bin/env bash
# issue 0698 — does the Zephyr SDK lookup survive THIS cmake?
#
# Runs a real configure for one SDK board and one native_sim board, resolving
# the toolchain variant exactly the way the fixture runners now do, and reports
# per board whether Zephyr's `FindZephyr-sdk.cmake` accepted its own `if()`.
#
# Deliberately stops caring after the toolchain stage: a configure may still
# fail later (stale CLI, missing generated interfaces) and that says nothing
# about this question. The verdict keys on the SDK-lookup outcome only.
#
# Run identically on both sides of the split:
#   host (CMake 4.x):  bash scripts/zephyr/cmake-variant-probe.sh
#   box  (CMake 3.22): distrobox enter ros2 -- bash -c \
#       'cd <checkout>-box && . scripts/dev/ros2-box-env.sh && bash scripts/zephyr/cmake-variant-probe.sh'
set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
source "$root/scripts/build/zephyr-toolchain.sh"

export ZEPHYR_BASE="$root/zephyr-workspace/zephyr"
[ -d "$ZEPHYR_BASE" ] || { echo "no zephyr workspace at $ZEPHYR_BASE — run: just zephyr setup" >&2; exit 2; }
# The in-tree venv is built for the HOST interpreter, so in a container it may
# be present, executable, and still wrong. `NROS_PYTHON` names the one to use —
# the SAME knob `check-python-deps.py`, `just doctor` and `scripts/zephyr/
# setup.sh` read, because a probe that answers for a different interpreter than
# the lane uses answers the wrong question convincingly. The probe then verifies
# it can actually import Zephyr's build deps rather than discovering that four
# frames into cmake as "Error finding board".
py="${NROS_PYTHON:-$root/scripts/zephyr/.venv/bin/python3}"
if ! "$py" -c 'import pykwalify, yaml, elftools' >/dev/null 2>&1; then
    alt="$(command -v python3)"
    if "$alt" -c 'import pykwalify, yaml, elftools' >/dev/null 2>&1; then
        py="$alt"
    else
        echo "no python3 with Zephyr's build deps (pykwalify, PyYAML, pyelftools)." >&2
        echo "  tried: $py and $alt" >&2
        echo "  set NROS_PYTHON to one that has them." >&2
        exit 2
    fi
fi

echo "cmake:  $(cmake --version | head -1)"
echo "zephyr: $(sed -n 's/^VERSION_MAJOR *= *//p' "$ZEPHYR_BASE/VERSION" 2>/dev/null).$(sed -n 's/^VERSION_MINOR *= *//p' "$ZEPHYR_BASE/VERSION" 2>/dev/null)"
echo

rc_all=0
probe() {
    local board="$1" label="$2" variant out bdir
    # The unset case is the bug; ask the shared resolver, same as the runners.
    variant="$(nros_zephyr_toolchain_variant "$board")"
    bdir="$root/tmp/zprobe-$label"
    rm -rf "$bdir"
    out="$(ZEPHYR_TOOLCHAIN_VARIANT="$variant" cmake \
        -DWEST_PYTHON="$py" -B"$bdir" -GNinja -DBOARD="$board" \
        -S"$root/examples/zephyr/c/talker" 2>&1)"

    if grep -q "FindZephyr-sdk.cmake.*(if)" <<<"$out"; then
        echo "FAIL  $label  board=$board variant=$variant — FindZephyr-sdk rejected its own if()"
        rc_all=1
    elif grep -q "Found toolchain:" <<<"$out"; then
        echo "ok    $label  board=$board variant=$variant — $(grep -m1 'Found toolchain:' <<<"$out" | sed 's/^-- //')"
    else
        echo "?     $label  board=$board variant=$variant — no verdict; last lines:"
        tail -4 <<<"$out" | sed 's/^/          /'
        rc_all=1
    fi
    rm -rf "$bdir"
}

probe mps2_an385 sdk-board
probe native_sim native-sim

echo
[ "$rc_all" = 0 ] && echo "PROBE OK — the SDK lookup parses under this cmake" \
                  || echo "PROBE FAILED"
exit "$rc_all"
