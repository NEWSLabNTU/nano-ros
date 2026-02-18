# CMake toolchain file for ARM Cortex-M3 (thumbv7m-none-eabi)
#
# This toolchain file is used for cross-compiling zenoh-pico and other
# C libraries for bare-metal ARM Cortex-M3 targets like QEMU mps2-an385.
#
# Usage:
#   cmake -DCMAKE_TOOLCHAIN_FILE=scripts/qemu/arm-none-eabi-cortex-m3.cmake ..

set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_SYSTEM_PROCESSOR arm)

# Toolchain prefix
set(TOOLCHAIN_PREFIX arm-none-eabi-)

# Find the toolchain
find_program(CMAKE_C_COMPILER ${TOOLCHAIN_PREFIX}gcc)
find_program(CMAKE_CXX_COMPILER ${TOOLCHAIN_PREFIX}g++)
find_program(CMAKE_ASM_COMPILER ${TOOLCHAIN_PREFIX}gcc)
find_program(CMAKE_AR ${TOOLCHAIN_PREFIX}ar)
find_program(CMAKE_OBJCOPY ${TOOLCHAIN_PREFIX}objcopy)
find_program(CMAKE_OBJDUMP ${TOOLCHAIN_PREFIX}objdump)
find_program(CMAKE_SIZE ${TOOLCHAIN_PREFIX}size)

# Cortex-M3 specific flags
set(CPU_FLAGS "-mcpu=cortex-m3 -mthumb")

# Compiler flags
set(CMAKE_C_FLAGS_INIT "${CPU_FLAGS} -ffunction-sections -fdata-sections -fno-common -fno-exceptions")
set(CMAKE_CXX_FLAGS_INIT "${CPU_FLAGS} -ffunction-sections -fdata-sections -fno-common -fno-exceptions -fno-rtti")
set(CMAKE_ASM_FLAGS_INIT "${CPU_FLAGS}")

# Linker flags
set(CMAKE_EXE_LINKER_FLAGS_INIT "-Wl,--gc-sections -specs=nosys.specs -specs=nano.specs")

# Don't try to run test programs during configuration
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)

# Search paths
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)
