# CMake toolchain file for NuttX ARM cross-compilation (QEMU virt, Cortex-A7)
#
# Usage: cmake -DCMAKE_TOOLCHAIN_FILE=.../armv7a-nuttx-toolchain.cmake

set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_SYSTEM_PROCESSOR arm)

set(CMAKE_C_COMPILER arm-none-eabi-gcc)
set(CMAKE_CXX_COMPILER arm-none-eabi-g++)
set(CMAKE_ASM_COMPILER arm-none-eabi-gcc)

# Cortex-A7 flags (matching NuttX QEMU virt board)
set(CMAKE_C_FLAGS_INIT "-mcpu=cortex-a7 -mfloat-abi=soft -ffunction-sections -fdata-sections")
set(CMAKE_CXX_FLAGS_INIT "-mcpu=cortex-a7 -mfloat-abi=soft -ffunction-sections -fdata-sections -std=c++14")
set(CMAKE_EXE_LINKER_FLAGS_INIT "-mcpu=cortex-a7 -mfloat-abi=soft -nostartfiles -Wl,--gc-sections")

# Rust target triple — used by NanoRosGenerateInterfaces for per-message FFI
set(Rust_CARGO_TARGET "armv7a-nuttx-eabi")

set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE NEVER)

set(CMAKE_C_COMPILER_WORKS TRUE)
set(CMAKE_CXX_COMPILER_WORKS TRUE)
