#!/usr/bin/env bash
# tests/zephyr-socket-service-stack-tests.sh -- issue 2785.
#
# WHAT THIS MEASURES
#
# Zephyr puts the socket service thread's whole poll array on that thread's
# stack (one k_poll_event per CONFIG_NET_SOCKETS_POLL_MAX entry). A Cyclone
# image with POLL_MAX=256 and a 4096-byte service stack overflowed it by
# 8.7 KB into .bss, over nano-ros's log dispatch guard: every log record was
# then queued and none printed. The module now refuses that configure
# (`nros_check_socket_service_stack`, zephyr/cmake/nros_socket_service_stack.cmake).
#
#   A  the measured image (64-bit, POLL_MAX=256, stack 4096) is refused, and
#      the message names the stack knob, the poll knob and the 13312-byte floor
#   B  the same image with a 16384-byte stack configures
#   C  a 32-bit image on the defaults nano-ros ships (POLL_MAX=8, Zephyr's
#      1200-byte stack) configures: the check does not refuse a working image
#   D  an image with no socket service thread, and a native_sim image
#      (CONFIG_ARCH_POSIX, host pthread stacks), are not judged
#   E  Zephyr 4.x's spelling (CONFIG_ZVFS_POLL_MAX) is read too
#   F  REACH: zephyr/CMakeLists.txt calls the check on every IP image
#
# A probe project that includes the module file, so seconds; no Zephyr
# workspace. PRECONDITIONS ARE HARD FAILURES (issue 1758): no cmake, red.
#
# Usage: ./tests/zephyr-socket-service-stack-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MODULE="$PROJECT_ROOT/zephyr/cmake/nros_socket_service_stack.cmake"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

FAILURES=0
fail() {
    echo "FAIL: $*" >&2
    FAILURES=$((FAILURES + 1))
}

command -v cmake >/dev/null 2>&1 || { echo "FAIL: cmake not found" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/src"
printf '%s\n' \
    'cmake_minimum_required(VERSION 3.22)' \
    'project(probe NONE)' \
    'include("${MODULE}")' \
    'nros_check_socket_service_stack()' \
    'message(STATUS "PROBE_CONFIGURED")' \
    > "$WORK/src/CMakeLists.txt"

# configure -DCONFIG_...=...: prints cmake's output; returns cmake's status.
configure() {
    rm -rf "$WORK/b"
    cmake -S "$WORK/src" -B "$WORK/b" "-DMODULE=$MODULE" "$@" 2>&1
}

ASI=(-DCONFIG_NET_SOCKETS_SERVICE=y -DCONFIG_64BIT=y -DCONFIG_NET_SOCKETS_POLL_MAX=256)

if out="$(configure "${ASI[@]}" -DCONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=4096)"; then
    fail "A: a 4096-byte service stack with POLL_MAX=256 configured"
else
    flat="$(printf '%s' "$out" | tr -s ' \n' ' ')"
    for want in 'CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=4096' \
                'CONFIG_NET_SOCKETS_POLL_MAX=256' 'at least 13312 bytes'; do
        [[ "$flat" == *"$want"* ]] || fail "A: the refusal does not say '$want': $flat"
    done
fi

out="$(configure "${ASI[@]}" -DCONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=16384)" ||
    fail "B: a 16384-byte service stack was refused: $out"

out="$(configure -DCONFIG_NET_SOCKETS_SERVICE=y -DCONFIG_NET_SOCKETS_POLL_MAX=8 \
    -DCONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=1200)" ||
    fail "C: a 32-bit image on the default stack and POLL_MAX=8 was refused: $out"

out="$(configure -DCONFIG_64BIT=y -DCONFIG_NET_SOCKETS_POLL_MAX=256)" ||
    fail "D: an image with no socket service thread was judged: $out"
out="$(configure "${ASI[@]}" -DCONFIG_ARCH_POSIX=y -DCONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=1200)" ||
    fail "D: a native_sim image was judged: $out"

if out="$(configure -DCONFIG_NET_SOCKETS_SERVICE=y -DCONFIG_64BIT=y -DCONFIG_ZVFS_POLL_MAX=64 \
    -DCONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=2048)"; then
    fail "E: CONFIG_ZVFS_POLL_MAX=64 with a 2048-byte stack configured"
elif [[ "$(printf '%s' "$out" | tr -s ' \n' ' ')" != *'CONFIG_ZVFS_POLL_MAX=64'* ]]; then
    fail "E: the 4.x refusal does not name CONFIG_ZVFS_POLL_MAX: $out"
fi

nros_grep_q 'nros_check_socket_service_stack()' "$PROJECT_ROOT/zephyr/CMakeLists.txt" ||
    fail "F: zephyr/CMakeLists.txt does not call nros_check_socket_service_stack"

if [ "$FAILURES" -ne 0 ]; then
    echo "zephyr-socket-service-stack: $FAILURES failure(s)" >&2
    exit 1
fi
echo "zephyr-socket-service-stack: OK (a socket service stack too small for its poll array is refused)"
