cmake_minimum_required(VERSION 3.22)
project(freertos_c_action_client VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM freertos)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    mps2-an385-freertos)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")
nros_generate_interfaces(example_interfaces "action/Fibonacci.action" LANGUAGE C SKIP_INSTALL)

nano_ros_node_register(
    NAME    action_client
    CLASS   freertos_c_action_client::FibonacciClient
    SOURCES src/FibonacciClient.c ${example_interfaces_GENERATED_SOURCES}
    DEPLOY  freertos)

nano_ros_deploy(
    TARGET     freertos
    RMW        ${NROS_RMW}
    DOMAIN_ID  0
    LOCATOR    "tcp/10.0.2.2:7447")

# Phase 214 followup — link interface lib for generated headers.
target_link_libraries(freertos_c_action_client_action_client_component PUBLIC example_interfaces__nano_ros_c)
