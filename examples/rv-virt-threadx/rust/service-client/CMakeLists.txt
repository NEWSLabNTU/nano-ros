cmake_minimum_required(VERSION 3.22)
project(riscv64_threadx_rust_service_client VERSION 0.1.0 LANGUAGES C CXX ASM)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Phase 175.B — ThreadX Rust CycloneDDS uses the same C startup path as
# the C/C++ examples. CMake owns Cyclone's C++ backend + generated
# descriptors; the Rust crate is imported as a staticlib exposing
# app_main().
# issue 0835 — the caller decides, and this is only the copy-out default.
#
# This line used to read `set(NROS_PLATFORM threadx)`, an unconditional
# normal variable that SHADOWED the `-DNROS_PLATFORM=threadx_riscv64` this
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
if(NOT DEFINED NROS_PLATFORM)
    set(NROS_PLATFORM threadx_riscv64)
endif()
set(NROS_RMW "cyclonedds" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
# Resolve the nano-ros SDK root — RFC-0103 D6 (amended 2026-10-12). This
# block is the ONE leaf spelling, byte-identical in every leaf that must know
# the root before it can include anything (a copy-out leaf has no repo file to
# include yet); `check-repo-dir-readers` refuses any `$ENV{NROS_REPO_DIR}`.
#   1. explicit: -DNROS_REPO_DIR=<path> or -Dnano_ros_ROOT=<path>;
#   2. ask `nros sdk-root`. It walks up from THIS leaf first, so an in-tree
#      leaf answers its own checkout; anywhere else the binary answers with
#      its own SDK root (a release's share/nano-ros, or the checkout an in-tree
#      `nros` was built in).
# Never the environment: an exported path is what a linked worktree inherits
# from its parent (issue 1280).
if(NOT NROS_REPO_DIR)
    if(nano_ros_ROOT)
        set(NROS_REPO_DIR "${nano_ros_ROOT}")
    else()
        # host-tool-path-ok: no SDK root was given, so this is a user project; its installed `nros` is on PATH.
        find_program(NROS_TOOLCHAIN_EXECUTABLE nros)
        if(NROS_TOOLCHAIN_EXECUTABLE)
            execute_process(
                COMMAND "${NROS_TOOLCHAIN_EXECUTABLE}" sdk-root
                WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
                OUTPUT_VARIABLE NROS_REPO_DIR
                OUTPUT_STRIP_TRAILING_WHITESPACE
                RESULT_VARIABLE _nros_sdk_root_rc)
            if(NOT _nros_sdk_root_rc EQUAL 0)
                # An `nros` too old to know the verb exits non-zero: no answer,
                # rather than clap's usage text as a path.
                set(NROS_REPO_DIR "")
            endif()
        endif()
    endif()
    if(NOT NROS_REPO_DIR)
        message(FATAL_ERROR
            "nano-ros: cannot locate the nano-ros SDK root.\n"
            "  Pass -DNROS_REPO_DIR=<path> (or -Dnano_ros_ROOT=<path>), or put\n"
            "  `nros` on PATH — `nros sdk-root` answers this.")
    endif()
endif()
# phase-369 W2 — cyclone-only setup, guarded so the same CMakeLists serves both
# RMWs. The typesupport module below hard-requires `CycloneDDS::ddsc`, so an
# unguarded include fails the zenoh configure outright.
if(NROS_RMW STREQUAL "cyclonedds")
    set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
        "${NROS_REPO_DIR}/scripts/cyclonedds/msg_to_cyclone_idl.py"
        CACHE FILEPATH "nano-ros Cyclone DDS .msg/.srv to IDL converter" FORCE)
endif()
# Issue #214 — a two-QEMU pair on one L2 link needs distinct wire identities.
# This leaf's is its system.toml `[image.*] net` (RFC-0103 D10, phase-497 W2),
# which the cmake-generated NROS_APP_CONFIG TU now carries.
add_subdirectory("${NROS_REPO_DIR}" nano_ros)
if(NROS_RMW STREQUAL "cyclonedds")
    include("${NROS_REPO_DIR}/packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake")
endif()

nros_generate_interfaces(example_interfaces "srv/AddTwoInts.srv" LANGUAGE C SKIP_INSTALL)
# Phase 212.M.10 — `nros.toml` retired; the Config literal lives in
# `src/lib.rs::start_from_reset()`. The Cyclone path here doesn't
# reference `NROS_APP_CONFIG`, so the generated app_config.h was dead
# weight + dropped along with the source-of-truth toml.

# Issue #205 step 3 — the corrosion import, generated link-anchor TU,
# executable, and platform/RMW link calls collapse into the board-overlay
# seam (cmake/board/nano-ros-board-rv-virt-threadx.cmake). The Rust crate's
# `app_main!(register)` (board macro) provides `app_main`.
nros_threadx_rv64_rust_app(riscv64_threadx_rust_service_client
    CRATE rv-virt-threadx-service-client
    LINK  example_interfaces__nano_ros_c)
