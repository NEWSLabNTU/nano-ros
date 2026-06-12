cmake_minimum_required(VERSION 3.22)
project(cpp_service_client_callback LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 219.I — bootstrap nano-ros workspace helpers. Solo build
# walks up four levels for the nano-ros root; workspace builds inherit
# the helpers from the parent root.
# RMW override: `-DNROS_RMW=<zenoh|xrce|cyclonedds>` (default zenoh).
if(NOT COMMAND nano_ros_workspace_pkg_guard)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosWorkspace.cmake")
endif()
nano_ros_workspace_pkg_guard()

nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_entry(
    NAME cpp_service_client_callback
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries(cpp_service_client_callback PRIVATE example_interfaces__nano_ros_cpp)
nros_platform_link_app(cpp_service_client_callback)
