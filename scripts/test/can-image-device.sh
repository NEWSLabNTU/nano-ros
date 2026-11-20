#!/usr/bin/env bash
# RFC-0132 D4 / phase-560 W9.e -- the image's CAN frame device reaches the
# backend's `open(NULL)` on the cargo road and on the cmake road.
#
#   ./scripts/test/can-image-device.sh [<iface>]
#
#   <iface>  default vcan0; an up SocketCAN interface the second image of each
#            road names and sends one classic frame on.
#
# Two fixture workspaces, one per road, each with two `linux` images:
#
#   tests/can-image-device/rust  (cargo road: a component, `nros::main!` entry)
#   tests/can-image-device/c     (cmake road: a C entry, `nano_ros_add_executable`)
#
#   image `linux`        [image.linux.can] device = "vcan7", a device no host
#                        has: the probe must report `device=vcan7` and ENODEV
#                        (-7), so the name came from system.toml and nowhere
#                        else (a default would be vcan0, or none).
#   image `linux_vcan0`  device = <iface>: `open(NULL)` opens it and sends
#                        0x263#26330E00CAFE0102, which candump must read.
#   image `freertos_posix_vcan0` (cmake road only): the same on a cross board,
#                        built with its host census ON; that census build
#                        links the CAN mock (RFC-0132 D6), and its run must
#                        open the mock.
#
# The cargo-road images open an executor before the component registers, so a
# zenoh router runs for them on a private port (scripts/dev/zenohd.sh). The
# west road needs no probe here: its device is the devicetree's
# (`nros,can`, else `zephyr,canbus`), opened by `scripts/test/can-frame-zephyr.sh`.
#
# Needs: this checkout's `nros` (just setup-cli), the interface, `candump` (can-utils), and the
# ROS zenoh router. A missing precondition is a REPORTED skip
# (`nros_check_unverified`), never a pass; under NROS_CHECK_SKIP_STRICT=1 it
# fails. Nothing is created on the host.
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
# shellcheck source=../dev/zenohd.sh
source scripts/dev/zenohd.sh

NAME=can-image-device
IFACE="${1:-vcan0}"
FIXTURE="$ROOT/tests/can-image-device"
EXPECTED="263#26330E00CAFE0102"

is_up() {
    local flags
    flags="$(cat "/sys/class/net/$1/flags" 2>/dev/null)" || return 1
    (( flags & 0x1 ))
}

if ! NROS="$(nros_cli_bin --checkout-only)"; then
    nros_check_unverified "$NAME" "this checkout's nros CLI is not built (just setup-cli)" || exit 1
    exit 0
fi
if ! command -v candump >/dev/null 2>&1; then
    nros_check_unverified "$NAME" "no candump on PATH (install can-utils)" || exit 1
    exit 0
fi
if ! is_up "$IFACE"; then
    nros_check_unverified "$NAME" "$IFACE is not an up CAN interface -- scripts/test/vcan-setup.sh $IFACE" || exit 1
    exit 0
fi
if ! nros_zenohd_bin >/dev/null 2>&1; then
    nros_check_unverified "$NAME" "no zenoh router for the cargo-road images (source a ROS 2 setup with rmw_zenoh_cpp)" || exit 1
    exit 0
fi

OUT="$(mktemp -d)"
ROUTER_PID=""
DUMP_PID=""
cleanup() {
    [ -n "$DUMP_PID" ] && kill "$DUMP_PID" 2>/dev/null
    [ -n "$ROUTER_PID" ] && kill "$ROUTER_PID" 2>/dev/null
    wait 2>/dev/null
}
trap cleanup EXIT
fail=0

# The fixture's `linux_vcan0` images name vcan0; another interface is written
# into a copy, never into the tracked fixture.
WORK="$OUT/fixture"
cp -r "$FIXTURE" "$WORK"
rm -rf "$WORK"/*/build "$WORK"/*/.cargo "$WORK"/*/nros-sdk.lock
if [ "$IFACE" != vcan0 ]; then
    sed -i "s/device = \"vcan0\"/device = \"$IFACE\"/" "$WORK"/*/src/*_bringup/system.toml
fi

build() { # <road> <image> -> prints the executable
    local road="$1" image="$2" bin
    if ! (cd "$WORK/$road" && "${NROS_DEADLINE[@]}" 1800 "$NROS" build "$image") \
        >"$OUT/build-$road-$image.log" 2>&1; then
        tail -30 "$OUT/build-$road-$image.log" >&2
        echo "[$NAME] FAIL: $road road: nros build $image" >&2
        return 1
    fi
    case "$road" in
        rust) bin="$(find "$WORK/rust/build" -type f -perm -u+x -path "*/${image}_entry/target/*/${image}_entry" | head -1)" ;;
        c) bin="$(find "$WORK/c/build" -type f -perm -u+x -name can_image_probe_c -path "*/cmake/pkg/*" \
                -path "*-${BOARD_OF[$image]:-linux}-*" | head -1)" ;;
    esac
    [ -n "$bin" ] || { echo "[$NAME] FAIL: $road road: no executable for $image" >&2; return 1; }
    echo "$bin"
}

run() { # <road> <image> <bin> -> the probe's CAN-IMAGE lines in $OUT/<road>-<image>.log
    local road="$1" image="$2" bin="$3" log pid i
    log="$OUT/$road-$image.log"
    if [ "$road" = rust ]; then
        NROS_LOCATOR="tcp/127.0.0.1:$PORT" "$bin" >"$log" 2>&1 &
        pid=$!
        for i in $(seq 1 150); do
            nros_grep_q "CAN-IMAGE:.*open" "$log" && break
            kill -0 "$pid" 2>/dev/null || break
            sleep 0.1
        done
        sleep 0.5
        kill "$pid" 2>/dev/null
        wait "$pid" 2>/dev/null
    else
        # The C probe brings up a ROS session after its CAN lines; a dead
        # locator makes that fail at once, whatever router the host runs.
        NROS_LOCATOR="tcp/127.0.0.1:9" "${NROS_DEADLINE[@]}" 30 "$bin" >"$log" 2>&1
    fi
    sed 's/^/    /' "$log" | grep "CAN-IMAGE" || sed 's/^/    /' "$log" >&2
}

# The board each image's cmake tree is named after.
declare -A BOARD_OF=([linux]=linux [linux_vcan0]=linux [freertos_posix_vcan0]=freertos-posix)

PORT=$((20000 + RANDOM % 20000))
nros_router_exec "tcp/127.0.0.1:$PORT" >"$OUT/router.log" 2>&1 &
ROUTER_PID=$!
sleep 1

stdbuf -o0 candump -L "$IFACE,263:7FF" >"$OUT/dump.log" 2>&1 &
DUMP_PID=$!
sleep 1

for road in rust c; do
    # The cmake road builds both images into one configure tree, so each
    # image runs right after its own build.
    if bin="$(build "$road" linux)"; then
        run "$road" linux "$bin"
        if nros_grep_q "^CAN-IMAGE: device=vcan7 open(NULL)=-7\$" "$OUT/$road-linux.log"; then
            echo "[$NAME] $road road: image \`linux\` handed vcan7 to open(NULL) (ENODEV, no such device)"
        else
            echo "[$NAME] FAIL: $road road: image \`linux\` did not open vcan7" >&2
            fail=1
        fi
    else
        fail=1
    fi
    if bin="$(build "$road" linux_vcan0)"; then
        before="$(wc -l <"$OUT/dump.log")"
        run "$road" linux_vcan0 "$bin"
        sleep 1
        after="$(tail -n +"$((before + 1))" "$OUT/dump.log")"
        if nros_grep_q "^CAN-IMAGE: device=$IFACE open(NULL)=0\$" "$OUT/$road-linux_vcan0.log" \
            && nros_grep_q " $IFACE $EXPECTED\$" <<<"$after"; then
            echo "[$NAME] $road road: image \`linux_vcan0\` opened $IFACE by open(NULL); candump read its frame"
        else
            echo "[$NAME] FAIL: $road road: image \`linux_vcan0\` did not send on $IFACE:" >&2
            sed 's/^/    /' <<<"$after" >&2
            fail=1
        fi
    else
        fail=1
    fi
done

# RFC-0132 D6 -- the same C probe on `freertos-posix`, a cross board whose C
# entry is sized by its host census. That census build links the CAN mock, so
# a plain `nros build` gets through it (it stopped at an undefined
# `nros_platform_can_open` before), and the image then opens vcan0 by
# `open(NULL)` like the linux one.
if bin="$(build c freertos_posix_vcan0)"; then
    if nros_grep_q "^CAN-IMAGE: device=- open(NULL)=0\$" "$OUT/build-c-freertos_posix_vcan0.log"; then
        echo "[$NAME] c road: image \`freertos_posix_vcan0\`: its host census ran the probe over the CAN mock"
    else
        echo "[$NAME] FAIL: c road: the census run did not open the CAN mock" >&2
        fail=1
    fi
    before="$(wc -l <"$OUT/dump.log")"
    run c freertos_posix_vcan0 "$bin"
    sleep 1
    after="$(tail -n +"$((before + 1))" "$OUT/dump.log")"
    if nros_grep_q "^CAN-IMAGE: device=$IFACE open(NULL)=0\$" "$OUT/c-freertos_posix_vcan0.log" \
        && nros_grep_q " $IFACE $EXPECTED\$" <<<"$after"; then
        echo "[$NAME] c road: image \`freertos_posix_vcan0\` opened $IFACE by open(NULL); candump read its frame"
    else
        echo "[$NAME] FAIL: c road: image \`freertos_posix_vcan0\` did not send on $IFACE:" >&2
        sed 's/^/    /' <<<"$after" >&2
        fail=1
    fi
else
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    echo "[$NAME] FAIL (logs: $OUT)" >&2
    trap - EXIT
    cleanup
    exit 1
fi
rm -rf "$OUT"
echo "[$NAME] PASS: the image's CAN device reached open(NULL) on the cargo and cmake roads, and a CAN-calling C entry got through its host census"
