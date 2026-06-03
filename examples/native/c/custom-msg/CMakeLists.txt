cmake_minimum_required(VERSION 3.22)
project(native_c_custom_msg VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 212.M (native/c sweep) — Application pkg shape, C variant.
# Hybrid example: this pkg IS its own interface pkg (member_of_group=
# rosidl_interface_packages in package.xml) AND ships a smoke binary
# (`test_messages`) that exercises the generated C bindings.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW zenoh)

set(_NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../..")
add_subdirectory("${_NANO_ROS_ROOT}" nano_ros)
include("${_NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

# Generate C msg bindings for THIS pkg's own msg/ dir.
nros_generate_interfaces(${PROJECT_NAME} SKIP_INSTALL)

nano_ros_entry(
    NAME test_messages
    SOURCES src/main.c
    DEPLOY native)

target_link_libraries(test_messages PRIVATE
    ${PROJECT_NAME}__nano_ros_c)
nros_platform_link_app(test_messages)
