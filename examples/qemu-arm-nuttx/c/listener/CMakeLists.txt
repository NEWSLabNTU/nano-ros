cmake_minimum_required(VERSION 3.22)
project(nuttx_c_listener VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING "Active RMW.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-arm)
# Resolve the nano-ros checkout root once (phase-277 W6 standard guard):
#   1. -DNANO_ROS_ROOT=<path> cache var (explicit, e.g. a copied-out example),
#   2. NROS_REPO_DIR env var (exported by nano-ros's activate.sh),
#   3. in-repo relative walk-up (this example inside the nano-ros tree).
if(NOT DEFINED NANO_ROS_ROOT)
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    else()
        get_filename_component(NANO_ROS_ROOT
            "${CMAKE_CURRENT_SOURCE_DIR}/../../../.." ABSOLUTE)
    endif()
endif()
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)

include("${NANO_ROS_ROOT}/cmake/NanoRosNodeRegister.cmake")

# Phase 240.4 (RFC-0043) — TYPED carrier: the NuttX bootable ELF constructs the
# C component via its `__nros_c_component_<pkg>_*` factory/configure seam
# (NROS_C_COMPONENT) + runs it on the real executor (`NuttxBoard::run_components`),
# instead of the declarative register → interpreter path. The raw subscription
# carries the type name as a string, so no generated message headers are needed.
nano_ros_node_register(
    NAME     listener
    CLASS    nuttx_c_listener::Listener
    LANGUAGE C
    TYPED
    SOURCES  src/Listener.c
    DEPLOY   nuttx)

nano_ros_deploy(TARGET nuttx RMW ${NROS_RMW} DOMAIN_ID 0)
