# CMake toolchain file for NuttX riscv cross-compilation (QEMU rv-virt, rv32imac)
#
# Usage: cmake -DCMAKE_TOOLCHAIN_FILE=.../riscv-nuttx-toolchain.cmake
# Most of the time cargo drives the cross via build.rs and CMake is host-mode;
# this file is for callers that want CMake itself to cross to riscv.

set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_SYSTEM_PROCESSOR riscv)

set(CMAKE_C_COMPILER riscv-none-elf-gcc)
set(CMAKE_CXX_COMPILER riscv-none-elf-g++)
set(CMAKE_ASM_COMPILER riscv-none-elf-gcc)

# rv32imac / ilp32 (matching the NuttX rv-virt flat build)
set(CMAKE_C_FLAGS_INIT "-march=rv32imac -mabi=ilp32 -ffunction-sections -fdata-sections")
set(CMAKE_CXX_FLAGS_INIT "-march=rv32imac -mabi=ilp32 -ffunction-sections -fdata-sections -std=c++14")
set(CMAKE_EXE_LINKER_FLAGS_INIT "-march=rv32imac -mabi=ilp32 -nostartfiles -Wl,--gc-sections")

# Rust target triple — used by NanoRosGenerateInterfaces for per-message FFI
set(Rust_CARGO_TARGET "riscv32imac-unknown-nuttx-elf")

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE NEVER)

set(CMAKE_C_COMPILER_WORKS TRUE)
set(CMAKE_CXX_COMPILER_WORKS TRUE)
