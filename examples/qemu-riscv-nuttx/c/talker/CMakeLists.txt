cmake_minimum_required(VERSION 3.22)
# Phase 212.L Component pkg — class prefix must equal PROJECT_NAME (L.4).
project(nuttx_riscv_c_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.6 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-riscv)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Phase 240.6 (RFC-0043) — TYPED carrier: the bootable ELF constructs the C
# Talker component + `talker_configure(node, executor)` creates a raw publisher +
# a timer publishing the std_msgs/String demo payload (hand-encoded CDR —
# the NuttX kernel link has no generated-interface plumbing yet, so the
# typed generated C bindings the other platforms use are unavailable here).
# The raw publish carries the DDS-mangled keyexpr verbatim.
nano_ros_node_register(
    NAME     talker
    CLASS    nuttx_riscv_c_talker::Talker
    LANGUAGE C
    TYPED
    SOURCES  src/Talker.c
    DEPLOY   nuttx)

# Per-target deploy config (Phase 212.L.8).
nano_ros_deploy(
    TARGET     nuttx
    RMW        ${NROS_RMW}
    DOMAIN_ID  0)
