# SPDX-License-Identifier: Apache-2.0
# phase-521 W7 (RFC-0113 D2) -- the idle-wake witness on mps2/an385: one idle
# subscription and one 1 s timer, and a count of how often the executor's
# thread woke (src/Witness.c). Consumed by tests/zephyr_cortex_m_qemu.rs.
cmake_minimum_required(VERSION 3.20.0)

project(nros_idle_wake_witness_c)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

# The board measurement (tracing hooks, devicetree IRQ, cycle counter) is
# Zephyr's, so its source is built only inside a Zephyr build; the host
# metadata probe compiles the node against the host seam instead, which is
# what lets it read the node's declarations at all. `zephyr_library_sources`
# exists only in a Zephyr build.
if(COMMAND zephyr_library_sources)
  set(_witness_board src/witness_board_zephyr.c)
else()
  set(_witness_board src/witness_board_host.c)
endif()
nano_ros_auto_add_library(witness_lib STATIC src/Witness.c ${_witness_board})
target_include_directories(witness_lib PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/src)

nros_components_register_node(witness_lib
    PLUGIN nros_idle_wake_witness_c::Witness
    EXECUTABLE witness
    SHAPE configure
    TYPED)
