# cmake/toolchain/arm-freertos-armcr5f.cmake
#
# CMake toolchain file for FreeRTOS on ARM Cortex-R5F, HARD float (phase-575
# W1). Sibling of arm-freertos-armcr52.cmake, and the toolchain half of
# `[arch.cortex-r5f]` in packages/platform/nros-platform-freertos/nros-platform.toml:
# the two carry the same `-mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=hard`.
#
# Hard float means AAPCS-VFP: a `float` crosses a function boundary in s0../d0..,
# so every object in the image must agree. The Rust half is therefore
# `armv7r-none-eabihf`, never `armv7r-none-eabi`. A softfp C half, the NVIDIA
# Orin SPE's, is the other R5 profile (`[arch.cortex-r5]`) and has no toolchain
# file of its own; the linker refuses a mix ("uses VFP register arguments"),
# and an image built here can be checked with `readelf -A` for
# `Tag_ABI_VFP_args: VFP registers`.
#
# The first consumer is TI's J784S4 R5F under its Processor SDK RTOS, which
# builds with TI ARM Clang. The flags are spelled the same for tiarmclang, so a
# tiarmclang road (phase-575 W3) can reuse them; this file is the GCC one, for
# CI and for a board whose SDK builds with GCC.
#
# Usage (the board package names its own linker script and startup):
#   cmake -S . -B build \
#         -DCMAKE_TOOLCHAIN_FILE=cmake/toolchain/arm-freertos-armcr5f.cmake \
#         -DNROS_PLATFORM=freertos \
#         -DNROS_BOARD=<an R5F board>
#   cmake --build build

set(CMAKE_SYSTEM_NAME       Generic)
# Read by `nros_arm_r_fpu_cflags_env` (NanoRosCargoProfile.cmake), which hands
# cargo's cc-rs builds the FPU this file gives the CMake lane.
set(CMAKE_SYSTEM_PROCESSOR  cortex-r5f)

# Issue 1117 — which arm-none-eabi-gcc, and from where: the shared resolver,
# its provenance line and the GCC floor, as every arm toolchain file here.
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

set(_cr5f_flags "-mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=hard -ffunction-sections -fdata-sections -fno-common")

set(CMAKE_C_FLAGS_INIT   "${_cr5f_flags}")
set(CMAKE_CXX_FLAGS_INIT "${_cr5f_flags} -std=c++17")
set(CMAKE_ASM_FLAGS_INIT "${_cr5f_flags}")
set(CMAKE_EXE_LINKER_FLAGS_INIT "-Wl,--gc-sections -nostartfiles")

# Rust target triple — read by Corrosion and NanoRosGenerateInterfaces.cmake.
# FORCE: the triple IS this file's ABI, so a cached soft-float one from an
# earlier configure must not survive.
set(Rust_CARGO_TARGET "armv7r-none-eabihf" CACHE STRING "Rust target triple" FORCE)

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE NEVER)

# Bare metal: a test link would need the board's startup and linker script.
set(CMAKE_C_COMPILER_WORKS   TRUE CACHE BOOL "Compiler works" FORCE)
set(CMAKE_CXX_COMPILER_WORKS TRUE CACHE BOOL "Compiler works" FORCE)
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
