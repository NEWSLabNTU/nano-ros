#!/usr/bin/env bash
# tests/can-census-mock-tests.sh -- RFC-0132 D6, the census host build's CAN
# backend (`cmake/NanoRosCanImage.cmake`, `nros-can-mock/c/census_mock.c`).
#
# WHAT THIS MEASURES
#
# A C/C++ entry on a cross board is sized by its host census: its own sources,
# built for the host and run in counting mode. An entry that calls the CAN
# frame driver needs a backend there, and before D6's mock the census build
# had none, so `nros build` stopped at `undefined reference to
# nros_platform_can_open`. A tiny project drives the same function
# `nano_ros_entry` calls (`nros_entry_can_image`):
#
#   A  in a census host build (`NROS_LEAF_CENSUS_HOST=1`) an entry that calls
#      the driver links, and runs against the mock: open(NULL) 0, a classic
#      send 0, an FD send without want_fd ENOFD (-3), recv EAGAIN (-4), an FD
#      send after open(want_fd) 0, and the device store links too
#   B  NEGATIVE CONTROL: the same project outside a census build, with no
#      image device, links no CAN backend (the link fails), so A's backend is
#      the census mock and nothing else
#
# A configure + build of one C file, seconds. PRECONDITIONS ARE HARD FAILURES
# (issue 1758): no cmake or no cc, no verdict.
#
# Usage: ./tests/can-census-mock-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=../scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

for tool in cmake cc; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "can-census-mock: FAIL: no $tool" >&2
        exit 1
    fi
done

FAILURES=0
fail() { echo "  FAIL $*"; FAILURES=$((FAILURES + 1)); }
ok() { echo "  ok   $*"; }

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/src"
cat >"$T/src/CMakeLists.txt" <<CMAKE
cmake_minimum_required(VERSION 3.22)
project(can_census_probe C)
include("$ROOT/cmake/NanoRosCanImage.cmake")
add_executable(probe main.c)
target_include_directories(probe PRIVATE "$ROOT/packages/platform/nros-platform-api/include")
nros_entry_can_image(probe linux)
CMAKE
cat >"$T/src/main.c" <<'C'
#include <stdio.h>
#include <nros/platform_can.h>
extern const char* nros_board_can_image_device(void);
int main(void) {
    static nros_platform_can_t can;
    nros_platform_can_frame_t classic = {.id = 0x123u, .flags = 0u, .len = 8u};
    nros_platform_can_frame_t fd = {.id = 0x124u, .flags = NROS_PLATFORM_CAN_FLAG_FD, .len = 64u};
    nros_platform_can_frame_t rx;
    int o = nros_platform_can_open(&can, NULL, 0u);
    int s = nros_platform_can_send(&can, &classic, 0u);
    int f = nros_platform_can_send(&can, &fd, 0u);
    int r = nros_platform_can_recv(&can, &rx, 0u);
    nros_platform_can_close(&can);
    int o2 = nros_platform_can_open(&can, NULL, 1u);
    int f2 = nros_platform_can_send(&can, &fd, 0u);
    printf("open=%d send=%d fd=%d recv=%d open_fd=%d fd_send=%d store=%s\n", o, s, f, r, o2, f2,
           nros_board_can_image_device() == NULL ? "empty" : "set");
    return 0;
}
C

echo "A  a census host build links the CAN mock"
if NROS_LEAF_CENSUS_HOST=1 cmake -S "$T/src" -B "$T/a" >"$T/a.log" 2>&1 \
    && cmake --build "$T/a" >>"$T/a.log" 2>&1; then
    out="$("$T/a/probe")"
    if nros_grep_q "^open=0 send=0 fd=-3 recv=-4 open_fd=0 fd_send=0 store=empty\$" <<<"$out"; then
        ok "the entry links and runs against the mock ($out)"
    else
        fail "unexpected mock behaviour: $out"
    fi
else
    fail "the census host build does not link: $(tail -5 "$T/a.log")"
fi

echo "B  negative control: no census build, no device, no backend"
if env -u NROS_LEAF_CENSUS_HOST cmake -S "$T/src" -B "$T/b" >"$T/b.log" 2>&1 \
    && ! cmake --build "$T/b" >>"$T/b.log" 2>&1; then
    rc=0
    nros_grep_q "undefined reference to .nros_platform_can_open" "$T/b.log" || rc=$?
    if [ "$rc" -eq 0 ]; then
        ok "the link fails on nros_platform_can_open: the census mock is A's backend"
    else
        fail "the control failed for another reason: $(tail -5 "$T/b.log")"
    fi
else
    fail "the control linked or did not configure: $(tail -5 "$T/b.log")"
fi

if [ "$FAILURES" -ne 0 ]; then
    echo "can-census-mock: $FAILURES failure(s)"
    exit 1
fi
echo "can-census-mock: all assertions held"
