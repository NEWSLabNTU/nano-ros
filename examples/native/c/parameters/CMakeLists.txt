cmake_minimum_required(VERSION 3.22)
# `nano_ros_entry()` + the platform link chain pull in C++ pieces
# (cbindgen-generated FFI glue, Cyclone optional branch); enable both
# languages so the cmake fns don't trip on missing CXX feature detection.
project(c_parameters VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Bootstrap nano-ros workspace helpers. Solo build walks up four levels for
# the nano-ros root; workspace builds inherit the helpers from the parent
# root. RMW override: `-DNROS_RMW=<zenoh|xrce|cyclonedds>` (default zenoh).
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

# Parameter-server demo only — no msg bindings, so no `nros_find_interfaces`
# call (mirrors examples/native/cpp/parameters).
nano_ros_entry(
    NAME c_parameters
    SOURCES src/main.c
    DEPLOY native)

nros_platform_link_app(c_parameters)
