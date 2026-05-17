cmake_minimum_required(VERSION 3.22)
project(native_c_custom_msg VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.1 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

# Generate C message bindings for the in-tree custom-msg package.
nros_generate_interfaces(${PROJECT_NAME} SKIP_INSTALL)

add_executable(test_messages src/main.c)
target_link_libraries(test_messages PRIVATE
    ${PROJECT_NAME}__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(test_messages)
