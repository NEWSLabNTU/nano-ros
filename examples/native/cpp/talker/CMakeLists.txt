cmake_minimum_required(VERSION 3.22)
project(cpp_talker LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 212.M.2 — Application pkg shape (§212.L.2 C++ variant).
#   * `package.xml`            — pkg identity + msg dep declaration
#   * `nros_find_interfaces`   — reads package.xml, generates the
#                                msg/srv/action bindings closure
#   * `nano_ros_entry()` — declares the exec entity + appends
#                                metadata for `nros codegen-system`
#
# RMW selected at cmake configure time via `-DNROS_RMW=<rmw>` (default
# zenoh). Per the §212.L.9 cmake fn surface, `NANO_ROS_PLATFORM` /
# `NANO_ROS_RMW` are STILL set here (the root CMake reads them at
# `add_subdirectory()` time — the cmake fns own the application entity
# wiring, not the per-target platform/RMW cache vars).
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds).")
set(NANO_ROS_RMW "${NROS_RMW}")

set(_NANO_ROS_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../../..")
add_subdirectory("${_NANO_ROS_ROOT}" nano_ros)

# `NanoRosNodeRegister.cmake` ships the §212.L.9 cmake fns
# (`nano_ros_entry`, `nano_ros_node_register`,
# `nano_ros_deploy`). The root CMake does NOT auto-load it today, so
# the per-example CMakeLists includes it directly — same pattern as
# the §212.L.9 fixture tests (`phase212_l9_cmake_fns.rs`).
include("${_NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

# Phase 210.E.4 — `nros_find_interfaces` reads `package.xml` and
# generates bindings (+ FFI glue) for every transitive interface dep.
# Replaces the per-`find_package(<pkg>)` chain the pre-212 shape used.
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

nano_ros_entry(
    NAME cpp_talker
    SOURCES src/main.cpp
    DEPLOY native)

target_link_libraries(cpp_talker PRIVATE std_msgs__nano_ros_cpp)
nros_platform_link_app(cpp_talker)
