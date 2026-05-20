cmake_minimum_required(VERSION 3.22)
project(cyclonedds_cpp_listener VERSION 0.1.0 LANGUAGES CXX C)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144.4 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     cyclonedds)
set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../scripts/cyclonedds/msg_to_cyclone_idl.py"
    CACHE FILEPATH "nano-ros Cyclone DDS .msg/.srv to IDL converter" FORCE)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)

add_executable(cpp_listener src/main.cpp)
target_link_libraries(cpp_listener PRIVATE
    std_msgs__nano_ros_cpp
    NanoRos::NanoRosCpp)
nros_platform_link_app(cpp_listener)
