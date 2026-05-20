cmake_minimum_required(VERSION 3.22)
project(cyclonedds_c_service_client VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.2 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     cyclonedds)
set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../scripts/cyclonedds/msg_to_cyclone_idl.py"
    CACHE FILEPATH "nano-ros Cyclone DDS .msg/.srv to IDL converter" FORCE)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../../.." nano_ros)

nros_generate_interfaces(example_interfaces SKIP_INSTALL)

add_executable(c_service_client src/main.c)
target_link_libraries(c_service_client PRIVATE
    example_interfaces__nano_ros_c
    NanoRos::NanoRos)
nros_platform_link_app(c_service_client)
