cmake_minimum_required(VERSION 3.22)
# Phase 212.L Component pkg — class prefix must equal PROJECT_NAME (L.4).
project(threadx_c_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.8 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Phase 246 (RFC-0043) — TYPED carrier: the bootable ELF constructs the C Talker
# component + `talker_configure(node, executor)` creates a raw publisher + a timer
# publishing a CDR-encoded Int32 counter. Raw publish carries the DDS-mangled
# keyexpr verbatim, so no generated message sources are needed. No manual
# support/executor init, no spin loop, no per-example main.c.
nano_ros_node_register(
    NAME     talker
    CLASS    threadx_c_talker::Talker
    LANGUAGE C
    TYPED
    SOURCES  src/Talker.c
    DEPLOY   threadx-linux)

nano_ros_deploy(TARGET threadx-linux RMW ${NROS_RMW} DOMAIN_ID 0)

# RMW backend onto the carrier-built ELF (`${PROJECT_NAME}`).
nano_ros_link_rmw(${PROJECT_NAME} RMW "${NROS_RMW}")
