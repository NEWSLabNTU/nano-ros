cmake_minimum_required(VERSION 3.22)
project(baremetal_demo LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 219.I — bootstrap nano-ros workspace helpers. Solo build
# walks up four levels for the nano-ros root; workspace builds inherit
# the helpers from the parent root.
# RMW override: `-DNROS_RMW=<zenoh|xrce|cyclonedds>` (default zenoh).
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()
# Despite the "baremetal_demo" name, this example uses the POSIX
# platform shim — the lesson is the custom `platform_impl.c` the
# example installs at runtime, not a cross-compile to bare-metal.
# Hard-codes zenoh; flipping RMWs is not the lesson here.
nano_ros_entry(
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
