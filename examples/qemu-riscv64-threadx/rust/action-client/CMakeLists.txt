cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_rust_action_client_cyclonedds VERSION 0.1.0 LANGUAGES C CXX ASM)

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
set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../../scripts/cyclonedds/msg_to_cyclone_idl.py"
    CACHE FILEPATH "nano-ros Cyclone DDS .msg/.srv to IDL converter" FORCE)
add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)
include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../packages/dds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake")

nros_generate_interfaces(example_interfaces "action/Fibonacci.action" LANGUAGE C SKIP_INSTALL)
# Phase 212.M.10 — `nros.toml` retired; the Config literal lives in
# `src/lib.rs::start_from_reset()`. The Cyclone path here doesn't
# reference `NROS_APP_CONFIG`, so the generated app_config.h was dead
# weight + dropped along with the source-of-truth toml.

corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
    CRATES qemu-riscv64-threadx-action-client
    CRATE_TYPES staticlib
    NO_DEFAULT_FEATURES
    FEATURES rmw-cyclonedds)

add_executable(riscv64_threadx_rust_action_client_cyclonedds src/cyclonedds_app.c)
target_link_libraries(riscv64_threadx_rust_action_client_cyclonedds PRIVATE
    qemu_riscv64_threadx_action_client
    example_interfaces__nano_ros_c
    NanoRos::NanoRos
    "-Wl,--allow-multiple-definition")
nros_platform_link_app(riscv64_threadx_rust_action_client_cyclonedds)
nano_ros_link_rmw(riscv64_threadx_rust_action_client_cyclonedds RMW "${NROS_RMW}")
