cmake_minimum_required(VERSION 3.22)
project(baremetal_demo LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.1 — consume nano-ros via add_subdirectory(<repo-root>).
# Despite the "baremetal_demo" name, this example uses the POSIX
# platform shim — the lesson is the custom platform_impl.c the
# example installs at runtime, not a cross-compile to bare-metal.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

add_executable(baremetal_demo
    src/main.c
    src/platform_impl.c)
target_link_libraries(baremetal_demo PRIVATE NanoRos::NanoRos)
nros_platform_link_app(baremetal_demo)

# Platform-specific libraries not covered by NanoRos::NanoRos.
if(UNIX AND NOT APPLE)
    target_link_libraries(baremetal_demo PRIVATE rt)
elseif(APPLE)
    target_link_libraries(baremetal_demo PRIVATE "-framework CoreFoundation")
endif()

target_compile_options(baremetal_demo PRIVATE
    -Wall -Wextra -Wpedantic
    -Wno-unused-result)
