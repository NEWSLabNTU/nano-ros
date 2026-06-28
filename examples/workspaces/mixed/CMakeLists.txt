cmake_minimum_required(VERSION 3.22)

# phase-263 C2c — a cross-compiled embedded board needs its CMAKE_TOOLCHAIN_FILE set
# BEFORE project() (the first compiler probe). The workspace builds ONE board per
# configure; map the board key to its toolchain. Host-sim (threadx-linux) + posix need
# none. Mirrors examples/workspaces/{c,cpp}/CMakeLists.txt.
if(NOT CMAKE_TOOLCHAIN_FILE AND DEFINED NANO_ROS_BOARD)
    get_filename_component(_nros_root "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
    if(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
        set(CMAKE_TOOLCHAIN_FILE
            "${_nros_root}/cmake/toolchain/arm-freertos-armcm3.cmake")
    endif()
endif()

project(c_and_cpp_mixed_workspace LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# phase-263 C2c — ONE platform per configure (the Rust+C+C++ mix is combined by the
# `nros_ws_runtime` umbrella). Default posix; an embedded fixture row passes
# `-DNANO_ROS_PLATFORM=<rtos> -DNANO_ROS_BOARD=<board>`.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/c_talker_pkg
    src/cpp_listener_pkg
    src/rust_heartbeat_pkg)

if(NANO_ROS_PLATFORM STREQUAL "posix")
    list(APPEND _ws_subdirs
        # phase-263 A1 (services, MIXED) — cross-language AddTwoInts: C server + C client pkgs
        # + their two single-node entries (cross-process, issue 0096). Pkgs before entries.
        src/c_add_server_pkg
        src/c_add_client_pkg
        src/native_entry
        src/native_entry_robot1
        src/native_entry_robot2
        src/native_service_server_entry
        src/native_service_client_entry)
elseif(NANO_ROS_BOARD STREQUAL "threadx-linux")
    list(APPEND _ws_subdirs src/threadx_entry)
elseif(NANO_ROS_BOARD STREQUAL "mps2-an385-freertos")
    list(APPEND _ws_subdirs src/freertos_entry)
endif()

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
