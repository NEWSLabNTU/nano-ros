# cmake/toolchain/riscv32imac-nuttx-elf.cmake
#
# 194.3c — CMake toolchain file for NuttX on riscv (QEMU rv-virt, rv32imac).
# Selects the riscv-none-elf cross-compiler and the Rust target triple so
# Corrosion compiles nros-c / nros-cpp for riscv32imac-unknown-nuttx-elf.
# Mirror of armv7a-nuttx-eabihf.cmake.
#
# Usage:
#   cmake -S . -B build \
#         -DCMAKE_TOOLCHAIN_FILE=cmake/toolchain/riscv32imac-nuttx-elf.cmake \
#         -DNROS_RMW=zenoh -DNROS_BUILD_CODEGEN=OFF
#   cmake --build build

set(CMAKE_SYSTEM_NAME       Generic)
set(CMAKE_SYSTEM_PROCESSOR  riscv)

# Issue 1117 — WHICH riscv-none-elf-gcc, and from WHERE. See
# NanoRosCrossToolchain.cmake: SDK store (newest first) beats PATH, the choice
# is printed, and a below-floor GCC is refused at configure rather than 400
# lines into a generated TU.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCrossToolchain.cmake")
nros_cross_toolchain_resolve(
    TOOL         riscv-none-elf-gcc
    PREFIXES     riscv-none-elf
    OVERRIDE_VAR NROS_RISCV_NONE_ELF_PREFIX
    OUT_PREFIX   _NROS_RV32_PREFIX
    OUT_ORIGIN   _NROS_RV32_ORIGIN)
nros_cross_toolchain_report(
    TOOL   riscv-none-elf-gcc
    PREFIX "${_NROS_RV32_PREFIX}"
    ORIGIN "${_NROS_RV32_ORIGIN}"
    OVERRIDE_VAR NROS_RISCV_NONE_ELF_PREFIX)

set(CMAKE_C_COMPILER    ${_NROS_RV32_PREFIX}-gcc)
set(CMAKE_CXX_COMPILER  ${_NROS_RV32_PREFIX}-g++)
set(CMAKE_ASM_COMPILER  ${_NROS_RV32_PREFIX}-gcc)
set(CMAKE_AR            ${_NROS_RV32_PREFIX}-ar  CACHE FILEPATH "Archiver")
set(CMAKE_RANLIB        ${_NROS_RV32_PREFIX}-ranlib CACHE FILEPATH "Ranlib")

# phase-550 W3b — no CPU flags here. NuttX states them (its export's
# `scripts/Make.defs` ARCHCPUFLAGS), and every object a NuttX image links is
# compiled inside the cargo build, which takes them from the export's
# `nros-cargo.toml` (`nros_nuttx_build_example`). CMake compiles nothing that
# reaches the image (measured: no object outside cargo-target/ in a NuttX
# build dir), so restating `-mcpu`/`-march` here would only be a second copy,
# one a reconfigured kernel would leave behind. Do not add one back: there is
# nothing for it to fix here, and `just check nuttx-cpu-flag-copies` refuses it.
# (The board defconfig disables the FPU, so the kernel is ilp32 soft-float,
# matching the soft-float riscv32imac-unknown-nuttx-elf Rust target.)
set(CMAKE_C_FLAGS_INIT   "-ffunction-sections -fdata-sections")
set(CMAKE_CXX_FLAGS_INIT "-ffunction-sections -fdata-sections -fno-exceptions -fno-rtti -std=c++14")

# Rust target triple — Tier 3, requires nightly + build-std. Keep the nightly
# pin in lockstep with the arm NuttX toolchain file / the example's
# rust-toolchain.toml (the build-std libc match is nightly-version-sensitive).
set(Rust_CARGO_TARGET "riscv32imac-unknown-nuttx-elf" CACHE STRING "Rust target triple" FORCE)
# Issue 2312 — read from its one source, never a literal.
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosRustToolchain.cmake")
nros_pinned_nightly(_nros_nightly)
set(Rust_TOOLCHAIN "${_nros_nightly}" CACHE STRING "Rust toolchain" FORCE)

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE NEVER)

set(CMAKE_C_COMPILER_WORKS   TRUE CACHE BOOL "Compiler works" FORCE)
set(CMAKE_CXX_COMPILER_WORKS TRUE CACHE BOOL "Compiler works" FORCE)
