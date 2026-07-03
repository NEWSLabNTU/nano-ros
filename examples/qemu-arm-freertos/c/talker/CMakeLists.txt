cmake_minimum_required(VERSION 3.22)
# Phase 212.L Component pkg — class prefix must equal PROJECT_NAME (L.4).
project(freertos_c_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM freertos)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    mps2-an385-freertos)
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

# Phase 240.6 (RFC-0043) — TYPED carrier: the bootable ELF constructs the C
# Talker component + `configure(node, executor)` creates a raw publisher + a
# timer publishing the std_msgs/String demo payload. The raw publish carries the
# DDS-mangled keyexpr verbatim, so no generated message sources are needed.
nano_ros_node_register(
    NAME     talker
    CLASS    freertos_c_talker::Talker
    LANGUAGE C
    TYPED
    SOURCES  src/Talker.c
    DEPLOY   freertos)

# Per-target deploy config (Phase 212.L.8).
nano_ros_deploy(
    TARGET     freertos
    RMW        ${NROS_RMW}
    DOMAIN_ID  0
    LOCATOR    "tcp/10.0.2.2:7447")
