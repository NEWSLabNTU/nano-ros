cmake_minimum_required(VERSION 3.22)
project(nuttx_c_service_client VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")
nros_generate_interfaces(example_interfaces "srv/AddTwoInts.srv" LANGUAGE C SKIP_INSTALL)

nano_ros_node_register(
    NAME    service_client
    CLASS   nuttx_c_service_client::AddTwoIntsClient
    SOURCES src/AddTwoIntsClient.c ${example_interfaces_GENERATED_SOURCES}
    DEPLOY  nuttx)

nano_ros_deploy(TARGET nuttx RMW ${NROS_RMW} DOMAIN_ID 0)

# Phase 214 followup — link interface lib so generated headers
# (e.g. ${pkg}__nano_ros_c-provided example_interfaces/example_interfaces.h) are on the include
# path for src/*.c sources that #include "example_interfaces.h".
target_link_libraries(nuttx_c_service_client_service_client_component PUBLIC example_interfaces__nano_ros_c)
