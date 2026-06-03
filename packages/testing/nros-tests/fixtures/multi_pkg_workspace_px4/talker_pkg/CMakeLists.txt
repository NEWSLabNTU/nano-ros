# Phase 212.M.10 fixture — talker component pkg (PX4 host-compile shape).
#
# Per §212.L.9 the C++ pkg metadata surface is the cmake fn pair
# `nano_ros_node_register` + `nano_ros_deploy` — sidecar
# `component_nros.toml` is forbidden (§Non-Goals + §212.M.10).
#
# PX4 itself is uORB-only (Phase 115.K.4 collapsed uORB to C++) and
# the canonical PX4 surface lives at
# `examples/px4/cpp/uorb/nros-register-check/`, which wires components
# into the firmware via PX4's own `px4_add_module()`. This fixture is
# the *host-side* compile-check shape — it lets the planner walk the
# pkg dir without dragging in the PX4 build system.

cmake_minimum_required(VERSION 3.22)
# §212.L.4 — CLASS prefix must equal PROJECT_NAME (pkg dir name).
project(talker_pkg VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Resolve the cmake fn registry. The fixture lives under
# `packages/testing/nros-tests/fixtures/multi_pkg_workspace_px4/<pkg>/`
# so the repo root is six levels up.
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME    talker
    CLASS   talker_pkg::talker
    SOURCES src/talker.cpp
    DEPLOY  px4)

nano_ros_deploy(
    TARGET    px4
    RMW       cyclonedds
    DOMAIN_ID 0)
