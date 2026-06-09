cmake_minimum_required(VERSION 3.22)
project(freertos_cpp_action_server VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM freertos)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    mps2-an385-freertos)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_node_register(
    NAME    action_server
    CLASS   freertos_cpp_action_server::FibonacciServer
    SOURCES src/FibonacciServer.cpp ${example_interfaces_GENERATED_SOURCES}
    DEPLOY  freertos)

nano_ros_deploy(
    TARGET     freertos
    RMW        ${NROS_RMW}
    DOMAIN_ID  0
    LOCATOR    "tcp/10.0.2.2:7447")
