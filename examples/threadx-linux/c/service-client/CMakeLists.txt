cmake_minimum_required(VERSION 3.22)
# Phase 212.L Component pkg — class prefix must equal PROJECT_NAME (L.4).
project(threadx_c_service_client VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Phase 246 (RFC-0043) — TYPED carrier: the bootable ELF constructs the C
# ServiceClient component + `client_configure` creates a service client + a poll
# timer. Raw register carries the type-name verbatim, no generated message
# sources. No manual support/executor init, no spin loop, no main.c.
nano_ros_node_register(
    NAME     service_client
    CLASS    threadx_c_service_client::ServiceClient
    LANGUAGE C
    TYPED
    SOURCES  src/ServiceClient.c
    DEPLOY   threadx-linux)

nano_ros_deploy(TARGET threadx-linux RMW ${NROS_RMW} DOMAIN_ID 0)

nano_ros_link_rmw(${PROJECT_NAME} RMW "${NROS_RMW}")
