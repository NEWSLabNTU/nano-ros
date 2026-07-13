cmake_minimum_required(VERSION 3.22)

# phase-263 C2c — a cross-compiled embedded board needs its CMAKE_TOOLCHAIN_FILE set
# BEFORE project() (the first compiler probe). The workspace builds ONE board per
# configure; map the board key to its toolchain here. Host-sim boards (threadx-linux) and
# posix need none. Mirrors examples/workspaces/c/CMakeLists.txt.
if(NOT CMAKE_TOOLCHAIN_FILE AND DEFINED NANO_ROS_BOARD)
    get_filename_component(_nros_root "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
    if(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
        set(CMAKE_TOOLCHAIN_FILE
            "${_nros_root}/cmake/toolchain/arm-freertos-armcm3.cmake")
    endif()
endif()

project(multi_node_workspace_cpp LANGUAGES C CXX)

# Phase 219.I — workspace-root single call. Each subdir uses
# `nano_ros_workspace_pkg_guard()` to inherit the helpers without re-importing nano-ros.
#
# Standalone solo build of any subdir:
#     cmake -S src/talker_pkg -B build -DNANO_ROS_ROOT=/path/to/nano-ros
# Workspace build (from this dir):
#     cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros && cmake --build build
find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-263 C2c — the workspace builds as ONE platform per configure (like the C
# workspace + the standalone embedded examples). Default posix (native); an embedded
# fixture row passes `-DNANO_ROS_PLATFORM=<rtos> -DNANO_ROS_BOARD=<board>`. Only the
# entries matching the active platform are added.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/talker_pkg
    src/listener_pkg)

if(NANO_ROS_PLATFORM STREQUAL "posix")
    list(APPEND _ws_subdirs
        # phase-263 A1 (services, C++) — AddTwoInts server/client pkgs + their two single-node
        # entries (cross-process, issue 0096). Node pkgs before entries. Native-only.
        src/cpp_add_server_pkg
        src/cpp_add_client_pkg
        # phase-263 A4 (actions, C++) — the Fibonacci server/client Node pkgs + their two
        # single-node entries (cross-process, issue 0096). Node pkgs before entries.
        src/cpp_fib_server_pkg
        src/cpp_fib_client_pkg
        src/native_entry
        src/native_entry_robot1
        src/native_entry_robot2
        src/native_service_server_entry
        src/native_service_client_entry
        src/native_action_server_entry
        src/native_action_client_entry)
elseif(NANO_ROS_BOARD STREQUAL "threadx-linux")
    list(APPEND _ws_subdirs src/threadx_entry)
elseif(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
    list(APPEND _ws_subdirs src/freertos_entry)
endif()

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SUBDIRS  ${_ws_subdirs})
