cmake_minimum_required(VERSION 3.22)

# phase-263 C2b — a cross-compiled embedded board needs its CMAKE_TOOLCHAIN_FILE set
# BEFORE project() (the first compiler probe). The workspace builds ONE board per
# configure, so map the board key to its toolchain here; host-sim boards (threadx-linux)
# and posix need none. Keeps the embedded fixture rows simple (they pass only
# -DNANO_ROS_PLATFORM/-DNANO_ROS_BOARD; the workspace wires the rest), unlike the
# standalone qemu examples which rely on the recipe passing -DCMAKE_TOOLCHAIN_FILE.
if(NOT CMAKE_TOOLCHAIN_FILE AND DEFINED NANO_ROS_BOARD)
    get_filename_component(_nros_root "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
    if(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
        set(CMAKE_TOOLCHAIN_FILE
            "${_nros_root}/cmake/toolchain/arm-freertos-armcm3.cmake")
    endif()
endif()

project(pure_c_workspace LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# phase-263 C2 — the workspace builds as ONE platform per configure (like the standalone
# embedded examples + the Rust per-fixture-row model). Default posix (native); an
# embedded fixture row passes `-DNANO_ROS_PLATFORM=<rtos> -DNANO_ROS_BOARD=<board>`. Only
# the entries matching the active platform are added, so a posix configure never processes
# an embedded entry's CMakeLists and vice versa. The node pkgs build for whichever platform
# is active.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/c_talker_pkg
    src/c_listener_pkg)

if(NANO_ROS_PLATFORM STREQUAL "posix")
    list(APPEND _ws_subdirs
        src/native_entry
        src/native_entry_robot1
        src/native_entry_robot2)
elseif(NANO_ROS_BOARD STREQUAL "threadx-linux")
    list(APPEND _ws_subdirs src/native_threadx_entry)
elseif(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
    list(APPEND _ws_subdirs src/freertos_entry)
endif()

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
