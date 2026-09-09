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

# rv32imac / ilp32 SOFT-float — must match the NuttX kernel ABI (the board
# defconfig disables the FPU so the kernel is ilp32 soft, matching the
# soft-float riscv32imac-unknown-nuttx-elf Rust target; rustc ships no
# riscv32imafdc-nuttx target to pair with a hard-float kernel).
set(CMAKE_C_FLAGS_INIT   "-march=rv32imac -mabi=ilp32 -ffunction-sections -fdata-sections")
# phase-432 W3.1 — `-DCONFIG_LIBCXXTOOLCHAIN=1`: say out loud that this TU is
# compiled against the TOOLCHAIN's libstdc++.
#
# NuttX's `include/cxx` and `include` are deliberately placed AHEAD of the
# toolchain's headers (issue 0036, so NuttX's `div_t` wins), so libstdc++'s
# `<cctype>` resolves to NuttX's `include/cxx/cctype` -> NuttX's `ctype.h`.
# That header gates its `_U`/`_L`/`_N` block on `CONFIG_LIBCXXTOOLCHAIN`, whose
# own comment says "GNU libstdc++ is expecting ctype.h to define a few macros" —
# and it is unset, because our defconfig sets `CONFIG_HAVE_CXX=y` and leaves the
# libxx choice at `LIBCXXNONE`. libstdc++'s `bits/ctype_base.h` then read macros
# nobody defined and EVERY embedded C++ NuttX image failed to compile:
#
#     ctype_base.h:54: error: '_L' was not declared in this scope
#
# reached from `<nros/nros.hpp>` -> `log.hpp`'s `<sstream>` -> `<ios>` ->
# `bits/locale_facets.h` -> `<cctype>`, on a TU that includes no STL itself.
#
# A COMPILE flag, not `CONFIG_LIBCXXTOOLCHAIN=y` in the defconfig: that would
# flip NuttX's libxx *choice* and change what NuttX itself builds and links,
# which is a far larger blast radius than a header-macro problem earns. Nor is
# the include ORDER touched — that ordering is issue 0036's fix.
#
# Known limit, stated rather than discovered later: NuttX ships no `_ctype_`
# symbol, so a TU that actually instantiates `std::ctype<char>` would fail to
# LINK rather than to compile. Nothing here uses locales. The real fix is
# phase-438 W1 — delete the `#elif defined(__has_include)` arm in `log.hpp` and
# friends, which is what drags `<sstream>` in on a target that never asked for
# it — but that sweep must first give the HOSTED lanes an explicit
# `NROS_CPP_STD`, because a compile test there uses `RCLCPP_*_STREAM` and the
# arm is what currently supplies it (measured: removing the arm alone turns
# `just check cpp` red on `ros2_api_adoption_stage2.cpp`).
set(CMAKE_CXX_FLAGS_INIT "-march=rv32imac -mabi=ilp32 -ffunction-sections -fdata-sections -fno-exceptions -fno-rtti -std=c++14 -DCONFIG_LIBCXXTOOLCHAIN=1")
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
