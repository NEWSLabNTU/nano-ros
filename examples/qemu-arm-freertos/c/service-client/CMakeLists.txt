cmake_minimum_required(VERSION 3.22)
project(freertos_c_service_client VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM freertos)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    mps2-an385-freertos)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_node_register(
    NAME    service_client
    CLASS   freertos_c_service_client::AddTwoIntsClient
    SOURCES src/AddTwoIntsClient.c ${example_interfaces_GENERATED_SOURCES}
    DEPLOY  freertos)

nano_ros_deploy(
    TARGET     freertos
    RMW        ${NROS_RMW}
    DOMAIN_ID  0
    LOCATOR    "tcp/10.0.2.2:7447")
