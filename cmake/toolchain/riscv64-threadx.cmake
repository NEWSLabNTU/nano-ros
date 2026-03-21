# cmake/toolchain/riscv64-threadx.cmake
#
# CMake toolchain file for ThreadX on RISC-V 64-bit (QEMU virt).
#
# Selects the riscv64-unknown-elf cross-compiler and sets the Rust target
# triple so that Corrosion compiles nros-c / nros-cpp for riscv64gc.
#
# Usage:
#   cmake -S . -B build \
#         -DCMAKE_TOOLCHAIN_FILE=cmake/toolchain/riscv64-threadx.cmake \
#         -DNANO_ROS_RMW=zenoh \
#         -DNANO_ROS_PLATFORM=threadx_riscv64 \
#         -DNANO_ROS_BUILD_CODEGEN=OFF
#   cmake --build build
#   cmake --install build --prefix /path/to/prefix

set(CMAKE_SYSTEM_NAME       Generic)
set(CMAKE_SYSTEM_PROCESSOR  riscv64)

set(CMAKE_C_COMPILER    riscv64-unknown-elf-gcc)
set(CMAKE_CXX_COMPILER  riscv64-unknown-elf-g++)
set(CMAKE_ASM_COMPILER  riscv64-unknown-elf-gcc)
set(CMAKE_AR            riscv64-unknown-elf-ar  CACHE FILEPATH "Archiver")
set(CMAKE_RANLIB        riscv64-unknown-elf-ranlib CACHE FILEPATH "Ranlib")

set(CMAKE_C_FLAGS_INIT   "-march=rv64gc -mabi=lp64d -mcmodel=medany -ffunction-sections -fdata-sections")
set(CMAKE_CXX_FLAGS_INIT "-march=rv64gc -mabi=lp64d -mcmodel=medany -ffunction-sections -fdata-sections -fno-exceptions -fno-rtti -std=c++14 -ffreestanding")
set(CMAKE_ASM_FLAGS_INIT "-march=rv64gc -mabi=lp64d -mcmodel=medany")

# Rust target triple
set(Rust_CARGO_TARGET "riscv64gc-unknown-none-elf" CACHE STRING "Rust target triple" FORCE)

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE NEVER)

set(CMAKE_C_COMPILER_WORKS   TRUE CACHE BOOL "Compiler works" FORCE)
set(CMAKE_CXX_COMPILER_WORKS TRUE CACHE BOOL "Compiler works" FORCE)

# Use rust-lld as the linker instead of GNU ld.
# picolibc's libc.a has TLS errno which GNU ld refuses to link with
# ThreadX's non-TLS errno. LLD handles this correctly (like Rust does).
# GCC 10.x doesn't support -fuse-ld=lld for cross targets, so we override
# the entire link rule via CMAKE_C_LINK_EXECUTABLE.
execute_process(
    COMMAND rustc --print sysroot
    OUTPUT_VARIABLE _RUST_SYSROOT OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
find_program(_RUST_LLD rust-lld
    PATHS "${_RUST_SYSROOT}/lib/rustlib/x86_64-unknown-linux-gnu/bin"
    NO_DEFAULT_PATH)
if(_RUST_LLD)
    set(CMAKE_LINKER "${_RUST_LLD}" CACHE FILEPATH "Linker" FORCE)
    # GCC 10.x's collect2 ignores -B for linker selection on cross targets.
    # Override the entire link rule to call rust-lld directly.
    # <CMAKE_LINKER> is rust-lld; <FLAGS> <LINK_FLAGS> come from targets.
    # Don't include <FLAGS> (C compiler flags like -march, -isystem) in the link rule.
    set(CMAKE_C_LINK_EXECUTABLE
        "${_RUST_LLD} -flavor gnu <CMAKE_C_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>"
        CACHE STRING "" FORCE)
    set(CMAKE_CXX_LINK_EXECUTABLE
        "${_RUST_LLD} -flavor gnu <CMAKE_CXX_LINK_FLAGS> <LINK_FLAGS> <OBJECTS> -o <TARGET> <LINK_LIBRARIES>"
        CACHE STRING "" FORCE)
endif()
