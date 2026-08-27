#!/usr/bin/env bash
# RFC-0083 / phase-394 W0 — prove the vendored ISO-TP library is usable on an
# MCU: it must build for a bare-metal Cortex-M target and pull in NO allocator.
#
#   ./scripts/can/isotp-c-mcu-check.sh [--cpu cortex-m4]
#
# The no-allocator property is the reason this library was chosen over the
# alternatives, so it is checked rather than assumed. The caller supplies both
# buffers to `isotp_init_link`; nothing inside allocates.
set -euo pipefail

CPU=cortex-m4
while [ $# -gt 0 ]; do
    case "$1" in
        --cpu) CPU="$2"; shift 2 ;;
        -h | --help) awk 'NR>1 && /^#/ { sub(/^# ?/,""); print; next } NR>1 { exit }' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

CC=${ISOTP_C_CC:-arm-none-eabi-gcc}
NM=${ISOTP_C_NM:-arm-none-eabi-nm}
SRC="packages/rmw/zenoh/zpico-sys/zenoh-pico/third_party/isotp-c"
OBJ="$(mktemp -d)/isotp-$CPU.o"
trap 'rm -rf "$(dirname "$OBJ")"' EXIT

command -v "$CC" >/dev/null 2>&1 || {
    echo "[isotp-c] $CC not installed — skipping (Debian/Ubuntu: apt install gcc-arm-none-eabi)"
    exit 0
}
[ -f "$SRC/isotp.c" ] || { echo "[isotp-c] $SRC/isotp.c missing" >&2; exit 1; }

echo "[isotp-c] building for $CPU, bare metal"
"$CC" -c -O2 -mcpu="$CPU" -mthumb -ffreestanding -I"$SRC" -o "$OBJ" "$SRC/isotp.c"

# `grep -c` rather than `grep -q`: -q closes the pipe early, and under a
# `set -o pipefail` caller nm takes SIGPIPE and the pipeline fails ON A MATCH.
ALLOC="$("$NM" -u "$OBJ" | grep -c -E '\b(m|c|re)alloc\b|\bfree\b' || true)"
if [ "$ALLOC" -ne 0 ]; then
    echo "[isotp-c] FAIL: the library references an allocator:" >&2
    "$NM" -u "$OBJ" | grep -E '\b(m|c|re)alloc\b|\bfree\b' >&2
    exit 1
fi

echo "[isotp-c] PASS: no allocator. External symbols it does need:"
"$NM" -u "$OBJ" | sed 's/^/    /'
