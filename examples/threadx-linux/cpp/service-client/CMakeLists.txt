cmake_minimum_required(VERSION 3.22)
# Phase 212.L.9 — pkg name = CLASS prefix (L.4 rule).
project(threadx_linux_cpp_service_client VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD threadx-linux)
# Resolve the nano-ros checkout root once (phase-277 W6 standard guard):
#   1. -DNANO_ROS_ROOT=<path> cache var (explicit, e.g. a copied-out example),
#   2. NROS_REPO_DIR env var (exported by nano-ros's activate.sh),
#   3. in-repo relative walk-up (this example inside the nano-ros tree).
if(NOT DEFINED NANO_ROS_ROOT)
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        get_filename_component(NANO_ROS_ROOT
            "${CMAKE_CURRENT_SOURCE_DIR}/../../../.." ABSOLUTE)
    endif()
endif()
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)

include("${NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Phase 246 (RFC-0043) — TYPED carrier: constructs the typed ServiceClient +
# `configure(node)` creates the service client + binds the poll-driver timer.
nano_ros_node_register(
    NAME     add_two_ints_client
    CLASS    threadx_linux_cpp_service_client::ServiceClient
    LANGUAGE CPP
    HEADER   ServiceClient.hpp
    TYPED
    SOURCES  src/ServiceClient.cpp
    DEPLOY   threadx-linux)

target_link_libraries(threadx_linux_cpp_service_client_add_two_ints_client_component
    PUBLIC example_interfaces__nano_ros_cpp)

nano_ros_deploy(TARGET threadx-linux RMW ${NROS_RMW} DOMAIN_ID 0)

nano_ros_link_rmw(${PROJECT_NAME} RMW "${NROS_RMW}")
