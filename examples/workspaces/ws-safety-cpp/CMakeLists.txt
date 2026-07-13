cmake_minimum_required(VERSION 3.22)

project(ws_safety_cpp LANGUAGES C CXX)

find_package(nano_ros REQUIRED COMPONENTS workspace)

# phase-269 W4 (subscription E2E-integrity, C++ — issue #118) — the C++
# projection of ws-safety-rust. Native (posix) only: a C++ talker + a C++
# listener that reads back subscription end-to-end integrity, as two single-node
# entries (cross-process, issue 0096). The node pkgs are listed BEFORE the
# entries so the entry codegen sees their `nano_ros_node_register` metadata.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/cpp_safety_talker_pkg
    src/cpp_safety_listener_pkg
    src/native_safety_talker_entry
    src/native_safety_listener_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
