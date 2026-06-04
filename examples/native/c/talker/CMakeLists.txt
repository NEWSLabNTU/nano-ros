cmake_minimum_required(VERSION 3.22)
# `nano_ros_entry()` + the codegen target_link_libraries chain
# pull in C++ pieces (cbindgen-generated FFI glue, Cyclone optional
# branch); enable both languages so the cmake fns don't trip on
# missing CXX feature detection.
project(c_talker LANGUAGES C CXX)

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
#                                C-side msg bindings closure
#
# The Cyclone DDS backend is C++ (operator new/delete, std::nothrow). A C app
# linking it gets LINKER_LANGUAGE CXX (set by nano_ros_link_rmw), which needs
# CMAKE_CXX_LINK_EXECUTABLE — i.e. CXX enabled in *this* directory scope. The
# nano-ros add_subdirectory enables CXX only in its own subtree, so enable it
# here for the Cyclone RMW. (zenoh / xrce stay pure C.)
if(NROS_RMW STREQUAL "cyclonedds")
    enable_language(CXX)
endif()

# generates C-side bindings (+ FFI glue) for every transitive interface
# dep. Replaces the per-`nros_generate_interfaces(<pkg>)` chain the
# pre-212 shape used.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

nano_ros_entry(
    NAME c_talker
    SOURCES src/main.c
    DEPLOY native)

target_link_libraries(c_talker PRIVATE std_msgs__nano_ros_c)
nros_platform_link_app(c_talker)
