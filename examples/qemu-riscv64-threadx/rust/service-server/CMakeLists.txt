cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_rust_service_server_cyclonedds VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 175.B — ThreadX Rust CycloneDDS uses the same C startup path as
# the C/C++ examples. CMake owns Cyclone's C++ backend + generated
# descriptors; the Rust crate is imported as a staticlib exposing
# app_main().
set(NANO_ROS_PLATFORM threadx)
set(NROS_RMW "cyclonedds" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "${NROS_RMW}" CACHE STRING
    "nano-ros root RMW backend" FORCE)
set(NANO_ROS_BOARD riscv64-qemu)
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
set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
    "${NANO_ROS_ROOT}/scripts/cyclonedds/msg_to_cyclone_idl.py"
    CACHE FILEPATH "nano-ros Cyclone DDS .msg/.srv to IDL converter" FORCE)
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)
include("${NANO_ROS_ROOT}/packages/dds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake")

nros_generate_interfaces(example_interfaces "srv/AddTwoInts.srv" LANGUAGE C SKIP_INSTALL)
# Phase 212.M.10 — `nros.toml` retired; the Config literal lives in
# `src/lib.rs::start_from_reset()`. The Cyclone path here doesn't
# reference `NROS_APP_CONFIG`, so the generated app_config.h was dead
# weight + dropped along with the source-of-truth toml.

# Issue #205 step 3 — the corrosion import, generated link-anchor TU,
# executable, and platform/RMW link calls collapse into the board-overlay
# seam (cmake/board/nano-ros-board-riscv64-qemu.cmake). The Rust crate's
# `cyclonedds_app_main!(register)` (board macro) provides `app_main`.
nros_threadx_rv64_rust_cyclone_app(riscv64_threadx_rust_service_server_cyclonedds
    CRATE qemu-riscv64-threadx-service-server
    LINK  example_interfaces__nano_ros_c)
