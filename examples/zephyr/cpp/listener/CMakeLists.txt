# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Listener Example (C++) — TYPED component (RFC-0043 / phase-244.C2).
#
# Instead of a hand-written imperative `main.cpp`, the node is a stateful
# component (`Listener::configure(node)`); the Zephyr typed carrier generates the
# entry (`nano_ros_add_node(... TYPED ...)` → ZephyrBoard::run_components).

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(zephyr_cpp_listener LANGUAGES CXX)

# nros zephyr module (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y) auto-loaded
# via `find_package(Zephyr)`; provides zenoh shim, platform support, and
# the nros-cpp library (zephyr_library_named(nros)).
#
# Phase 210.E.3.c — find_package shape via the rclcpp source-compat
# module's smart Find-stub.
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# Cyclone DDS topic descriptors for this image's interface packages are generated
# by the shared interface pipeline (`zephyr/cmake/nros_generate_interfaces.cmake`)
# when `find_package(<pkg>)` runs below. A hand-written descriptor block here
# used to generate the same descriptors a second time; with the global
# `--allow-multiple-definition` gone (issue 1636) that was a duplicate-symbol
# link error, so it is gone too (issue 1673).

# The Zephyr typed carrier branch keys on NANO_ROS_PLATFORM=zephyr + DEPLOY
# zephyr + TARGET app + TYPED, and target_sources/target_link_libraries the
# generated entry + component lib into the global `app`. No second executable.

nano_ros_auto_add_library(listener_lib STATIC src/Listener.cpp)

nros_components_register_node(listener_lib
    HEADER Listener.hpp
    PLUGIN zephyr_cpp_listener::Listener
    EXECUTABLE listener
    SHAPE configure
    TYPED
    # phase-412 -- `ENTITIES` is retired. A standalone Zephyr application has
    # no launch file, so it has no contract sidecar to state itself in; what
    # this image creates is stated as the XRCE session caps in prj-xrce.conf,
    # which is what phase-412's own guidance leaves such an image ("a system
    # with no contract yet keeps its configured" knobs). See issue 1033.
)
