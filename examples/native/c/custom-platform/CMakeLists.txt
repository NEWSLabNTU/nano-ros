cmake_minimum_required(VERSION 3.22)
project(baremetal_demo LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 212.M (native/c sweep) — Application pkg shape, C variant.
# Despite the "baremetal_demo" name, this example uses the POSIX
# platform shim — the lesson is the custom `platform_impl.c` the
# example installs at runtime, not a cross-compile to bare-metal.
# Hard-codes zenoh; flipping RMWs is not the lesson here.
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW zenoh)

set(_NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../..")
add_subdirectory("${_NANO_ROS_ROOT}" nano_ros)
include("${_NANO_ROS_ROOT}/cmake/NanoRosComponentRegister.cmake")

nano_ros_application(
    NAME baremetal_demo
    SOURCES src/main.c src/platform_impl.c
    DEPLOY native)

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
