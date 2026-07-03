cmake_minimum_required(VERSION 3.22)
# Phase 212.L.9 — pkg name = CLASS prefix (L.4 rule).
project(riscv64_threadx_cpp_action_server VERSION 0.1.0 LANGUAGES C CXX ASM)

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

# Typed action server needs the example_interfaces C++ binding. The carrier
# auto-links the generated interface lib (Phase 220.G.2). For cyclonedds, the
# same call generates the idlc type support (selected by NROS_RMW).
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)

# Phase 246 (RFC-0043) — TYPED carrier: the bare-metal riscv64 firmware
# constructs the typed ActionServer component + `configure(node)` binds the raw
# action server on `/fibonacci` + a timer driving goal execution on the
# executor. The board startup.c dispatches to the carrier-emitted `app_main`. No
# manual `Executor::open` / spin / locator, no per-example `main.cpp`.
nano_ros_node_register(
    NAME      fibonacci_action_server
    CLASS     riscv64_threadx_cpp_action_server::ActionServer
    LANGUAGE  CPP
    HEADER    ActionServer.hpp
    TYPED
    SOURCES   src/ActionServer.cpp
    DEPLOY    riscv64-qemu)

target_link_libraries(riscv64_threadx_cpp_action_server_fibonacci_action_server_component
    PUBLIC example_interfaces__nano_ros_cpp)

nano_ros_deploy(TARGET riscv64-qemu RMW ${NROS_RMW} DOMAIN_ID 0)

# CycloneDDS needs the per-app Fibonacci descriptor registration (register_* from the
# idlc type support) hooked into nros_rmw_cyclonedds_register_app_descriptors.
if(NROS_RMW STREQUAL "cyclonedds")
    target_sources(${PROJECT_NAME} PRIVATE src/cyclonedds_app.c)
endif()

# RMW backend (zenoh or cyclonedds) onto the carrier-built firmware ELF.
nano_ros_link_rmw(${PROJECT_NAME} RMW "${NROS_RMW}")
