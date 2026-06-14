# SPDX-License-Identifier: Apache-2.0
# nano-ros Zephyr BYO starter app (zenoh, C) — TYPED component
# (RFC-0043 / phase-244.C4). The node is a stateful C component
# (`NROS_C_COMPONENT(talker_t, talker_configure)`); the nano-ros module's Zephyr
# typed carrier (`zephyr_entry_main_c_typed.cpp.in`) generates the entry +
# composes it into `app` (ZephyrBoard::run_components) — no hand-written
# `nros_app_main`. The raw publisher carries the type name as a string, so no
# generated message headers are needed (copy this app + edit topic/logic).
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nano_ros_byo_talker)

# NROS_REPO_DIR is published as a CACHE var by the nano-ros Zephyr module
# (auto-loaded via find_package(Zephyr) when CONFIG_NROS=y).
set(NANO_ROS_PLATFORM zephyr)
include("${NROS_REPO_DIR}/cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME      byo_talker
    CLASS     nano_ros_byo_talker::Talker
    LANGUAGE  C
    TYPED
    SOURCES   src/Talker.c
    DEPLOY    zephyr)
