# SPDX-License-Identifier: Apache-2.0
# nano-ros Zephyr starter app (zenoh, C). Stock Zephyr app: the nano-ros module
# links NanoRos::NanoRos once CONFIG_NROS=y. `nros_generate_interfaces()` is
# provided by the module — it generates the C bindings for the listed messages.
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nano_ros_byo_talker)

nros_generate_interfaces(std_msgs "msg/Int32.msg")

target_sources(app PRIVATE src/main.c)
