# SPDX-License-Identifier: Apache-2.0
# Phase 117.14 — nros pub/sub demo on the ARM FVP Base_RevC AEMv8-R
# Cortex-A SMP target under Zephyr. Copy-out template per CLAUDE.md
# examples convention — no walk-up paths, no shared example-only
# helpers. The nros module supplies the C++ API + codegen.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_aemv8r_cyclonedds_talker LANGUAGES CXX)

# Generate C++ message bindings for the String type used by the talker.
# `nros_generate_interfaces()` is provided by the nros Zephyr module
# (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y). KEEP this (not the rclcpp-compat
# `find_package` + explicit `NROS_CYCLONE_*` descriptor block the native_sim
# reference uses) — it already emits the Cyclone C descriptor for this FVP build
# path, and `build-fvp-aemv8r-cyclonedds` does not export `NROS_CYCLONE_*`.
nros_generate_interfaces(std_msgs LANGUAGE CPP)

# phase-244.C2.1 — typed carrier. Instead of an imperative `main.cpp`, the node is
# a stateful component (`Talker::configure(node)`); the Zephyr typed carrier
# generates the entry (`nano_ros_add_node(... TYPED ...)` →
# ZephyrBoard::run_components) and target_sources/links it into the global `app`.
find_package(nano_ros REQUIRED)

nano_ros_add_node(talker CLASS nros_zephyr_aemv8r_cyclonedds_talker::Talker
    HEADER Talker.hpp
    TYPED
    SOURCES src/Talker.cpp)
