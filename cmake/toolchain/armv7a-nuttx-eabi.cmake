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

set(CMAKE_C_COMPILER    arm-none-eabi-gcc)
set(CMAKE_CXX_COMPILER  arm-none-eabi-g++)
set(CMAKE_ASM_COMPILER  arm-none-eabi-gcc)
set(CMAKE_AR            arm-none-eabi-ar  CACHE FILEPATH "Archiver")
set(CMAKE_RANLIB        arm-none-eabi-ranlib CACHE FILEPATH "Ranlib")

# Cortex-A7 flags matching NuttX QEMU virt board configuration.
set(CMAKE_C_FLAGS_INIT   "-mcpu=cortex-a7 -mfloat-abi=soft -ffunction-sections -fdata-sections")
set(CMAKE_CXX_FLAGS_INIT "-mcpu=cortex-a7 -mfloat-abi=soft -ffunction-sections -fdata-sections -fno-exceptions -fno-rtti -std=c++14")
set(CMAKE_ASM_FLAGS_INIT "-mcpu=cortex-a7 -mfloat-abi=soft")

# Rust target triple — read by Corrosion and NanoRosGenerateInterfaces.cmake
# for cross-compilation.  Must be set before FetchContent_MakeAvailable(Corrosion).
set(Rust_CARGO_TARGET "armv7a-nuttx-eabi" CACHE STRING "Rust target triple" FORCE)

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
