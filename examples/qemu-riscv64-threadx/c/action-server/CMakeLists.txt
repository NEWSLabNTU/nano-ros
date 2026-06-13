cmake_minimum_required(VERSION 3.22)
# Phase 212.L Component pkg — class prefix must equal PROJECT_NAME (L.4).
project(riscv64_threadx_c_action_server VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 144.7 — consume nano-ros via add_subdirectory(<repo-root>).
set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}")
set(NANO_ROS_BOARD    riscv64-qemu)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")

# Generated C interface (idlc type support is selected by NROS_RMW for cyclone).
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

# Phase 246 (RFC-0043) — TYPED carrier: the bare-metal riscv64 firmware
# constructs the C ActionServer component + `server_configure(node, executor)`
# binds goal/cancel callbacks as a raw action server on `/fibonacci` plus a timer
# that drives goal execution on the executor. The board startup.c dispatches to
# the carrier-emitted `app_main`. No manual support/executor init, no spin loop,
# no per-example `main.c`.
nano_ros_node_register(
    NAME     action_server
    CLASS    riscv64_threadx_c_action_server::ActionServer
    LANGUAGE C
    TYPED
    SOURCES  src/ActionServer.c
    DEPLOY   riscv64-qemu)

nano_ros_deploy(TARGET riscv64-qemu RMW ${NROS_RMW} DOMAIN_ID 0)

# CycloneDDS needs the per-app Fibonacci descriptor registration (register_* from the
# idlc type support) hooked into nros_rmw_cyclonedds_register_app_descriptors.
if(NROS_RMW STREQUAL "cyclonedds")
    target_sources(${PROJECT_NAME} PRIVATE src/cyclonedds_app.c)
endif()

# RMW backend (zenoh or cyclonedds) onto the carrier-built firmware ELF.
nano_ros_link_rmw(${PROJECT_NAME} RMW "${NROS_RMW}")
