# SPDX-License-Identifier: Apache-2.0
# nros Zephyr Talker (C) — Phase 168.4 collapsed shape.
#
# Multi-RMW C example. Selection via prj-<rmw>.conf overlay:
#   west build -b native_sim/native/64 -d build-c-talker-<rmw> \
#     examples/zephyr/c/talker -- -DCONF_FILE="prj.conf;prj-<rmw>.conf"

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_talker_c)

nros_generate_interfaces(std_msgs "msg/Int32.msg")

target_sources(app PRIVATE src/main.c)
