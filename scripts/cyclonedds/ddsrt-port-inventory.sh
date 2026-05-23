#!/usr/bin/env bash
# Inventory the Cyclone DDS ddsrt RTOS port surface used by Phase 175.B.
#
# This is intentionally read-only. It records what the pinned Cyclone tree
# already provides and what nano-ros still has to supply before RTOS Cyclone
# fixtures may be enabled.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cyclone_dir="${CYCLONEDDS_DIR:-$repo_root/third-party/dds/cyclonedds}"
ddsrt_dir="$cyclone_dir/src/ddsrt"

missing=0

check_file() {
    local path="$1"
    local label="$2"
    if [ -f "$path" ]; then
        printf '  [OK]      %s\n' "$label"
    else
        printf '  [MISSING] %s (%s)\n' "$label" "$path"
        missing=1
    fi
}

check_dir() {
    local path="$1"
    local label="$2"
    if [ -d "$path" ]; then
        printf '  [OK]      %s\n' "$label"
    else
        printf '  [MISSING] %s (%s)\n' "$label" "$path"
        missing=1
    fi
}

echo "Cyclone DDS ddsrt RTOS port inventory"
echo "  Source: $cyclone_dir"

check_file "$ddsrt_dir/CMakeLists.txt" "ddsrt CMakeLists"
check_file "$ddsrt_dir/include/dds/ddsrt/sync/freertos.h" "FreeRTOS sync header"
check_file "$ddsrt_dir/include/dds/ddsrt/threads/freertos.h" "FreeRTOS thread header"
check_file "$ddsrt_dir/include/dds/ddsrt/time/freertos.h" "FreeRTOS time header"
check_file "$ddsrt_dir/src/sync/freertos/sync.c" "FreeRTOS sync implementation"
check_file "$ddsrt_dir/src/threads/freertos/threads.c" "FreeRTOS thread implementation"
check_file "$ddsrt_dir/src/time/freertos/time.c" "FreeRTOS time implementation"
check_file "$ddsrt_dir/src/heap/freertos/heap.c" "FreeRTOS heap implementation"
check_file "$ddsrt_dir/src/process/freertos/process.c" "FreeRTOS process implementation"
check_file "$ddsrt_dir/src/ifaddrs/lwip/ifaddrs.c" "lwIP ifaddrs implementation"
check_file "$ddsrt_dir/src/sockets/posix/socket.c" "POSIX/lwIP socket wrapper"
check_dir "$repo_root/third-party/freertos/lwip/src/include/lwip" "lwIP headers"
check_dir "$repo_root/third-party/threadx/netxduo" "NetX Duo tree"

if find "$ddsrt_dir" -path '*threadx*' -print -quit | grep -q .; then
    echo "  [OK]      ThreadX ddsrt files exist in this nano-ros tree"
else
    echo "  [TODO]    ThreadX ddsrt port is not present upstream"
fi

echo
echo "Phase 175.B split:"
echo "  FreeRTOS: upstream ddsrt + lwIP surface exists; nano-ros carries"
echo "            the MPS2 cross-build and fixture wiring."
echo "  ThreadX:  upstream Cyclone has no ddsrt surface; nano-ros carries"
echo "            an experimental NetX Duo-backed port plus cross-build probe."

exit "$missing"
