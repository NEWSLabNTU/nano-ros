cmake_minimum_required(VERSION 3.22)
# Phase 212.L.9 — pkg name = CLASS prefix (L.4 rule).
project(threadx_linux_cpp_service_server VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Phase 246 (RFC-0043) — TYPED carrier: constructs the typed ServiceServer +
# `configure(node)` binds the real `handle_add` service callback on the executor.
nano_ros_node_register(
    NAME     service_server
    CLASS    threadx_linux_cpp_service_server::ServiceServer
    LANGUAGE CPP
    HEADER   ServiceServer.hpp
    TYPED
    SOURCES  src/ServiceServer.cpp
    DEPLOY   threadx-linux)

target_link_libraries(threadx_linux_cpp_service_server_service_server_component
    PUBLIC example_interfaces__nano_ros_cpp)

nano_ros_deploy(TARGET threadx-linux RMW ${NROS_RMW} DOMAIN_ID 0)

nano_ros_link_rmw(${PROJECT_NAME} RMW "${NROS_RMW}")
