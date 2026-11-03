# SPDX-License-Identifier: Apache-2.0
# phase-521 W7 (RFC-0113 D2) -- the idle-wake witness on mps2/an385: one idle
# subscription and one 1 s timer, and a count of how often the executor's
# thread woke (src/Witness.c). Consumed by tests/zephyr_cortex_m_qemu.rs.
cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_idle_wake_witness_c)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(witness_lib STATIC src/Witness.c)

nros_components_register_node(witness_lib
    PLUGIN nros_idle_wake_witness_c::Witness
    EXECUTABLE witness
    SHAPE configure
    TYPED)
