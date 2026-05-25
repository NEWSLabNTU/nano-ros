#!/usr/bin/env bash
# Configure/build probe for Cyclone DDS on nano-ros ThreadX RISC-V64.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cyclone_dir="${CYCLONEDDS_DIR:-$repo_root/third-party/dds/cyclonedds}"
build_dir="${CYCLONEDDS_THREADX_PROBE_BUILD_DIR:-$repo_root/build/cyclonedds-threadx-rv64-probe}"
install_dir="${CYCLONEDDS_THREADX_PROBE_INSTALL_DIR:-$repo_root/build/cyclonedds-threadx-rv64-install}"
toolchain="${CYCLONEDDS_THREADX_TOOLCHAIN:-$repo_root/cmake/toolchain/riscv64-threadx.cmake}"
threadx_dir="${THREADX_DIR:-$repo_root/third-party/threadx/kernel}"
netx_dir="${NETX_DIR:-$repo_root/third-party/threadx/netxduo}"
config_dir="${CYCLONEDDS_THREADX_CONFIG_DIR:-$repo_root/packages/boards/nros-board-threadx-qemu-riscv64/config}"
threadx_port_dir="${THREADX_PORT_DIR:-$threadx_dir/ports/risc-v64/gnu/inc}"

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

echo "Cyclone DDS ThreadX+NetX Duo cross-build probe"
echo "  Source: $cyclone_dir"
echo "  Build:  $build_dir"
echo "  Prefix: $install_dir"

check_file "$toolchain" "ThreadX RISC-V64 CMake toolchain"
check_file "$cyclone_dir/CMakeLists.txt" "Cyclone DDS source tree"
check_dir "$threadx_dir/common/inc" "ThreadX kernel headers"
check_dir "$threadx_port_dir" "ThreadX RISC-V64 port headers"
check_dir "$netx_dir/common/inc" "NetX Duo headers"
check_file "$netx_dir/addons/BSD/nxd_bsd.h" "NetX Duo BSD header"
check_file "$config_dir/tx_user.h" "ThreadX tx_user.h"
check_file "$config_dir/nx_user.h" "NetX nx_user.h"
check_file "$config_dir/tx_port.h" "nano-ros ThreadX port shim"
check_file "$config_dir/nx_port.h" "nano-ros NetX port shim"
if ! command -v riscv64-unknown-elf-gcc >/dev/null 2>&1; then
    echo "  [MISSING] riscv64-unknown-elf-gcc"
    missing=1
else
    echo "  [OK]      riscv64-unknown-elf-gcc"
fi
if [ "$missing" -ne 0 ]; then
    exit "$missing"
fi

picolibc_sysroot="$(riscv64-unknown-elf-gcc -march=rv64gc -mabi=lp64d --specs=picolibc.specs -print-sysroot 2>/dev/null || true)"
if [ -z "$picolibc_sysroot" ] || [ ! -d "$picolibc_sysroot/include" ]; then
    picolibc_sysroot="/usr/lib/picolibc/riscv64-unknown-elf"
fi
if [ ! -d "$picolibc_sysroot/include" ]; then
    echo "  [MISSING] picolibc include dir"
    exit 1
fi

c_flags=(
    "-march=rv64gc"
    "-mabi=lp64d"
    "-mcmodel=medany"
    "-ffunction-sections"
    "-fdata-sections"
    "-fno-builtin"
    "-fno-lto"
    "-isystem $picolibc_sysroot/include"
    "-I$config_dir"
    "-I$threadx_dir/common/inc"
    "-I$threadx_port_dir"
    "-I$netx_dir/common/inc"
    "-I$netx_dir/addons/BSD"
    "-DTX_INCLUDE_USER_DEFINE_FILE"
    "-DNX_INCLUDE_USER_DEFINE_FILE"
)

cmake_args=(
    -S "$cyclone_dir"
    -B "$build_dir"
    "-DCMAKE_TOOLCHAIN_FILE=$toolchain"
    -DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY
    -DCMAKE_BUILD_TYPE=Release
    -DCMAKE_INTERPROCEDURAL_OPTIMIZATION=OFF
    -DBUILD_SHARED_LIBS=OFF
    -DBUILD_IDLC=OFF
    -DBUILD_TESTING=OFF
    -DBUILD_IDLC_TESTING=OFF
    -DBUILD_EXAMPLES=OFF
    -DBUILD_DDSPERF=OFF
    -DBUILD_DOCS=OFF
    -DENABLE_LTO=OFF
    -DENABLE_SECURITY=OFF
    -DENABLE_SSL=OFF
    -DENABLE_SHM=OFF
    -DENABLE_IPV6=OFF
    -DWITH_THREADX=ON
    "-DCMAKE_C_FLAGS=${c_flags[*]}"
)

echo
# Phase 179.G — self-heal a stale CMake cache. A build dir configured
# before LTO was disabled keeps `ENABLE_LTO:BOOL=ON`, and an incremental
# reconfigure leaves the GCC slim-LTO objects in place. Those objects
# carry GIMPLE bytecode, not machine code, so rust-lld (the linker for
# the ThreadX examples) cannot resolve any `dds_*` symbol from them. Wipe
# the build dir whenever the cached LTO setting does not match the
# LTO-off config so the rebuild produces real, linkable objects.
cache="$build_dir/CMakeCache.txt"
if [ -f "$cache" ] && ! grep -q '^ENABLE_LTO:BOOL=OFF' "$cache"; then
    echo "Stale CMake cache (LTO not disabled) — wiping $build_dir for a clean reconfigure"
    rm -rf "$build_dir"
fi

echo "Configuring Cyclone DDS for ThreadX+NetX Duo..."
cmake "${cmake_args[@]}"

if [ "$mode" = "configure" ]; then
    echo
    echo "Configure-only probe passed."
    exit 0
fi

echo
echo "Building Cyclone DDS ddsc target..."
cmake --build "$build_dir" --target ddsc --parallel "${NROS_BUILD_JOBS:-4}"
cmake --install "$build_dir" --prefix "$install_dir"
