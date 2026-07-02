cmake_minimum_required(VERSION 3.22)
# Phase 212.L.9 — pkg name = CLASS prefix (L.4 rule).
project(threadx_linux_cpp_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144.8 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD threadx-linux)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Typed Publisher<String> needs the std_msgs C++ binding. The carrier auto-links
# the generated interface lib (Phase 220.G.2).
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Phase 246 (RFC-0043) — TYPED carrier: the bootable ELF constructs the typed
# Talker component + `configure(node)` creates a typed publisher + binds the real
# `on_tick` timer (publishes the demo String payload) on the executor. No baker, no
# `nros_system_main` stub, no per-example `main.c`.
nano_ros_node_register(
    NAME      talker
    CLASS     threadx_linux_cpp_talker::Talker
    LANGUAGE  CPP
    HEADER    Talker.hpp
    TYPED
    SOURCES   src/Talker.cpp
    DEPLOY    threadx-linux)

target_link_libraries(threadx_linux_cpp_talker_talker_component
    PUBLIC std_msgs__nano_ros_cpp)

nano_ros_deploy(TARGET threadx-linux RMW ${NROS_RMW} DOMAIN_ID 0)

# RMW backend onto the carrier-built ELF (`${PROJECT_NAME}`).
nano_ros_link_rmw(${PROJECT_NAME} RMW "${NROS_RMW}")
