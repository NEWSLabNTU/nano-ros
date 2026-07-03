cmake_minimum_required(VERSION 3.22)
# Phase 212.L Component pkg — class prefix must equal PROJECT_NAME (L.4).
project(threadx_c_listener VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    threadx-linux)
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

# package.xml SSoT: generate the typed C bindings for the interface closure
# (std_msgs) — the component uses the generated String struct + serializer
# (phase-277 W4; was hand-rolled CDR).
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

# Phase 246 (RFC-0043) — TYPED carrier: the bootable ELF constructs the C Listener
# component + `listener_configure` binds a raw zero-copy subscription on /chatter.
# Raw register carries the type-name verbatim, no generated message sources. No
# manual support/executor init, no spin loop, no per-example main.c.
nano_ros_node_register(
    NAME     listener
    CLASS    threadx_c_listener::Listener
    LANGUAGE C
    TYPED
    SOURCES  src/Listener.c
    DEPLOY   threadx-linux)

nano_ros_deploy(TARGET threadx-linux RMW ${NROS_RMW} DOMAIN_ID 0)

nano_ros_link_rmw(${PROJECT_NAME} RMW "${NROS_RMW}")
