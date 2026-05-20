cmake_minimum_required(VERSION 3.22)
project(cyclonedds_c_action_client VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.2 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     cyclonedds)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

nros_generate_interfaces(example_interfaces SKIP_INSTALL)

add_executable(c_action_client src/main.c)
target_link_libraries(c_action_client PRIVATE
    example_interfaces__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(c_action_client)
