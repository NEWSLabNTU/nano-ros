cmake_minimum_required(VERSION 3.22)
# Phase 212.L Component pkg — class prefix must equal PROJECT_NAME (L.4).
project(nuttx_riscv_c_talker VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 144.6 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM nuttx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    nuttx-qemu-riscv)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")
# Message deps resolved from the sibling package.xml SSoT (issue #11)
# instead of a hardcoded interface-file list.
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

# Phase 212.L.9 — declarative Component pkg shape. No add_executable
# here, no `main()`; the generated runtime (emitted by `nros codegen-
# system` via the integrations/nuttx H.2 adapter shim) owns the entry
# point, executor init, and the spin loop.
nano_ros_node_register(
    NAME    talker
    CLASS   nuttx_riscv_c_talker::Talker
    SOURCES src/Talker.c ${std_msgs_GENERATED_SOURCES}
    DEPLOY  nuttx)

# Per-target deploy config (Phase 212.L.8).
nano_ros_deploy(
    TARGET     nuttx
    RMW        ${NROS_RMW}
    DOMAIN_ID  0)
