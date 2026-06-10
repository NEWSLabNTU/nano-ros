# cmake/toolchain/riscv32imac-nuttx-elf.cmake
#
# 194.3c — CMake toolchain file for NuttX on riscv (QEMU rv-virt, rv32imac).
# Selects the riscv-none-elf cross-compiler and the Rust target triple so
# Corrosion compiles nros-c / nros-cpp for riscv32imac-unknown-nuttx-elf.
# Mirror of armv7a-nuttx-eabi.cmake.
#
# Usage:
#   cmake -S . -B build \
#         -DCMAKE_TOOLCHAIN_FILE=cmake/toolchain/riscv32imac-nuttx-elf.cmake \
#         -DNANO_ROS_RMW=zenoh -DNANO_ROS_BUILD_CODEGEN=OFF
#   cmake --build build

set(CMAKE_SYSTEM_NAME       Generic)
set(CMAKE_SYSTEM_PROCESSOR  riscv)

set(CMAKE_C_COMPILER    riscv-none-elf-gcc)
set(CMAKE_CXX_COMPILER  riscv-none-elf-g++)
set(CMAKE_ASM_COMPILER  riscv-none-elf-gcc)
set(CMAKE_AR            riscv-none-elf-ar  CACHE FILEPATH "Archiver")
set(CMAKE_RANLIB        riscv-none-elf-ranlib CACHE FILEPATH "Ranlib")

# rv32imac / ilp32 SOFT-float — must match the NuttX kernel ABI (the board
# defconfig disables the FPU so the kernel is ilp32 soft, matching the
# soft-float riscv32imac-unknown-nuttx-elf Rust target; rustc ships no
# riscv32imafdc-nuttx target to pair with a hard-float kernel).
set(CMAKE_C_FLAGS_INIT   "-march=rv32imac -mabi=ilp32 -ffunction-sections -fdata-sections")
set(CMAKE_CXX_FLAGS_INIT "-march=rv32imac -mabi=ilp32 -ffunction-sections -fdata-sections -fno-exceptions -fno-rtti -std=c++14")
set(CMAKE_ASM_FLAGS_INIT "-march=rv32imac -mabi=ilp32")

# Rust target triple — Tier 3, requires nightly + build-std. Keep the nightly
# pin in lockstep with the arm NuttX toolchain file / the example's
# rust-toolchain.toml (the build-std libc match is nightly-version-sensitive).
set(Rust_CARGO_TARGET "riscv32imac-unknown-nuttx-elf" CACHE STRING "Rust target triple" FORCE)
set(Rust_TOOLCHAIN "nightly-2026-04-11" CACHE STRING "Rust toolchain" FORCE)

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE NEVER)

set(CMAKE_C_COMPILER_WORKS   TRUE CACHE BOOL "Compiler works" FORCE)
set(CMAKE_CXX_COMPILER_WORKS TRUE CACHE BOOL "Compiler works" FORCE)
