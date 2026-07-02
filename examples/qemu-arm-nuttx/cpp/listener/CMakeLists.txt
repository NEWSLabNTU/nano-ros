cmake_minimum_required(VERSION 3.22)
project(nuttx_cpp_listener VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# package.xml SSoT: generate the typed C++ bindings for the interface closure
# (std_msgs) — the component decodes /chatter via the generated String
# deserializer (phase-277 W4; was hand-rolled CDR).
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Phase 240.3 (RFC-0043) — TYPED carrier: the NuttX bootable ELF constructs
# the `Listener` component + calls `configure(node)` on the real executor
# (`NuttxBoard::run_components`), instead of the declarative register-symbol →
# interpreter path. HEADER is the component header relative to the `src`
# include dir (the carrier `#include`s it to construct the class).
nano_ros_node_register(
    NAME     listener
    CLASS    nuttx_cpp_listener::Listener
    LANGUAGE CPP
    HEADER   Listener.hpp
    TYPED
    SOURCES  src/Listener.cpp
    DEPLOY   nuttx)

nano_ros_deploy(TARGET nuttx RMW ${NROS_RMW} DOMAIN_ID 0)
