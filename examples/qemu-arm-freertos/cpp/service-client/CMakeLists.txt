cmake_minimum_required(VERSION 3.22)
# Phase 212.L Component pkg — class prefix must equal PROJECT_NAME (L.4).
project(freertos_cpp_service_client VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM freertos)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    mps2-an385-freertos)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Typed service client needs the example_interfaces C++ binding (AddTwoInts).
# The carrier auto-links the generated interface lib.
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Phase 240.6 (RFC-0043) — TYPED carrier: the bootable ELF constructs the typed
# AddTwoIntsClient component + `configure(node)` creates a typed service client +
# binds the real `on_tick` poll timer on the executor.
nano_ros_node_register(
    NAME      add_two_ints_client
    CLASS     freertos_cpp_service_client::AddTwoIntsClient
    LANGUAGE  CPP
    HEADER    AddTwoIntsClient.hpp
    TYPED
    SOURCES   src/AddTwoIntsClient.cpp
    DEPLOY    freertos)

target_link_libraries(freertos_cpp_service_client_add_two_ints_client_component
    PUBLIC example_interfaces__nano_ros_cpp)

nano_ros_deploy(
    TARGET     freertos
    RMW        ${NROS_RMW}
    DOMAIN_ID  0
    LOCATOR    "tcp/10.0.2.2:7447")
