#!/usr/bin/env bash
# tests/can-board-defaults-tests.sh -- RFC-0132 D3, the weak fail-loud
# defaults of the CAN frame driver
# (`nros-board-common/c/nros_board_can_unsupported.c`) on every board with no
# CAN backend of its own (issue 2788).
#
# WHAT THIS MEASURES
#
# On a board whose stack is not "host" and that is not Zephyr (a "board"
# stack, or NuttX's "os" stack) nano-ros has no CAN frame backend: the driver
# is a board package's or a consumer's strong symbols. An image whose board
# supplies none must LINK, and fail at `open` with NROS_PLATFORM_CAN_ENOTSUP
# and one line naming the board -- not fail at link, and not silently. The
# cmake road compiles the defaults into every entry on such a board
# (`nros_entry_can_defaults`, `cmake/NanoRosCanImage.cmake`); the cargo road
# compiles them in the board glue runners (`nros_board_common::can_unsupported`).
# A tiny host project drives the cmake function `nano_ros_entry` calls, with
# the board's stack read from its own descriptor (standing in for the CLI's
# `nros_board_net_c`):
#
#   A  mps3-an536-freertos (the board issue 2788 names): an entry that calls
#      the driver links; open(NULL) returns ENOTSUP (-6) and the line names
#      `mps3-an536-freertos`; send and recv return ENOTSUP, fd_active is 0
#   B  the same entry with a strong `nros_platform_can_open` in another object
#      of the image: the strong one is called, the default's line is not
#      printed
#   C  NEGATIVE CONTROL: a "host"-stack board (freertos-posix, whose backend is
#      SocketCAN) gets no defaults, so the link fails: the weak symbols never
#      shadow a real backend
#   D  every in-tree board that is neither "host" nor Zephyr is covered on the
#      cargo road too: its build script runs a glue runner that compiles the
#      defaults, or it is listed with why not
#
# Seconds. PRECONDITIONS ARE HARD FAILURES (issue 1758): no cmake or no cc,
# no verdict.
#
# Usage: ./tests/can-board-defaults-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=../scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

for tool in cmake "${CC:-cc}"; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "can-board-defaults: FAIL: no $tool" >&2
        exit 1
    fi
done

FAILURES=0
fail() { echo "  FAIL $*"; FAILURES=$((FAILURES + 1)); }
ok() { echo "  ok   $*"; }

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/src"

descriptor_stack() {
    sed -n 's/^stack *= *"\([a-z]*\)".*/\1/p' "$1/nros-board.toml" | sort -u
}

cat >"$T/src/CMakeLists.txt" <<CMAKE
cmake_minimum_required(VERSION 3.22)
project(can_board_defaults C)
include("$ROOT/cmake/NanoRosCanImage.cmake")
# The CLI's answer for the board's stack, read from its descriptor by the
# script, so the test needs no CLI; the rule under test is
# nros_entry_can_defaults'.
function(nros_board_net_c board out_var)
    cmake_parse_arguments(_T "" "STACK_VAR" "" \${ARGN})
    set(\${_T_STACK_VAR} "\${TEST_BOARD_STACK}" PARENT_SCOPE)
    set(\${out_var} "" PARENT_SCOPE)
endfunction()
add_executable(entry main.c \${TEST_EXTRA_SRC})
target_include_directories(entry PRIVATE "$ROOT/packages/platform/nros-platform-api/include")
nros_entry_can_defaults(entry "\${TEST_BOARD}")
CMAKE
cat >"$T/src/main.c" <<'C'
#include <stdio.h>
#include <nros/platform_can.h>
int main(void) {
    static nros_platform_can_t can;
    nros_platform_can_frame_t f = {0};
    int8_t o = nros_platform_can_open(&can, NULL, 0u);
    int8_t s = nros_platform_can_send(&can, &f, 0u);
    int8_t r = nros_platform_can_recv(&can, &f, 0u);
    printf("open=%d send=%d recv=%d fd=%u\n", o, s, r, nros_platform_can_fd_active(&can));
    nros_platform_can_close(&can);
    return 0;
}
C
cat >"$T/src/strong.c" <<'C'
#include <nros/platform_can.h>
int8_t nros_platform_can_open(nros_platform_can_t* can, const char* device, uint8_t want_fd) {
    (void)can; (void)device; (void)want_fd;
    return NROS_PLATFORM_CAN_OK;
}
C

# configure_build <build dir> <board key> <board crate dir> [extra source]
configure_build() {
    local dir="$1" board="$2" bdir="$3" extra="${4:-}" stack
    stack="$(descriptor_stack "$bdir")"
    env -u NROS_LEAF_CENSUS_HOST cmake -S "$T/src" -B "$dir" \
        -DTEST_BOARD="$board" -DTEST_BOARD_STACK="$stack" \
        -DTEST_EXTRA_SRC="$extra" \
        -DCMAKE_C_FLAGS="-std=c11 -Wall -Wextra -Werror" >"$dir.log" 2>&1 \
        && cmake --build "$dir" >>"$dir.log" 2>&1
}
AN536="$ROOT/packages/boards/nros-board-mps3-an536-freertos"
POSIX="$ROOT/packages/boards/nros-board-freertos-posix"

echo "A  mps3-an536-freertos: an entry that calls the driver links, open fails loud"
if configure_build "$T/a" mps3-an536-freertos "$AN536"; then
    out="$("$T/a/entry")"
    if nros_grep_q "board \`mps3-an536-freertos\` defines no nros_platform_can_" <<<"$out"; then
        ok "the line names the board"
    else
        fail "the line does not name the board: $out"
    fi
    if nros_grep_q "^open=-6 send=-6 recv=-6 fd=0\$" <<<"$out"; then
        ok "open, send and recv return ENOTSUP; fd_active is 0"
    else
        fail "unexpected codes: $out"
    fi
else
    fail "the AN536 entry does not link: $(tail -5 "$T/a.log")"
fi

echo "B  a strong open in another object wins"
if configure_build "$T/b" mps3-an536-freertos "$AN536" "$T/src/strong.c"; then
    out="$("$T/b/entry")"
    rc_open=0
    nros_grep_q "^open=0 " <<<"$out" || rc_open=$?
    rc_line=0
    nros_grep_q "defines no" <<<"$out" || rc_line=$?
    if [ "$rc_open" -eq 0 ] && [ "$rc_line" -eq 1 ]; then
        ok "the strong definition is the one called"
    else
        fail "the weak default shadowed the strong one: $out"
    fi
else
    fail "the image with a strong open does not link: $(tail -5 "$T/b.log")"
fi

echo "C  negative control: a host-stack board gets no defaults"
if ! configure_build "$T/c" freertos-posix "$POSIX"; then
    rc=0
    nros_grep_q "undefined reference to .nros_platform_can_open" "$T/c.log" || rc=$?
    if [ "$rc" -eq 0 ]; then
        ok "freertos-posix links no defaults (its backend is SocketCAN)"
    else
        fail "the control failed for another reason: $(tail -5 "$T/c.log")"
    fi
else
    fail "a host-stack board compiled the weak defaults"
fi

echo "D  every in-tree board with no CAN backend is covered on the cargo road"
# Board crates whose cargo road compiles no board C glue for the defaults to
# ride in: mps2-an385 (bare metal, Rust-only startup) and nuttx-qemu (NuttX's
# glue is its own export build). Their cmake road is covered (the rule is the
# stack, A).
CARGO_EXEMPT=" nros-board-mps2-an385 nros-board-nuttx-qemu "
for runner in freertos_build threadx_qemu_riscv64_build; do
    rc=0
    nros_grep_q "can_unsupported::add_defaults" \
        "packages/boards/nros-board-common/src/$runner.rs" || rc=$?
    if [ "$rc" -eq 0 ]; then
        ok "the $runner glue runner compiles the defaults"
    else
        fail "the $runner glue runner does not compile the defaults"
    fi
done
for toml in packages/boards/*/nros-board.toml; do
    dir="${toml%/nros-board.toml}"
    name="${dir##*/}"
    stack="$(descriptor_stack "$dir")"
    platform="$(sed -n 's/^platform *= *"\([a-z_-]*\)".*/\1/p' "$toml" | head -1)"
    [ "$stack" = host ] && continue
    # A Zephyr board has the Zephyr backend and never reaches the rule.
    [ "$platform" = zephyr ] && continue
    case "$CARGO_EXEMPT" in
        *" $name "*)
            ok "$name: stack \"$stack\": cmake road only (no board C glue on cargo)"
            continue
            ;;
    esac
    rc=0
    nros_grep_q "freertos_build::\|threadx_qemu_riscv64_build::run" "$dir/build.rs" || rc=$?
    if [ "$rc" -eq 0 ]; then
        ok "$name: stack \"$stack\": both roads"
    else
        fail "$name: stack \"$stack\", and its build script runs no glue runner that compiles the CAN defaults"
    fi
done

if [ "$FAILURES" -ne 0 ]; then
    echo "can-board-defaults: $FAILURES failure(s)"
    exit 1
fi
echo "can-board-defaults: all assertions held"
