# CMake toolchain file for ARM Cortex-M3 cross-compilation (MPS2-AN385)
#
# Usage: cmake -DCMAKE_TOOLCHAIN_FILE=.../arm-none-eabi-toolchain.cmake

set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_SYSTEM_PROCESSOR arm)

set(CMAKE_C_COMPILER arm-none-eabi-gcc)
set(CMAKE_CXX_COMPILER arm-none-eabi-g++)
set(CMAKE_ASM_COMPILER arm-none-eabi-gcc)
set(CMAKE_OBJCOPY arm-none-eabi-objcopy)
set(CMAKE_SIZE arm-none-eabi-size)

# Cortex-M3 flags
set(CMAKE_C_FLAGS_INIT "-mcpu=cortex-m3 -mthumb -ffunction-sections -fdata-sections")
set(CMAKE_CXX_FLAGS_INIT "-mcpu=cortex-m3 -mthumb -ffunction-sections -fdata-sections -fno-exceptions -fno-rtti -std=c++14 -ffreestanding")
set(CMAKE_ASM_FLAGS_INIT "-mcpu=cortex-m3 -mthumb")
set(CMAKE_EXE_LINKER_FLAGS_INIT "-mcpu=cortex-m3 -mthumb --specs=nosys.specs -Wl,--gc-sections")

# Rust target triple — used by Corrosion and NanoRosGenerateInterfaces for cross-compilation
set(Rust_CARGO_TARGET "thumbv7m-none-eabi")

# Don't search host paths for libraries/headers
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE NEVER)

# Disable compiler tests (bare-metal, no libc by default)
set(CMAKE_C_COMPILER_WORKS TRUE)
set(CMAKE_CXX_COMPILER_WORKS TRUE)
