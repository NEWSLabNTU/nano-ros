#!/usr/bin/env bash
# RFC-0132 / phase-560 W9.c -- the CAN frame driver's `socketcan` backend on
# the two Linux-process boards that run it: a `linux` process and a
# `freertos-posix` process (the board's FreeRTOS kernel, a FreeRTOS task
# sending) each send a classic and an FD frame on a host SocketCAN interface,
# and `candump` must read both, byte for byte.
#
#   ./scripts/test/can-frame-socketcan.sh [<fd-iface> [<classic-iface>]]
#
#   <fd-iface>       default vcan0; must be up with the CAN FD MTU (72)
#   <classic-iface>  optional; up with the classic MTU (16). With it, each probe
#                    also checks that `open(want_fd)` is refused (ENOFD) and a
#                    classic open has `fd_active` false. Without it, the same
#                    check runs in a private user + network namespace
#                    (`unshare -rn`) over its own vcan pair, when the kernel
#                    allows one; nothing is created on the host.
#
# Needs: the interfaces (scripts/test/vcan-setup.sh creates vcan0; a classic one
# is `ip link add dev vcan1 type vcan && ip link set vcan1 mtu 16 up`, root),
# `candump` (can-utils), the FreeRTOS kernel (`nros setup --source
# freertos-kernel`). A missing precondition is a REPORTED skip
# (`nros_check_unverified`), never a pass; under NROS_CHECK_SKIP_STRICT=1 it
# fails. The frame ids are 0x123 and 0x1ABCDEF0: the interface may be shared.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
# shellcheck source=../build/check-skip.sh
source scripts/build/check-skip.sh
# shellcheck source=../build/cargo.sh
source scripts/build/cargo.sh
# shellcheck source=../lib/deadline.sh
. scripts/lib/deadline.sh
# shellcheck source=../lib/grep-q.sh
source scripts/lib/grep-q.sh

NAME=can-frame-socketcan
IFACE="${1:-vcan0}"
CLASSIC="${2:-}"

mtu_of() { cat "/sys/class/net/$1/mtu" 2>/dev/null; }
# IFF_UP, read from sysfs: no `ip | grep` pipeline to misread (issue 0726).
is_up() {
    local flags
    flags="$(cat "/sys/class/net/$1/flags" 2>/dev/null)" || return 1
    (( flags & 0x1 ))
}

if ! command -v candump >/dev/null 2>&1; then
    nros_check_unverified "$NAME" "no candump on PATH (install can-utils)" || exit 1
    exit 0
fi
if ! is_up "$IFACE" || [ "$(mtu_of "$IFACE")" != 72 ]; then
    nros_check_unverified "$NAME" "$IFACE is not an up CAN FD interface (MTU 72) -- scripts/test/vcan-setup.sh $IFACE" || exit 1
    exit 0
fi
probe_args=("$IFACE")
if [ -n "$CLASSIC" ]; then
    if ! is_up "$CLASSIC" || [ "$(mtu_of "$CLASSIC")" != 16 ]; then
        echo "[$NAME] FAIL: $CLASSIC is not an up classic CAN interface (MTU 16)" >&2
        exit 1
    fi
    probe_args+=("$CLASSIC")
fi
freertos_dir="$(nros_locate_source freertos-kernel 2>/dev/null)" || freertos_dir=""

OUT="$(mktemp -d)"
DUMP_PID=""
cleanup() { [ -n "$DUMP_PID" ] && kill "$DUMP_PID" 2>/dev/null; }
trap cleanup EXIT

# Build first, so the bus capture covers only the probes' own runs.
echo "[$NAME] building the linux probe"
"${NROS_DEADLINE[@]}" 1200 cargo test -q -p nros-board-linux --features can-probe \
    --test can_socketcan_probe --no-run >"$OUT/build-linux.log" 2>&1 \
    || { tail -30 "$OUT/build-linux.log" >&2; echo "[$NAME] FAIL: linux probe build" >&2; exit 1; }
freertos_env=(FREERTOS_PORT=GCC_POSIX
    "FREERTOS_CONFIG_DIR=$ROOT/packages/boards/nros-board-freertos-posix/config")
if [ -n "$freertos_dir" ]; then
    echo "[$NAME] building the freertos-posix probe"
    env "${freertos_env[@]}" "${NROS_DEADLINE[@]}" 1200 cargo test -q -p nros-board-freertos-posix \
        --features can-probe --test can_socketcan_probe --no-run >"$OUT/build-freertos.log" 2>&1 \
        || { tail -30 "$OUT/build-freertos.log" >&2; echo "[$NAME] FAIL: freertos-posix probe build" >&2; exit 1; }
fi

# Capture only the probes' two ids (the interface may carry other traffic).
stdbuf -o0 candump -L "$IFACE,123:7FF,1ABCDEF0:1FFFFFFF" >"$OUT/dump.log" 2>&1 &
DUMP_PID=$!
sleep 1

expected_classic="123#1122334455667788"
fd_data="$(for i in $(seq 0 63); do printf '%02X' "$i"; done)"
fail=0

run_probe() { # <label> <env...> -- cargo args
    local label="$1"; shift
    local before after
    before="$(wc -l <"$OUT/dump.log")"
    if ! "$@" -- "${probe_args[@]}" >"$OUT/$label.log" 2>&1; then
        echo "[$NAME] FAIL: $label probe exited non-zero" >&2
        sed 's/^/    /' "$OUT/$label.log" >&2
        fail=1
        return
    fi
    sed 's/^/    /' "$OUT/$label.log" | grep "CAN-PROBE"
    sleep 1
    after="$(tail -n +"$((before + 1))" "$OUT/dump.log")"
    # The kernel adds CANFD_FDF to the flags on newer releases, so the FD
    # line's flags digit is 1 (BRS) or 5 (BRS|FDF); the bytes are exact.
    if nros_grep_q " $IFACE $expected_classic\$" <<<"$after" \
        && nros_grep_q -E " $IFACE 1ABCDEF0##[15]$fd_data\$" <<<"$after"; then
        echo "[$NAME] $label: candump read the classic and the FD frame, byte for byte"
    else
        echo "[$NAME] FAIL: $label: candump did not read both frames:" >&2
        sed 's/^/    /' <<<"$after" >&2
        fail=1
    fi
}

run_probe linux "${NROS_DEADLINE[@]}" 120 cargo test -q -p nros-board-linux \
    --features can-probe --test can_socketcan_probe
if [ -n "$freertos_dir" ]; then
    run_probe freertos-posix env "${freertos_env[@]}" "${NROS_DEADLINE[@]}" 120 cargo test -q \
        -p nros-board-freertos-posix --features can-probe --test can_socketcan_probe
else
    nros_check_unverified "$NAME-freertos-posix" "the FreeRTOS kernel is not provisioned -- nros setup --source freertos-kernel" || fail=1
fi
# The classic-interface check, when no classic interface was given: in a
# private user + network namespace (`unshare -rn`, no root), with its own FD
# and classic vcan, so nothing is created on the host. Only the open-time
# refusal is checked there; the bus capture above stays the host's.
netns_classic() { # <label> -- cargo args
    local label="$1"; shift
    unshare -rn env CARGO_NET_OFFLINE=true bash -c '
        ip link add dev vcanfd type vcan && ip link set vcanfd mtu 72 up &&
        ip link add dev vcancl type vcan && ip link set vcancl mtu 16 up &&
        exec "$@" -- vcanfd vcancl' _ "$@" >"$OUT/$label-netns.log" 2>&1
}
if [ -z "$CLASSIC" ]; then
    if unshare -rn true 2>/dev/null; then
        for label in linux freertos-posix; do
            if [ "$label" = linux ]; then
                cmd=("${NROS_DEADLINE[@]}" 120 cargo test -q -p nros-board-linux
                    --features can-probe --test can_socketcan_probe)
            elif [ -n "$freertos_dir" ]; then
                cmd=(env "${freertos_env[@]}" "${NROS_DEADLINE[@]}" 120 cargo test -q
                    -p nros-board-freertos-posix --features can-probe --test can_socketcan_probe)
            else
                continue
            fi
            if netns_classic "$label" "${cmd[@]}" \
                && nros_grep_q "want_fd refused (ENOFD), fd_active false" <"$OUT/$label-netns.log"; then
                echo "[$NAME] $label: a classic (MTU 16) interface refuses open(want_fd) with ENOFD (private netns)"
            else
                echo "[$NAME] FAIL: $label: the classic-interface check in a private netns:" >&2
                sed 's/^/    /' "$OUT/$label-netns.log" >&2
                fail=1
            fi
        done
    else
        nros_check_unverified "$NAME-classic" "no classic interface given and no private network namespace (unshare -rn): open(want_fd) on a classic (MTU 16) interface was not checked" || fail=1
    fi
fi

[ "$fail" -eq 0 ] && { echo "[$NAME] PASS"; rm -rf "$OUT"; exit 0; }
echo "[$NAME] FAIL. Logs kept in $OUT" >&2
exit 1
