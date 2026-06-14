cmake_minimum_required(VERSION 3.22)
project(freertos_c_service_server VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM freertos)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    mps2-an385-freertos)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Phase 240.6 (RFC-0043) — TYPED carrier: the bootable ELF constructs the C
# component via its factory/configure seam (NROS_C_COMPONENT) + registers the
# real service handler on the executor. The raw service register carries the
# type name as a string, so no generated message headers are needed.
nano_ros_node_register(
    NAME     service_server
    CLASS    freertos_c_service_server::AddTwoIntsServer
    LANGUAGE C
    TYPED
    SOURCES  src/AddTwoIntsServer.c
    DEPLOY   freertos)

nano_ros_deploy(
    TARGET     freertos
    RMW        ${NROS_RMW}
    DOMAIN_ID  0
    LOCATOR    "tcp/10.0.2.2:7447")
