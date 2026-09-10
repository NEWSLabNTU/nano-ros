cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_rust_listener VERSION 0.1.0 LANGUAGES C CXX ASM)

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
#   platform=threadx         |rmw=zenoh     |board=rv-virt-threadx|…   <- these six
#   platform=threadx_riscv64 |rmw=zenoh     |board=rv-virt-threadx|…   <- the c/cpp leaves
#   platform=threadx         |rmw=cyclonedds|board=rv-virt-threadx|…
#   platform=threadx_riscv64 |rmw=cyclonedds|board=rv-virt-threadx|…
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
set(NANO_ROS_BOARD rv-virt-threadx)
# Resolve the nano-ros SDK root once (phase-277 W6 standard guard; 4th rung
# phase-447 A2 / RFC-0099 D3):
#   1. -DNANO_ROS_ROOT=<path> cache var (explicit, e.g. a copied-out example),
#   2. NROS_REPO_DIR env var (exported by nano-ros's activate.sh),
#   3. in-repo relative walk-up (this example inside the nano-ros tree),
#   4. ask the installed toolchain: `nros sdk-root`. A release carries its own
#      SDK root at <prefix>/share/nano-ros, which is the only rung a copied-out
#      project on a machine with no checkout can reach — before it this guard
#      resolved three ways and all three were a checkout.
# LAST is a decision, not an ordering detail: a store rung placed earlier would
# answer a contributor with a store copy of the tree they are editing, which is
# the ownership guard's own failure mode (phase-431 W1). And the answer is ASKED
# for rather than baked in — a store moves, `nros pin` swaps a toolchain under a
# project, and a project is committed and then built on another machine.
if(NOT DEFINED NANO_ROS_ROOT)
    set(_nros_walk_up "${CMAKE_CURRENT_SOURCE_DIR}/../../../..")
    if(DEFINED ENV{NROS_REPO_DIR} AND NOT "$ENV{NROS_REPO_DIR}" STREQUAL "")
        set(NANO_ROS_ROOT "$ENV{NROS_REPO_DIR}")
    elseif(EXISTS "${_nros_walk_up}/packages/core/nros-core/Cargo.toml")
        # The marker `nros_launcher::checkout::MONOREPO_MARKER` names, so "is
        # this a nano-ros root" has one answer whichever language asks. The walk
        # used to be UNCONDITIONAL, which gave a copied-out project a confident
        # wrong path that failed one line later at the include — and left rung 4
        # unreachable, since rung 3 never declined.
        get_filename_component(NANO_ROS_ROOT "${_nros_walk_up}" ABSOLUTE)
    else()
        find_program(NROS_TOOLCHAIN_EXECUTABLE nros)
        if(NROS_TOOLCHAIN_EXECUTABLE)
            execute_process(
                COMMAND "${NROS_TOOLCHAIN_EXECUTABLE}" sdk-root
                WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
                OUTPUT_VARIABLE NANO_ROS_ROOT
                OUTPUT_STRIP_TRAILING_WHITESPACE
                RESULT_VARIABLE _nros_sdk_root_rc)
            if(NOT _nros_sdk_root_rc EQUAL 0)
                # An `nros` too old to know the verb exits non-zero. Treat that
                # as "no answer" rather than letting clap's usage text become a
                # path.
                set(NANO_ROS_ROOT "")
            endif()
        endif()
    endif()
    if(NOT NANO_ROS_ROOT)
        message(FATAL_ERROR
            "nano-ros: cannot locate the nano-ros SDK root.\n"
            "  Pass -DNANO_ROS_ROOT=<path>, or export NROS_REPO_DIR, or put an\n"
            "  installed toolchain's `nros` on PATH — `nros sdk-root` answers\n"
            "  this, and a released toolchain answers it out of itself.")
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
# Issue #214 — second-node wire identity: distinct trailing IP/MAC octets so a
# two-QEMU pair on one L2 link discovers (the first-node role keeps the
# .40/:56 defaults). Feeds the cmake-generated NROS_APP_CONFIG TU that
# startup.c applies before the kernel — must be set BEFORE add_subdirectory.
set(NROS_APP_NET_IP_LAST 41 CACHE STRING "" FORCE)
set(NROS_APP_NET_MAC_LAST "0x57" CACHE STRING "" FORCE)
add_subdirectory("${NANO_ROS_ROOT}" nano_ros)
if(NROS_RMW STREQUAL "cyclonedds")
    include("${NANO_ROS_ROOT}/packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake")
endif()

nros_generate_interfaces(std_msgs "msg/String.msg" LANGUAGE C SKIP_INSTALL)
# Phase 212.M.10 — `nros.toml` retired; the Config literal lives in
# `src/lib.rs::start_from_reset()`. The Cyclone path here doesn't
# reference `NROS_APP_CONFIG`, so the generated app_config.h was dead
# weight + dropped along with the source-of-truth toml.

# Issue #205 step 3 — the corrosion import, generated link-anchor TU,
# executable, and platform/RMW link calls collapse into the board-overlay
# seam (cmake/board/nano-ros-board-rv-virt-threadx.cmake). The Rust crate's
# `app_main!(register)` (board macro) provides `app_main`.
nros_threadx_rv64_rust_app(riscv64_threadx_rust_listener
    CRATE rv-virt-threadx-listener
    LINK  std_msgs__nano_ros_c)
