#!/usr/bin/env bash
# Build and run the ROS-2-over-CAN demo (RFC-0082 / phase-387).
#
#   docker/can-demo/run.sh --zenoh <path-to-zenoh-fork> [--negative|--unicast] [--build-only]
#
#   --zenoh <dir>   checkout of the zenoh fork on branch feat/can-links-ros,
#                   which carries BOTH link crates. Defaults to $ZENOH_DIR.
#   --pico <dir>    zenoh-pico checkout. Defaults to the vendored submodule.
#   --negative      run the deliberately-broken variant, which must FAIL to
#                   communicate -- this is how we know the assertions fire
#   --unicast       run the ISO-TP demo: a ROS 2 SERVICE CALL over CAN, and the
#                   same call over the multicast link, which must fail
#   --build-only    build the image and stop
#
# The host must have the vcan kernel module available, and --unicast also needs
# can-isotp. The container creates its own vcan0 in its own network namespace
# with --cap-add=NET_ADMIN; it takes no other privileges and needs no CAN
# interface on the host.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NROS_ROOT="$(cd "$HERE/../.." && pwd)"
ZENOH_DIR="${ZENOH_DIR:-}"
PICO_DIR="$NROS_ROOT/packages/rmw/zenoh/zpico-sys/zenoh-pico"
IMAGE="nros-can-demo"
MODE=""
BUILD_ONLY=0

while [ $# -gt 0 ]; do
    case "$1" in
        --zenoh) ZENOH_DIR="$2"; shift 2 ;;
        --pico)  PICO_DIR="$2"; shift 2 ;;
        --negative) MODE="--negative"; shift ;;
        --unicast)  MODE="--unicast"; shift ;;
        --build-only) BUILD_ONLY=1; shift ;;
        -h | --help)
            awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"
            exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

die() { echo "[can-demo] error: $*" >&2; exit 1; }

[ -n "$ZENOH_DIR" ] || die "--zenoh is required (or set ZENOH_DIR): the zenoh fork carrying both CAN links"
ZENOH_DIR="$(cd "$ZENOH_DIR" 2>/dev/null && pwd)" || die "--zenoh path does not exist"
# BOTH crates. The image carries one libzenohc.so with both links because the
# demo's point is the contrast between them.
for crate in zenoh-link-can zenoh-link-isotp; do
    [ -d "$ZENOH_DIR/io/zenoh-links/$crate" ] \
        || die "$ZENOH_DIR has no io/zenoh-links/$crate. Wrong checkout, or wrong
     branch -- feat/can-links-ros is the one carrying both."
done
[ -d "$PICO_DIR/include/zenoh-pico" ] || die "$PICO_DIR is not a zenoh-pico checkout"

# The container reproduces what rmw_zenoh ships, which is zenoh 2687c5135 -- a
# commit on neither main nor release/1.8.0. Building from a checkout based on
# anything else produces a demo that does not represent the deployed stack.
EXPECTED_BASE=2687c51352121f006e3a603ce07925a8ad0b295c
if git -C "$ZENOH_DIR" cat-file -e "$EXPECTED_BASE^{commit}" 2>/dev/null; then
    if ! git -C "$ZENOH_DIR" merge-base --is-ancestor "$EXPECTED_BASE" HEAD 2>/dev/null; then
        echo "[can-demo] WARNING: $ZENOH_DIR is not based on $EXPECTED_BASE," >&2
        echo "           the zenoh revision rmw_zenoh actually builds. The demo will" >&2
        echo "           still run, but it no longer reproduces the deployed stack." >&2
    fi
else
    echo "[can-demo] note: $EXPECTED_BASE not present locally; cannot check the base" >&2
fi

if ! modinfo vcan >/dev/null 2>&1 && ! lsmod 2>/dev/null | grep -q '^vcan'; then
    die "the host has no vcan kernel module. The container cannot load it -- it
     takes no privileges beyond NET_ADMIN by design.
       sudo modprobe vcan
     (Debian/Ubuntu may need linux-modules-extra-\$(uname -r))"
fi

# can-isotp is a SEPARATE module from vcan, and only --unicast needs it. Checked
# here rather than inside the container so the failure names the host command
# that fixes it.
if [ "$MODE" = "--unicast" ] \
   && ! grep -qw can_isotp /proc/modules 2>/dev/null && [ ! -d /sys/module/can_isotp ]; then
    die "--unicast needs the can-isotp kernel module, which is not loaded.
       sudo modprobe can-isotp
     (Debian/Ubuntu may need linux-modules-extra-\$(uname -r))"
fi

echo "[can-demo] building $IMAGE"
DOCKER_BUILDKIT=1 docker build \
    --build-context "zenoh=$ZENOH_DIR" \
    --build-context "zenohpico=$PICO_DIR" \
    -t "$IMAGE" \
    "$HERE"

[ "$BUILD_ONLY" -eq 1 ] && { echo "[can-demo] built, not running"; exit 0; }

echo "[can-demo] running"
exec docker run --rm --cap-add=NET_ADMIN "$IMAGE" $MODE
