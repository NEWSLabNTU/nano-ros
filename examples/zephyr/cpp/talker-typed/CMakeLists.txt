# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Talker Example (C++) — TYPED component (RFC-0043 / phase-240.8).
#
# Same Zephyr boilerplate as `examples/zephyr/cpp/talker`, but instead of a
# hand-written imperative `main.cpp`, the node is a stateful **component**
# (`Talker::configure(node)`) and the Zephyr typed carrier generates the entry:
# `nano_ros_node_register(TYPED … DEPLOY zephyr)` `configure_file`s
# `zephyr_entry_main_typed.cpp.in` (plain `int main(void)` →
# `ZephyrBoard::run_components`) and composes it into the Zephyr `app` target.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
# L.4: the component CLASS must start with `${PROJECT_NAME}::`, so the project
# name IS the pkg name (`nros_zephyr_talker_typed_cpp::Talker`).
project(nros_zephyr_talker_typed_cpp LANGUAGES CXX)

include("${NROS_REPO_DIR}/cmake/compat/NrosRclcppCompat.cmake")
find_package(std_msgs REQUIRED)

# Cyclone DDS C descriptor for std_msgs/Int32 (same as the imperative example).
if(CONFIG_NROS_RMW_CYCLONEDDS)
    list(APPEND CMAKE_MODULE_PATH "${NROS_CYCLONE_CMAKE_DIR}")
    set(IDLC_EXECUTABLE "${NROS_CYCLONE_IDLC}" CACHE FILEPATH "" FORCE)
    set(ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR} "${NROS_CYCLONE_SCRIPTS_DIR}")
    include(NrosRmwCycloneddsTypeSupport)
    nros_rmw_cyclonedds_generate_from_msg(_nros_cyc_types
        PKG_NAME   std_msgs
        PKG_DIR    "$ENV{NROS_STD_MSGS_DIR}"
        INTERFACES msg/String.msg)
    target_sources(app PRIVATE ${_nros_cyc_types})
endif()

# Phase 240.8 — the Zephyr typed carrier branch keys on NANO_ROS_PLATFORM=zephyr
# + DEPLOY zephyr + TARGET app (the `find_package(Zephyr)` app) + TYPED, and
# `target_sources(app …)` + `target_link_libraries(app …)` the generated entry +
# component lib into the global `app`. No second executable.
set(NANO_ROS_PLATFORM zephyr)
include("${NROS_REPO_DIR}/cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME      talker
    CLASS     nros_zephyr_talker_typed_cpp::Talker
    LANGUAGE  CPP
    HEADER    Talker.hpp
    TYPED
    SOURCES   src/Talker.cpp
    DEPLOY    zephyr)
