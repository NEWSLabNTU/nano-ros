#!/usr/bin/env bash
# Configure/build probe for Cyclone DDS on the nano-ros FreeRTOS MPS2 target.
#
# Cross-builds the Cyclone `ddsc` static lib with WITH_FREERTOS + WITH_LWIP and
# installs it so `find_package(CycloneDDS)` resolves for the FreeRTOS examples.
# Shared boilerplate (prereq checks, mode parsing, configure/build/install) lives
# in cross-build-ddsc.sh (Phase 185.4); this file carries the FreeRTOS-specific
# toolchain, include checks, and CMake flags.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/cyclonedds/cross-build-ddsc.sh
source "$repo_root/scripts/cyclonedds/cross-build-ddsc.sh"

cyclone_dir="${CYCLONEDDS_DIR:-$repo_root/third-party/dds/cyclonedds}"
build_dir="${CYCLONEDDS_FREERTOS_PROBE_BUILD_DIR:-$repo_root/build/cyclonedds-freertos-probe}"
install_dir="${CYCLONEDDS_FREERTOS_PROBE_INSTALL_DIR:-$repo_root/build/cyclonedds-freertos-install}"
toolchain="${CYCLONEDDS_FREERTOS_TOOLCHAIN:-$repo_root/cmake/toolchain/arm-freertos-armcm3.cmake}"
freertos_dir="${FREERTOS_DIR:-$repo_root/third-party/freertos/kernel}"
lwip_dir="${LWIP_DIR:-$repo_root/third-party/freertos/lwip}"
config_dir="${FREERTOS_CONFIG_DIR:-$repo_root/packages/boards/nros-board-mps2-an385-freertos/config}"

csb_parse_mode "$@"

echo "Cyclone DDS FreeRTOS+lwIP cross-build probe"
echo "  Source: $cyclone_dir"
echo "  Build:  $build_dir"
echo "  Prefix: $install_dir"

csb_check_file "$toolchain" "MPS2 FreeRTOS CMake toolchain"
csb_check_file "$cyclone_dir/CMakeLists.txt" "Cyclone DDS source tree"
csb_check_dir "$freertos_dir/include" "FreeRTOS kernel headers"
csb_check_dir "$freertos_dir/portable/GCC/ARM_CM3" "FreeRTOS ARM_CM3 port"
csb_check_dir "$lwip_dir/src/include/lwip" "lwIP headers"
csb_check_dir "$lwip_dir/contrib/ports/freertos/include" "lwIP FreeRTOS port headers"
csb_check_file "$config_dir/FreeRTOSConfig.h" "MPS2 FreeRTOSConfig.h"
csb_check_file "$config_dir/lwipopts.h" "MPS2 lwipopts.h"
csb_check_file "$config_dir/arch/cc.h" "MPS2 lwIP arch/cc.h"
csb_require_compiler arm-none-eabi-gcc
csb_finalize_checks

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

# FreeRTOS does not disable LTO, so the stale-LTO-cache self-heal does not apply.
echo
echo "Building ddsc should now get past lwIP ifaddrs/netif_list."
csb_configure_build_install
