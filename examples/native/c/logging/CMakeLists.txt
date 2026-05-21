cmake_minimum_required(VERSION 3.22)
project(c_logging LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 88.13 — minimal C logging demo. Uses `nros::init` to bring
# up the executor (and thus the per-platform `nros_platform_log_*`
# delivery), then calls `NROS_LOG_*` macros against the node's
# Logger handle.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

add_executable(c_logging src/main.c)
target_link_libraries(c_logging PRIVATE NanoRos::NanoRos)
nros_platform_link_app(c_logging)
