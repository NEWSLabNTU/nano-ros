# SPDX-License-Identifier: Apache-2.0
# nros Zephyr XRCE Talker Example (C)

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_xrce_talker_c)

# nros module (CONFIG_NROS=y, CONFIG_NROS_C_API=y, CONFIG_NROS_RMW_XRCE=y)
# provides XRCE transport glue and the nros-c library automatically.

nros_generate_interfaces(std_msgs "msg/Int32.msg")

target_sources(app PRIVATE src/main.c)
