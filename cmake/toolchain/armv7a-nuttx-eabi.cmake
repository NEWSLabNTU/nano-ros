# cmake/toolchain/armv7a-nuttx-eabi.cmake
#
# CMake toolchain file for NuttX on ARM Cortex-A7 (QEMU virt board).
#
# Selects the arm-none-eabi cross-compiler and sets the Rust target triple
# so that Corrosion compiles nros-c / nros-cpp for armv7a-nuttx-eabi.
#
# Usage:
#   cmake -S . -B build \
#         -DCMAKE_TOOLCHAIN_FILE=cmake/toolchain/armv7a-nuttx-eabi.cmake \
#         -DNANO_ROS_RMW=zenoh \
#         -DNANO_ROS_PLATFORM=nuttx_armv7a \
#         -DNANO_ROS_BUILD_CODEGEN=OFF
#   cmake --build build
#   cmake --install build --prefix /path/to/prefix

set(CMAKE_SYSTEM_NAME       Generic)
set(CMAKE_SYSTEM_PROCESSOR  arm)

# Issue 1117 — WHICH arm-none-eabi-gcc, and from WHERE. The bare names below
# used to resolve on PATH alone, so a host that had never run `nros setup
# --tool arm-none-eabi-gcc` silently built with Ubuntu's 10.3.1 against a
# 13.2.rel1 pin, and said nothing. Shared module: resolution order, the
# provenance line, and the GCC floor.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCrossToolchain.cmake")
nros_cross_toolchain_resolve(
    TOOL         arm-none-eabi-gcc
    PREFIXES     arm-none-eabi
    OVERRIDE_VAR NROS_ARM_NONE_EABI_PREFIX
    OUT_PREFIX   _NROS_ARM_PREFIX
    OUT_ORIGIN   _NROS_ARM_ORIGIN)
nros_cross_toolchain_report(
    TOOL   arm-none-eabi-gcc
    PREFIX "${_NROS_ARM_PREFIX}"
    ORIGIN "${_NROS_ARM_ORIGIN}"
    OVERRIDE_VAR NROS_ARM_NONE_EABI_PREFIX)

set(CMAKE_C_COMPILER    ${_NROS_ARM_PREFIX}-gcc)
set(CMAKE_CXX_COMPILER  ${_NROS_ARM_PREFIX}-g++)
set(CMAKE_ASM_COMPILER  ${_NROS_ARM_PREFIX}-gcc)
set(CMAKE_AR            ${_NROS_ARM_PREFIX}-ar  CACHE FILEPATH "Archiver")
set(CMAKE_RANLIB        ${_NROS_ARM_PREFIX}-ranlib CACHE FILEPATH "Ranlib")

# Cortex-A7 flags matching NuttX QEMU virt board configuration.
# Must use hard-float to match NuttX kernel (built with -mfloat-abi=hard).
set(CMAKE_C_FLAGS_INIT   "-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4 -ffunction-sections -fdata-sections")
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
set(CMAKE_CXX_FLAGS_INIT "-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4 -ffunction-sections -fdata-sections -fno-exceptions -fno-rtti -std=c++14 -DCONFIG_LIBCXXTOOLCHAIN=1")
set(CMAKE_ASM_FLAGS_INIT "-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4")

# Rust target triple — armv7a-nuttx-eabihf for hard-float ABI.
# This is a Tier 3 target requiring nightly + build-std.
set(Rust_CARGO_TARGET "armv7a-nuttx-eabihf" CACHE STRING "Rust target triple" FORCE)
# Pin the EXACT nightly, not generic "nightly": NuttX uses -Z build-std against a
# patched libc whose version must match this nightly's std libc dep (see
# examples/qemu-armv7a-nuttx/rust-toolchain.toml — the SSOT). A generic "nightly"
# (a) isn't what's installed (the pin is dated) and (b) would break the libc
# match. Keep in lockstep with examples/qemu-armv7a-nuttx/rust-toolchain.toml.
set(Rust_TOOLCHAIN "nightly-2026-04-11" CACHE STRING "Rust toolchain" FORCE)

# Don't search host paths for libraries / headers when cross-compiling.
# PROGRAM is NEVER so CMake can still find host tools (cmake, ninja, etc.).
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE NEVER)

# Skip compiler capability tests — the cross-compiler produces bare-metal
# ELFs that cannot be executed on the host.
set(CMAKE_C_COMPILER_WORKS   TRUE CACHE BOOL "Compiler works" FORCE)
set(CMAKE_CXX_COMPILER_WORKS TRUE CACHE BOOL "Compiler works" FORCE)
