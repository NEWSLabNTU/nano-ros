#!/usr/bin/env bash
# RFC-0132 / phase-560 W9.b -- the CAN frame driver's `zephyr` backend.
#
# Builds tests/zephyr-can-smoke for native_sim twice and runs each against a
# host SocketCAN interface, reading the bus with `candump`:
#
#   fd.overlay       `nros,can` chosen, `bitrate-data` stated -> can_start.c
#                    puts the controller in CAN FD mode; the image sends the
#                    classic frame 0x123, the FD frame 0x1ABCDEF0 (EXT, BRS,
#                    64 bytes) and 0x124 with SEND_WAIT_TX, read byte for byte.
#   classic.overlay  `zephyr,canbus` only, no data bitrate -> classic mode;
#                    `open(want_fd)` gives `fd_active` false, the classic frame
#                    goes out and the FD send returns ENOFD (nothing FD on the
#                    bus).
#
# Then, with --s32k344, compiles the same app for `mr_canhubk3` (the S32K344,
# issue 2573's board: FlexCAN 0 as `nros,can`, FD path included). A compile
# check; no board runs it.
#
#   ./scripts/test/can-frame-zephyr.sh [--s32k344] [vcan0]
#
# Needs: a Zephyr west workspace (just zephyr setup), the interface up with the
# CAN FD MTU (scripts/test/vcan-setup.sh), `candump`. A missing precondition is
# a REPORTED skip (`nros_check_unverified`), never a pass.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
# shellcheck source=../build/check-skip.sh
source scripts/build/check-skip.sh
# shellcheck source=../lib/deadline.sh
. scripts/lib/deadline.sh
# shellcheck source=../lib/grep-q.sh
source scripts/lib/grep-q.sh

NAME=can-frame-zephyr
S32K344=0
IFACE=vcan0
for arg in "$@"; do
    case "$arg" in
        --s32k344) S32K344=1 ;;
        -*) echo "unknown option: $arg" >&2; exit 2 ;;
        *) IFACE="$arg" ;;
    esac
done
APP=tests/zephyr-can-smoke

WORKSPACE="$(scripts/lib/zephyr-workspace.sh resolve-or-default)"
if [ ! -f "$WORKSPACE/env.sh" ]; then
    nros_check_unverified "$NAME" "no Zephyr west workspace at $WORKSPACE (env.sh absent) -- just zephyr setup" || exit 1
    exit 0
fi
if ! command -v candump >/dev/null 2>&1; then
    nros_check_unverified "$NAME" "no candump on PATH (install can-utils)" || exit 1
    exit 0
fi
flags="$(cat "/sys/class/net/$IFACE/flags" 2>/dev/null)" || flags=0
if (( (flags & 0x1) == 0 )) || [ "$(cat "/sys/class/net/$IFACE/mtu" 2>/dev/null)" != 72 ]; then
    nros_check_unverified "$NAME" "$IFACE is not an up CAN FD interface (MTU 72) -- scripts/test/vcan-setup.sh $IFACE" || exit 1
    exit 0
fi
if [ "$IFACE" != vcan0 ]; then
    # The overlays' `host-interface` names vcan0, and Zephyr 3.7's native_sim
    # has no `--can-if` to override it at run time.
    echo "[$NAME] the overlays name vcan0; $IFACE is not supported" >&2
    exit 2
fi

# The Zephyr build environment, as `just zephyr build-c-port` sets it up.
source scripts/build/jobserver-pool.sh
make_bin="$(nros_pinned_make)" || make_bin="$(command -v make)"
source scripts/lib/sdk-tool-path.sh
sdk_tool_prefix="$(nros_sdk_tool_path_prefix make ninja)"
export PATH="${sdk_tool_prefix:+$sdk_tool_prefix:}$PATH"
# shellcheck disable=SC1091
source "$WORKSPACE/env.sh"
source scripts/build/zephyr-python.sh
nros_zephyr_activate
source scripts/build/zephyr-toolchain.sh

OUT="$(mktemp -d)"
DUMP_PID=""
cleanup() { [ -n "$DUMP_PID" ] && kill "$DUMP_PID" 2>/dev/null; }
trap cleanup EXIT
fail=0

build() { # <board> <build-dir> [cmake args...]
    local board="$1" bdir="$2"; shift 2
    ZEPHYR_TOOLCHAIN_VARIANT="$(nros_zephyr_toolchain_variant "$board")" \
        "${NROS_DEADLINE[@]}" 1200 west build -p always -b "$board" -d "$bdir" "$APP" \
        -- -DMAKE="$make_bin" "$@" >"$OUT/build-$(basename "$bdir").log" 2>&1
}

fd_data="$(for i in $(seq 0 63); do printf '%02X' "$i"; done)"

run_shape() { # <shape>
    local shape="$1" bdir="$APP/build-$1" dump
    echo "[$NAME] building native_sim, $shape.overlay"
    if ! build native_sim/native/64 "$bdir" -DDTC_OVERLAY_FILE="$shape.overlay"; then
        tail -30 "$OUT/build-build-$shape.log" >&2
        echo "[$NAME] FAIL: $shape build" >&2; fail=1; return
    fi
    stdbuf -o0 candump -L "$IFACE,123:7FF,124:7FF,1ABCDEF0:1FFFFFFF" >"$OUT/dump-$shape.log" 2>&1 &
    DUMP_PID=$!
    sleep 1
    if ! "${NROS_DEADLINE[@]}" 30 "$bdir/zephyr/zephyr.exe" >"$OUT/$shape.log" 2>&1; then
        echo "[$NAME] FAIL: the $shape image exited non-zero:" >&2
        sed 's/^/    /' "$OUT/$shape.log" >&2; fail=1
    fi
    sleep 1
    kill "$DUMP_PID" 2>/dev/null; wait "$DUMP_PID" 2>/dev/null; DUMP_PID=""
    sed -n 's/^\(CAN-SMOKE: .*\)$/    \1/p' "$OUT/$shape.log"
    dump="$(cat "$OUT/dump-$shape.log")"
    local ok=1
    nros_grep_q " $IFACE 123#1122334455667788\$" <<<"$dump" || ok=0
    if [ "$shape" = fd ]; then
        nros_grep_q -E " $IFACE 1ABCDEF0##[15]$fd_data\$" <<<"$dump" || ok=0
        nros_grep_q " $IFACE 124#5A\$" <<<"$dump" || ok=0
    else
        # Classic mode: the FD frame must NOT have reached the bus.
        nros_grep_q "1ABCDEF0" <<<"$dump" && ok=0
    fi
    if [ "$ok" = 1 ]; then
        echo "[$NAME] $shape: candump read exactly the expected frames, byte for byte"
    else
        echo "[$NAME] FAIL: $shape: the bus did not carry the expected frames:" >&2
        sed 's/^/    /' <<<"$dump" >&2; fail=1
    fi
}

run_shape fd
run_shape classic

if [ "$S32K344" = 1 ]; then
    echo "[$NAME] building mr_canhubk3 (S32K344), boards/mr_canhubk3.overlay"
    # The west workspace fetches hal_nxp (the S32 pinctrl headers) without
    # registering it as a module (nros-sdk-index.toml: no in-tree S32 board),
    # so it is named here for this one build.
    if build mr_canhubk3 "$APP/build-s32k344" -DEXTRA_ZEPHYR_MODULES="$WORKSPACE/modules/hal/nxp"; then
        echo "[$NAME] s32k344: the zephyr backend and can_start.c compile for FlexCAN"
    else
        tail -30 "$OUT/build-build-s32k344.log" >&2
        echo "[$NAME] FAIL: mr_canhubk3 build" >&2; fail=1
    fi
fi

[ "$fail" -eq 0 ] && { echo "[$NAME] PASS"; rm -rf "$OUT"; exit 0; }
echo "[$NAME] FAIL. Logs kept in $OUT" >&2
exit 1
