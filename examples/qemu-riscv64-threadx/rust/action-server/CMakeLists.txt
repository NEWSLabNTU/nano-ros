cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_rust_action_server VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 175.B — ThreadX Rust CycloneDDS uses the same C startup path as
# the C/C++ examples. CMake owns Cyclone's C++ backend + generated
# descriptors; the Rust crate is imported as a staticlib exposing
# app_main().
# issue 0835 — the caller decides, and this is only the copy-out default.
#
# This line used to read `set(NANO_ROS_PLATFORM threadx)`, an unconditional
# normal variable that SHADOWED the `-DNANO_ROS_PLATFORM=threadx_riscv64` this
# leaf's own build passes (`just/threadx-riscv64.just`, both the Rust
# `nros_cmake_fixture_build` path and the C/C++ `NROS_CMAKE_EXTRA_DEFS` one).
# The leaf therefore reported one platform while its CMakeCache recorded
# another, both written in the same second, and the shared cargo directory keyed
# on the label got FOUR groups where the configuration space has two:
#
#   platform=threadx         |rmw=zenoh     |board=riscv64-qemu|…   <- these six
#   platform=threadx_riscv64 |rmw=zenoh     |board=riscv64-qemu|…   <- the c/cpp leaves
#   platform=threadx         |rmw=cyclonedds|board=riscv64-qemu|…
#   platform=threadx_riscv64 |rmw=cyclonedds|board=riscv64-qemu|…
#
# — 2.8 GB of duplicate build output for a spelling difference. The key now
# hashes the resolved feature set rather than the label (`8033bc3f0`), so the
# duplication is already gone; this makes the leaf stop lying, which is the
# other half and the one that keeps the next label-keyed thing from repeating
# it.
if(NOT DEFINED NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM threadx_riscv64)
endif()
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
# phase-369 W2 — cyclone-only setup, guarded so the same CMakeLists serves both
# RMWs. The typesupport module below hard-requires `CycloneDDS::ddsc`, so an
# unguarded include fails the zenoh configure outright.
if(NROS_RMW STREQUAL "cyclonedds")
    set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
        "${NANO_ROS_ROOT}/scripts/cyclonedds/msg_to_cyclone_idl.py"
        CACHE FILEPATH "nano-ros Cyclone DDS .msg/.srv to IDL converter" FORCE)
endif()
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)
if(NROS_RMW STREQUAL "cyclonedds")
    include("${NANO_ROS_ROOT}/packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake")
endif()

# Action role — resolve example_interfaces (Fibonacci.action) + its action_msgs /
# unique_identifier_msgs / builtin_interfaces closure from package.xml, matching
# the c/action-server cyclone setup (nros_find_interfaces reads <depend>).
nros_find_interfaces(LANGUAGE C SKIP_INSTALL)
# Phase 212.M.10 — `nros.toml` retired; the Config literal lives in
# `src/lib.rs::start_from_reset()`. The Cyclone path here doesn't
# reference `NROS_APP_CONFIG`, so the generated app_config.h was dead
# weight + dropped along with the source-of-truth toml.

# Issue #205 step 3 — the corrosion import, generated link-anchor TU,
# executable, and platform/RMW link calls collapse into the board-overlay
# seam (cmake/board/nano-ros-board-riscv64-qemu.cmake). The Rust crate's
# `app_main!(register)` (board macro) provides `app_main`.
nros_threadx_rv64_rust_app(riscv64_threadx_rust_action_server
    CRATE qemu-riscv64-threadx-action-server
    LINK  example_interfaces__nano_ros_c)
