cmake_minimum_required(VERSION 3.22)
project(nuttx_c_action_server VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")
# Message deps resolved from the sibling package.xml SSoT (issue #11).
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_node_register(
    NAME    action_server
    CLASS   nuttx_c_action_server::FibonacciServer
    SOURCES src/FibonacciServer.c ${example_interfaces_GENERATED_SOURCES}
    DEPLOY  nuttx)

nano_ros_deploy(TARGET nuttx RMW ${NROS_RMW} DOMAIN_ID 0)
