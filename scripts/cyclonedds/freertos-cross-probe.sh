#!/usr/bin/env bash
# Configure/build probe for Cyclone DDS on the nano-ros FreeRTOS MPS2 target.
#
# This intentionally stops short of wiring any example fixture cells. It proves
# how far the pinned Cyclone tree gets with WITH_FREERTOS + WITH_LWIP and records
# the first real port/config blocker.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cyclone_dir="${CYCLONEDDS_DIR:-$repo_root/third-party/dds/cyclonedds}"
build_dir="${CYCLONEDDS_FREERTOS_PROBE_BUILD_DIR:-$repo_root/build/cyclonedds-freertos-probe}"
install_dir="${CYCLONEDDS_FREERTOS_PROBE_INSTALL_DIR:-$repo_root/build/cyclonedds-freertos-install}"
toolchain="${CYCLONEDDS_FREERTOS_TOOLCHAIN:-$repo_root/cmake/toolchain/arm-freertos-armcm3.cmake}"
freertos_dir="${FREERTOS_DIR:-$repo_root/third-party/freertos/kernel}"
lwip_dir="${LWIP_DIR:-$repo_root/third-party/freertos/lwip}"
config_dir="${FREERTOS_CONFIG_DIR:-$repo_root/packages/boards/nros-board-mps2-an385-freertos/config}"

mode="build"
if [ "${1:-}" = "--configure-only" ]; then
    mode="configure"
elif [ "${1:-}" != "" ]; then
    echo "usage: $0 [--configure-only]" >&2
    exit 2
fi

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

echo "Cyclone DDS FreeRTOS+lwIP cross-build probe"
echo "  Source: $cyclone_dir"
echo "  Build:  $build_dir"
echo "  Prefix: $install_dir"

check_file "$toolchain" "MPS2 FreeRTOS CMake toolchain"
check_file "$cyclone_dir/CMakeLists.txt" "Cyclone DDS source tree"
check_dir "$freertos_dir/include" "FreeRTOS kernel headers"
check_dir "$freertos_dir/portable/GCC/ARM_CM3" "FreeRTOS ARM_CM3 port"
check_dir "$lwip_dir/src/include/lwip" "lwIP headers"
check_dir "$lwip_dir/contrib/ports/freertos/include" "lwIP FreeRTOS port headers"
check_file "$config_dir/FreeRTOSConfig.h" "MPS2 FreeRTOSConfig.h"
check_file "$config_dir/lwipopts.h" "MPS2 lwipopts.h"
check_file "$config_dir/arch/cc.h" "MPS2 lwIP arch/cc.h"
if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
    echo "  [MISSING] arm-none-eabi-gcc"
    missing=1
else
    echo "  [OK]      arm-none-eabi-gcc"
fi
if [ "$missing" -ne 0 ]; then
    exit "$missing"
fi

c_flags=(
    "-mcpu=cortex-m3"
    "-mthumb"
    "-ffunction-sections"
    "-fdata-sections"
    "-D__int64_t_defined=1"
    "-DconfigUSE_TRACE_FACILITY=1"
    "-I$config_dir"
    "-I$config_dir/arch"
    "-I$freertos_dir/include"
    "-I$freertos_dir/portable/GCC/ARM_CM3"
    "-I$lwip_dir/src/include"
    "-I$lwip_dir/contrib/ports/freertos/include"
)

cmake_args=(
    -S "$cyclone_dir"
    -B "$build_dir"
    "-DCMAKE_TOOLCHAIN_FILE=$toolchain"
    -DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY
    -DCMAKE_BUILD_TYPE=Release
    -DBUILD_SHARED_LIBS=OFF
    -DBUILD_IDLC=OFF
    -DBUILD_TESTING=OFF
    -DBUILD_IDLC_TESTING=OFF
    -DBUILD_EXAMPLES=OFF
    -DBUILD_DDSPERF=OFF
    -DBUILD_DOCS=OFF
    -DENABLE_SECURITY=OFF
    -DENABLE_SSL=OFF
    -DENABLE_SHM=OFF
    -DENABLE_IPV6=OFF
    -DDDSRT_HAVE_RUSAGE=OFF
    -DWITH_FREERTOS=ON
    -DWITH_LWIP=ON
    "-DCMAKE_C_FLAGS=${c_flags[*]}"
)

echo
echo "Configuring Cyclone DDS for FreeRTOS+lwIP..."
cmake "${cmake_args[@]}"

if [ "$mode" = "configure" ]; then
    echo
    echo "Configure-only probe passed."
    exit 0
fi

echo
echo "Building Cyclone DDS ddsc target..."
echo "Building ddsc should now get past lwIP ifaddrs/netif_list."
cmake --build "$build_dir" --target ddsc --parallel "${NROS_BUILD_JOBS:-4}"
cmake --install "$build_dir" --prefix "$install_dir"
