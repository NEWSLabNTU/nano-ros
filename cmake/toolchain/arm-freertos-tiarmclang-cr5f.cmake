# cmake/toolchain/arm-freertos-tiarmclang-cr5f.cmake
#
# CMake toolchain file for FreeRTOS on ARM Cortex-R5F, hard float, built with
# TI Arm Clang (`tiarmclang`) and linked by TI's `tiarmlnk` (phase-575 W3).
# The TI sibling of arm-freertos-armcr5f.cmake: the same
# `-mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=hard`, so every object agrees on
# AAPCS-VFP, and the Rust half is the same `armv7r-none-eabihf` staticlib, which
# rustc builds. Every C translation unit of the image is built here, nano-ros's
# included: an image links ONE C library, and nano-ros's C must be compiled
# against its headers (TI's libc is not newlib: errno, stdio and assert differ).
#
# TI Arm Clang is licence-gated (`[gated.ti-arm-clang]` in nros-sdk-index.toml):
# nano-ros never fetches it. `TI_ARM_CLANG_ROOT` (-D or the environment) names
# an install, the directory holding `bin/tiarmclang`.
#
# The image's linker command file is the board's (a `.cmd`, not a GNU script);
# the board package passes it as `-Wl,<file>`.

set(CMAKE_SYSTEM_NAME       Generic)
# Read by `nros_arm_r_fpu_cflags_env` (NanoRosCargoProfile.cmake), as for GCC.
set(CMAKE_SYSTEM_PROCESSOR  cortex-r5f)

if(NOT DEFINED TI_ARM_CLANG_ROOT AND DEFINED ENV{TI_ARM_CLANG_ROOT})
    set(TI_ARM_CLANG_ROOT "$ENV{TI_ARM_CLANG_ROOT}")
endif()
if(NOT DEFINED TI_ARM_CLANG_ROOT OR NOT EXISTS "${TI_ARM_CLANG_ROOT}/bin/tiarmclang")
    message(FATAL_ERROR
        "arm-freertos-tiarmclang-cr5f: TI_ARM_CLANG_ROOT does not name a TI Arm "
        "Clang install (a directory holding bin/tiarmclang). It is licence-gated "
        "and never fetched: install it from TI, then set TI_ARM_CLANG_ROOT in the "
        "environment or pass -DTI_ARM_CLANG_ROOT=<dir>.")
endif()
set(TI_ARM_CLANG_ROOT "${TI_ARM_CLANG_ROOT}" CACHE PATH "TI Arm Clang install root")

# Which compiler, from where, against what the index states (issue 1117).
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCrossToolchain.cmake")
nros_gated_toolchain_report(
    TOOL     ti-arm-clang
    ROOT_VAR TI_ARM_CLANG_ROOT
    COMPILER "${TI_ARM_CLANG_ROOT}/bin/tiarmclang")

set(CMAKE_C_COMPILER    "${TI_ARM_CLANG_ROOT}/bin/tiarmclang")
set(CMAKE_CXX_COMPILER  "${TI_ARM_CLANG_ROOT}/bin/tiarmclang")
set(CMAKE_ASM_COMPILER  "${TI_ARM_CLANG_ROOT}/bin/tiarmclang")
set(CMAKE_AR            "${TI_ARM_CLANG_ROOT}/bin/tiarmar" CACHE FILEPATH "Archiver")

# `__STDC_WANT_LIB_EXT1__`: TI's libc declares strnlen only as C11 Annex K's
# strnlen_s, which zenoh-pico calls by the POSIX name.
set(_cr5f_flags "-mcpu=cortex-r5 -mfpu=vfpv3-d16 -mfloat-abi=hard -ffunction-sections -fdata-sections -fno-common -D__STDC_WANT_LIB_EXT1__=1")
set(CMAKE_C_FLAGS_INIT   "${_cr5f_flags}")
set(CMAKE_CXX_FLAGS_INIT "${_cr5f_flags} -std=c++17")
set(CMAKE_ASM_FLAGS_INIT "${_cr5f_flags}")
# tiarmlnk error 16004-D refuses objects whose Tag_ABI_VFP_args differ. Rust's
# compiler_builtins assembles `__udivsi3` / `__udivmodsi4` with none (0, the
# base variant). That is safe to mix: a function that takes and returns no
# floating-point value passes nothing in VFP registers, so the two variants
# agree on every call it makes. The diagnostic is discretionary, so it is
# downgraded to a warning here, for this toolchain only.
set(CMAKE_EXE_LINKER_FLAGS_INIT "-Wl,--diag_warning=16004")

set(Rust_CARGO_TARGET "armv7r-none-eabihf" CACHE STRING "Rust target triple" FORCE)

# The stack-floor check reads GCC's -fcallgraph-info output, which tiarmclang
# does not emit (issue 2693), so it cannot run on this road.
set(NROS_STACK_FLOOR OFF CACHE STRING "Stack-floor check (needs GCC's -fcallgraph-info; issue 2693)" FORCE)

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE NEVER)

# Bare metal: a test link would need the board's startup and linker command file.
set(CMAKE_C_COMPILER_WORKS   TRUE CACHE BOOL "Compiler works" FORCE)
set(CMAKE_CXX_COMPILER_WORKS TRUE CACHE BOOL "Compiler works" FORCE)
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
