cmake_minimum_required(VERSION 3.22)

project(ws_params_c LANGUAGES C CXX)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/NanoRosWorkspace.cmake")

# phase-269 W1 (launch parameters, C — issue #116) — the C projection of
# ws-params-rust. Native (posix) only: a C talker seeds launch parameters via
# the bringup `system.toml` and reads them live at runtime through the component
# `configure` seam. Single-node native entry. The node pkg is listed BEFORE the
# entry so the entry codegen sees its `nano_ros_node_register` metadata.
if(NOT NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM posix)
endif()

set(_ws_subdirs
    src/c_param_talker_pkg
    src/native_entry)

nano_ros_workspace(
    BACKEND  zenoh
    PLATFORM "${NANO_ROS_PLATFORM}"
    NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.."
    SYSTEM   demo_bringup
    SUBDIRS  ${_ws_subdirs})
