# SPDX-License-Identifier: Apache-2.0
# nano-ros Zephyr BYO starter app (zenoh, C) — TYPED component
# (RFC-0043 / phase-244.C4). The node is a stateful C component
# (`RCLC_C_COMPONENT(talker_t, talker_configure)`); the nano-ros module's Zephyr
# typed carrier (the shared entry pack) generates the entry +
# composes it into `app` (ZephyrBoard::run_components) — no hand-written
# `nros_app_main`. The node publishes `std_msgs/Int32` through the generated C
# bindings, so the app declares `std_msgs` here and in `package.xml` (issue
# 2140: it once claimed to need none, and a copy did not compile).
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nano_ros_byo_talker)

# The nano-ros Zephyr module (auto-loaded via find_package(Zephyr) when
# CONFIG_NROS=y) provides both packages; find_package(std_msgs) generates the
# typed C bindings Talker.c includes. The same shape as examples/zephyr/c/talker.
find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(byo_talker_lib STATIC src/Talker.c)

nros_components_register_node(byo_talker_lib
    PLUGIN nano_ros_byo_talker::Talker
    EXECUTABLE byo_talker
    SHAPE configure
    TYPED
    DEPLOY zephyr)
