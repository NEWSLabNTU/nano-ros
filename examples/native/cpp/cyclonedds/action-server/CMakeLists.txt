cmake_minimum_required(VERSION 3.22)
project(cyclonedds_cpp_action_server VERSION 0.1.0 LANGUAGES CXX C)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144.4 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     cyclonedds)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

nros_generate_interfaces(example_interfaces LANGUAGE CPP SKIP_INSTALL)

add_executable(cpp_action_server src/main.cpp)
target_link_libraries(cpp_action_server PRIVATE
    example_interfaces__nano_ros_cpp
    NanoRos::NanoRosCpp)
nros_platform_link_app(cpp_action_server)
