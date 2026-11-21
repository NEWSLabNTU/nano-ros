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
#   C  the metadata probe (issue 2787): a host target that is not an entry,
#      outside a census build, links the same mock through
#      `nros_link_can_census_mock` (the call every probe executable the CLI
#      renders makes) and runs against it as A does
#   D  issue 2822: the driver is called ONLY from a component archive the
#      entry reaches through a `<pkg>_<exec>_component` INTERFACE wrapper,
#      linked before `nros_entry_can_image` as the entry's sidecar does, and
#      from a second archive linked after it. Linking the mock to the entry
#      alone put it on the link line before both archives, so ld dropped it:
#      `libcomp_lib.a(comp.c.o): undefined reference to nros_platform_can_open`
#   E  the same component layout through `nros_link_can_census_mock` (the
#      metadata probe's road), outside a census build
#   F  the same layout with the `socketcan` backend (`nros_can_socketcan`, an
#      image that names a device): the component archives link against it.
#      Link only; the device's own road is `just check can-image-device`
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

echo "C  the metadata probe links the CAN mock outside a census build"
mkdir -p "$T/probe"
cp "$T/src/main.c" "$T/probe/main.c"
cat >"$T/probe/CMakeLists.txt" <<CMAKE
cmake_minimum_required(VERSION 3.22)
project(can_metadata_probe C)
include("$ROOT/cmake/NanoRosCanImage.cmake")
add_executable(probe main.c)
target_include_directories(probe PRIVATE "$ROOT/packages/platform/nros-platform-api/include")
nros_link_can_census_mock(probe)
CMAKE
if env -u NROS_LEAF_CENSUS_HOST cmake -S "$T/probe" -B "$T/c" >"$T/c.log" 2>&1 \
    && cmake --build "$T/c" >>"$T/c.log" 2>&1; then
    out="$("$T/c/probe")"
    if nros_grep_q "^open=0 send=0 fd=-3 recv=-4 open_fd=0 fd_send=0 store=empty\$" <<<"$out"; then
        ok "the probe links and runs against the mock ($out)"
    else
        fail "unexpected mock behaviour in the probe: $out"
    fi
else
    fail "the probe does not link the mock: $(tail -5 "$T/c.log")"
fi
probe_rs="packages/cli/nros-cli-core/src/orchestration/metadata_probe_cmake.rs"
rc=0
nros_grep_q '"nros_link_can_census_mock({t})' "$probe_rs" || rc=$?
if [ "$rc" -eq 0 ]; then
    ok "the CLI's probe project links it into every probe executable"
else
    fail "$probe_rs renders no nros_link_can_census_mock call"
fi

# D/E -- issue 2822. The driver calls live in a component archive only; the
# entry's own source calls the component.
mkdir -p "$T/comp"
sed 's/^int main(void) {$/int comp_run(void) {/' "$T/src/main.c" >"$T/comp/comp.c"
cat >"$T/comp/late.c" <<'C'
#include <nros/platform_can.h>
void late_close(void) {
    static nros_platform_can_t can;
    nros_platform_can_close(&can);
}
C
cat >"$T/comp/main.c" <<'C'
int comp_run(void);
void late_close(void);
int main(void) {
    late_close();
    return comp_run();
}
C
comp_cmake() {
    # $1: the CAN hook to call between the two links.
    cat <<CMAKE
cmake_minimum_required(VERSION 3.22)
project(can_component_probe C)
include("$ROOT/cmake/NanoRosCanImage.cmake")
add_library(comp_lib STATIC comp.c)
target_include_directories(comp_lib PRIVATE "$ROOT/packages/platform/nros-platform-api/include")
add_library(comp_pkg_comp_component INTERFACE)
target_link_libraries(comp_pkg_comp_component INTERFACE comp_lib)
add_library(late_lib STATIC late.c)
target_include_directories(late_lib PRIVATE "$ROOT/packages/platform/nros-platform-api/include")
add_executable(probe main.c)
target_link_libraries(probe PRIVATE comp_pkg_comp_component)
$1
target_link_libraries(probe PRIVATE late_lib)
CMAKE
}
comp_case() {
    # $1: row, $2: build dir, $3: hook, $4: census env (1 or empty).
    local row="$1" dir="$2" hook="$3" census="$4" src="$T/comp-$1"
    mkdir -p "$src"
    cp "$T/comp/comp.c" "$T/comp/late.c" "$T/comp/main.c" "$src/"
    comp_cmake "$hook" >"$src/CMakeLists.txt"
    local ok_cfg=0
    if [ -n "$census" ]; then
        NROS_LEAF_CENSUS_HOST=1 cmake -S "$src" -B "$dir" >"$dir.log" 2>&1 || ok_cfg=1
    else
        env -u NROS_LEAF_CENSUS_HOST cmake -S "$src" -B "$dir" >"$dir.log" 2>&1 || ok_cfg=1
    fi
    if [ "$ok_cfg" -eq 0 ] && cmake --build "$dir" >>"$dir.log" 2>&1; then
        out="$("$dir/probe")"
        if nros_grep_q "^open=0 send=0 fd=-3 recv=-4 open_fd=0 fd_send=0 store=empty\$" <<<"$out"; then
            ok "$row: the component archives link and run against the mock ($out)"
        else
            fail "$row: unexpected mock behaviour: $out"
        fi
    else
        fail "$row: the component archives do not link the mock: $(grep -m3 -E 'undefined reference|Error' "$dir.log")"
    fi
}

echo "D  census host build: the driver called only from component archives"
comp_case D "$T/d" "nros_entry_can_image(probe linux)" 1

echo "E  metadata probe road: the driver called only from component archives"
comp_case E "$T/e" "nros_link_can_census_mock(probe)" ""

echo "F  socketcan backend: the driver called only from component archives"
mkdir -p "$T/comp-F"
cp "$T/comp/comp.c" "$T/comp/late.c" "$T/comp/main.c" "$T/comp-F/"
# The device hook needs the CLI's codec; the placement under test is the
# backend link, so drive it directly as `nros_entry_can_image` does.
comp_cmake "_nros_can_socketcan_library()
_nros_can_link_backend(probe nros_can_socketcan)" >"$T/comp-F/CMakeLists.txt"
if env -u NROS_LEAF_CENSUS_HOST cmake -S "$T/comp-F" -B "$T/f" >"$T/f.log" 2>&1 \
    && cmake --build "$T/f" >>"$T/f.log" 2>&1; then
    ok "F: the component archives link against the socketcan backend"
else
    fail "F: the component archives do not link the socketcan backend: $(grep -m3 -E 'undefined reference|Error' "$T/f.log")"
fi

if [ "$FAILURES" -ne 0 ]; then
    echo "can-census-mock: $FAILURES failure(s)"
    exit 1
fi
echo "can-census-mock: all assertions held"
