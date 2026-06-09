cmake_minimum_required(VERSION 3.22)
project(threadx_linux_cpp_service_client VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Message deps resolved from the sibling package.xml SSoT (issue #11) —
# mirrors the native + FreeRTOS C++ shape: nros_find_interfaces reads
# package.xml's <depend>example_interfaces</depend>, generates the C++ bindings, and
# registers example_interfaces__nano_ros_cpp in NROS_GENERATED_INTERFACE_LIBS so
# nano_ros_node_register auto-links it onto the component (Phase 220.G.2).
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)
nano_ros_node_register(
    NAME    service_client
    CLASS   threadx_linux_cpp_service_client::ServiceClient
    SOURCES src/ServiceClient.cpp
    DEPLOY  threadx-linux)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosThreadxSystemCodegen.cmake")
nros_threadx_codegen_system(
    SYSTEM         .
    WORKSPACE_ROOT "${CMAKE_CURRENT_SOURCE_DIR}")

add_executable(threadx_cpp_service_client src/main.c)
target_link_libraries(threadx_cpp_service_client PRIVATE
    threadx_linux_cpp_service_client_service_client_component
    example_interfaces__nano_ros_cpp)
nros_threadx_link_app(threadx_cpp_service_client)
nano_ros_link_rmw(threadx_cpp_service_client RMW "${NROS_RMW}")
