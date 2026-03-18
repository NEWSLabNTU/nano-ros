# freertos-platform.cmake  (DEPRECATED)
#
# This file is kept for backward compatibility only.
# Use freertos-support.cmake + find_package(NanoRos CONFIG REQUIRED) instead:
#
#   set(CMAKE_TOOLCHAIN_FILE ".../cmake/toolchain/arm-freertos-armcm3.cmake" CACHE FILEPATH "")
#   project(... LANGUAGES C CXX ASM)
#   find_package(NanoRos CONFIG REQUIRED)
#   include(".../cmake/freertos-support.cmake")
#
# See docs/roadmap/phase-75-cmake-install-convention.md for details.

message(WARNING
    "freertos-platform.cmake is deprecated. "
    "Use freertos-support.cmake with find_package(NanoRos CONFIG REQUIRED) instead. "
    "See docs/roadmap/phase-75-cmake-install-convention.md.")

find_package(NanoRos CONFIG REQUIRED)
include("${CMAKE_CURRENT_LIST_DIR}/freertos-support.cmake")
