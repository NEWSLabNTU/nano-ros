#!/usr/bin/env bash
# tests/threadx-app-config-per-image-tests.sh -- issue 2336.
#
# WHAT THIS MEASURES
#
# A ThreadX image's `NROS_APP_CONFIG` carries its network (IP, MAC). Images of
# one bringup that share a cmake configure (phase-530 W3's coordinate union)
# each state their own network (`os_config`), and before issue 2336 the
# configure had ONE config TU, written by the board file from the configure's
# `NROS_IMAGE_NET`: every executable linked the last image's address, so the
# threadx-riscv64 Cyclone pair both came up as 10.0.2.41 / `:57` and never met.
#
#   A  the writer `nros_threadx_app_config_def` reads the image network IN
#      SCOPE: called from two function scopes with two networks, it writes two
#      TUs, each with its own address and MAC (the real in-tree CLI resolves
#      them, through `nros dev board net --image-net=`)
#   B  a rewrite with the same network leaves the file untouched (no recompile
#      on reconfigure)
#   C  REACH: `nano_ros_entry` writes a per-entry TU for a ThreadX image that
#      states a network and marks the target `NROS_OWN_APP_CONFIG`; the
#      ThreadX `nros_platform_link_app` drops the shared TU from such a target;
#      both ThreadX board files write their shared TU through the same writer
#
# A configure-time probe project, so seconds. PRECONDITIONS ARE HARD FAILURES
# (issue 1758): no cmake or no CLI, no verdict, red.
#
# Usage: ./tests/threadx-app-config-per-image-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

FAILURES=0
fail() {
    echo "FAIL: $*" >&2
    FAILURES=$((FAILURES + 1))
}

command -v cmake >/dev/null 2>&1 || { echo "FAIL: cmake not found" >&2; exit 1; }
# shellcheck source=../scripts/build/cargo.sh
source "$PROJECT_ROOT/scripts/build/cargo.sh"
CLI="$(nros_cli_bin --checkout-only 2>/dev/null || true)"
[ -n "$CLI" ] && [ -x "$CLI" ] ||
    { echo "FAIL: no in-tree nros CLI for this checkout (run: just setup-cli)" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

NET_A="eth0,ethernet,qemu,10.0.2.40,24,10.0.2.2,52:54:00:12:34:56,slirp"
NET_B="eth0,ethernet,qemu,10.0.2.41,24,10.0.2.2,52:54:00:12:34:57,slirp"

mkdir -p "$WORK/src"
printf '%s\n' \
    'cmake_minimum_required(VERSION 3.22)' \
    'project(probe NONE)' \
    'include("${NANO_ROS}/cmake/NanoRosBoardNet.cmake")' \
    '# Each call in its own function scope, the way nano_ros_entry shadows' \
    '# NROS_IMAGE_NET for one entry.' \
    'function(_write net out)' \
    '    set(NROS_IMAGE_NET "${net}")' \
    '    nros_threadx_app_config_def(rv-virt-threadx "${out}" ROOT "${NANO_ROS}")' \
    'endfunction()' \
    '_write("${NET_A}" "${CMAKE_BINARY_DIR}/a.c")' \
    '_write("${NET_B}" "${CMAKE_BINARY_DIR}/b.c")' \
    > "$WORK/src/CMakeLists.txt"

if ! out="$(NROS_CLI="$CLI" cmake -S "$WORK/src" -B "$WORK/b" \
        -DNANO_ROS="$PROJECT_ROOT" "-DNET_A=$NET_A" "-DNET_B=$NET_B" 2>&1)"; then
    echo "$out" >&2
    fail "A: the probe configure failed"
else
    A="$WORK/b/a.c"
    B="$WORK/b/b.c"
    nros_grep_q '\.ip = {10, 0, 2, 40}' "$A" || fail "A: image A's TU lacks 10.0.2.40: $(cat "$A")"
    nros_grep_q '0x34, 0x56}' "$A" || fail "A: image A's TU lacks MAC :56"
    nros_grep_q '\.ip = {10, 0, 2, 41}' "$B" || fail "A: image B's TU lacks 10.0.2.41: $(cat "$B")"
    nros_grep_q '0x34, 0x57}' "$B" || fail "A: image B's TU lacks MAC :57"
    if cmp -s "$A" "$B"; then
        fail "A: two networks wrote one TU (issue 2336)"
    fi

    before="$(stat -c '%Y.%N' "$A" 2>/dev/null || stat -f '%m' "$A")"
    sleep 1
    if ! NROS_CLI="$CLI" cmake -S "$WORK/src" -B "$WORK/b" >/dev/null 2>&1; then
        fail "B: the re-configure failed"
    fi
    after="$(stat -c '%Y.%N' "$A" 2>/dev/null || stat -f '%m' "$A")"
    [ "$before" = "$after" ] || fail "B: a re-configure with the same network rewrote the TU"
fi

# C -- REACH.
ENTRY="$PROJECT_ROOT/cmake/NanoRosEntry.cmake"
PLATFORM="$PROJECT_ROOT/cmake/platform/nano-ros-threadx.cmake"
nros_grep_q 'nros_threadx_app_config_def(' "$ENTRY" ||
    fail "C: nano_ros_entry writes no per-entry ThreadX config TU"
nros_grep_q 'NROS_OWN_APP_CONFIG TRUE' "$ENTRY" ||
    fail "C: nano_ros_entry does not mark the target NROS_OWN_APP_CONFIG"
nros_grep_q 'NROS_OWN_APP_CONFIG' "$PLATFORM" ||
    fail "C: the ThreadX link_app does not ask NROS_OWN_APP_CONFIG"
nros_grep_q 'REMOVE_ITEM _startup "\${THREADX_SHARED_APP_CONFIG_DEF}"' "$PLATFORM" ||
    fail "C: the ThreadX link_app does not drop the shared TU"
for board in rv-virt-threadx threadx-linux; do
    f="$PROJECT_ROOT/cmake/board/nano-ros-board-$board.cmake"
    nros_grep_q "nros_threadx_app_config_def($board " "$f" ||
        fail "C: $board writes its shared TU outside the one writer"
    code="$(grep -v '^[[:space:]]*#' "$f" || true)"
    if nros_grep_q 'const nros_app_config_t NROS_APP_CONFIG' <<<"$code"; then
        fail "C: $board spells the NROS_APP_CONFIG body itself (a second producer)"
    fi
done

if [ "$FAILURES" -ne 0 ]; then
    echo "threadx-app-config-per-image: $FAILURES failure(s)" >&2
    exit 1
fi
echo "threadx-app-config-per-image: OK (two image networks, two TUs; reach holds)"
