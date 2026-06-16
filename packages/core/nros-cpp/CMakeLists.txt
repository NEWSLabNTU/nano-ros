# packages/core/nros-cpp/CMakeLists.txt
#
# nros C++ API: header-only C++14 library + Rust FFI staticlib.
# Provides the NanoRos::NanoRosCpp target.
#
# This file is included from the top-level CMakeLists.txt via add_subdirectory().
# Two variables control the build:
#   NANO_ROS_RMW      — RMW backend: zenoh, xrce, or dds (default: zenoh)
#   NANO_ROS_PLATFORM — target platform: posix, freertos_armcm3, nuttx_armv7a
#                       (default: posix)

# ---- Platform (default: posix; may already be set by nros-c) ----
if(NOT DEFINED NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM "posix")
endif()

# ---- Map RMW to Cargo features ----
# Phase 128.C.3 — every backend now routes through the same `rmw-cffi`
# Rust feature; the per-backend `rmw-*-cffi` features were deleted.
# Selection happens at link time via `target_link_libraries(...
# NanoRos::Rmw::<name>)`; this block only validates `NANO_ROS_RMW`
# spelling so configure errors out early when the user picks an
# unknown name.
if(NOT (NANO_ROS_RMW STREQUAL "zenoh" OR NANO_ROS_RMW STREQUAL "xrce"
        OR NANO_ROS_RMW STREQUAL "cyclonedds"
        OR NANO_ROS_RMW STREQUAL "uorb"
        OR NANO_ROS_RMW STREQUAL "none"))
    message(FATAL_ERROR
        "Unknown NANO_ROS_RMW: ${NANO_ROS_RMW} "
        "(expected: zenoh, xrce, cyclonedds, uorb, or none)")
endif()
# Phase 241.D3-rev — single-runtime umbrella: bundle the selected Rust backend into
# libnros_cpp.a (the ONE Rust staticlib a C++ binary links). cyclonedds is a separate
# C++ lib (linked by the root), so the umbrella carries only the cffi shim.
if(NANO_ROS_RMW STREQUAL "zenoh")
    set(_rmw_features rmw-zenoh-cffi)
elseif(NANO_ROS_RMW STREQUAL "xrce")
    set(_rmw_features rmw-xrce-cffi)
else()
    set(_rmw_features rmw-cffi)
endif()

# ---- Map platform to Cargo features ----
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_platform_features std platform-posix)
elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3"
       OR NANO_ROS_PLATFORM STREQUAL "freertos")
    # Phase 138 canonical "freertos" accepted alongside legacy
    # "freertos_armcm3"; Phase 138 collapsed board into NANO_ROS_BOARD.
    set(_platform_features alloc panic-halt platform-freertos)
elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a"
       OR NANO_ROS_PLATFORM STREQUAL "nuttx")
    # Phase 138 canonical "nuttx" accepted alongside legacy
    # "nuttx_armv7a"; Phase 138 collapsed board into NANO_ROS_BOARD.
    set(_platform_features std platform-nuttx)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_linux")
    set(_platform_features std platform-threadx)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
    set(_platform_features alloc panic-halt platform-threadx)
elseif(NANO_ROS_PLATFORM STREQUAL "esp_idf")
    # ESP-IDF: Espressif's FreeRTOS port. Same feature set as the
    # canonical no_std FreeRTOS build (nros-c CMakeLists mirrors this).
    set(_platform_features alloc panic-halt platform-freertos)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx")
    # Phase 138 canonical "threadx" — board axis disambiguates the
    # std-vs-no_std split (Linux sim has a host libc; RV64 QEMU is
    # bare-metal). Required for `add_subdirectory(<nano-ros-root>)`
    # consumers per Phase 144.7-8.
    if(NANO_ROS_BOARD STREQUAL "threadx-linux")
        set(_platform_features std platform-threadx)
    elseif(NANO_ROS_BOARD STREQUAL "riscv64-qemu")
        set(_platform_features alloc panic-halt platform-threadx)
    else()
        message(FATAL_ERROR
            "NANO_ROS_PLATFORM=threadx requires NANO_ROS_BOARD to be one of: "
            "threadx-linux, riscv64-qemu. Got '${NANO_ROS_BOARD}'.")
    endif()
else()
    message(FATAL_ERROR "Unknown NANO_ROS_PLATFORM: ${NANO_ROS_PLATFORM} "
            "(expected: posix, freertos_armcm3, freertos, nuttx_armv7a, "
            "threadx_linux, threadx_riscv64, threadx, or esp_idf)")
endif()

set(_cpp_features ros-humble ${_rmw_features} ${_platform_features})

# Phase 241.D3-rev (W9) — enable param / lifecycle on the hosted C++ umbrella (the
# executor headers expose them; the native `parameters` example uses them). Forwards
# to nros-c. alloc-gated → hosted only; embedded opt-in.
if(NANO_ROS_PLATFORM STREQUAL "posix")
    list(APPEND _cpp_features param-services lifecycle-services)
endif()

# Phase 252 / issue 0073 — E2E message-integrity (CRC) for the C++ API. Mirrors
# nros-c's NANO_ROS_SAFETY_E2E: zenoh-only (the CRC path); surfaces
# nros_cpp_subscription_try_recv_validated. Codegen sets it for a `[safety]` system.
option(NANO_ROS_SAFETY_E2E "Enable E2E message-integrity (CRC) — zenoh only" OFF)
if(NANO_ROS_SAFETY_E2E)
    if(NANO_ROS_RMW STREQUAL "zenoh")
        list(APPEND _cpp_features safety-e2e)
    else()
        message(WARNING
            "NANO_ROS_SAFETY_E2E=ON ignored: only the zenoh RMW carries the CRC "
            "path (got NANO_ROS_RMW=${NANO_ROS_RMW}).")
    endif()
endif()

# ---- Build nros-cpp via Corrosion ----
corrosion_import_crate(
    MANIFEST_PATH "${PROJECT_SOURCE_DIR}/Cargo.toml"
    CRATES        nros-cpp
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      ${_cpp_features}
    LOCKED
)

# ---- Header-only C++ interface library ----
add_library(nros-cpp-headers INTERFACE)
add_library(NanoRos::NanoRosCpp ALIAS nros-cpp-headers)

target_include_directories(nros-cpp-headers INTERFACE
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
    $<INSTALL_INTERFACE:include>
)

# Phase 144 — mirror the per-build `nros_cpp_config_generated.h` (produced
# by nros-cpp's build.rs into CMAKE_CURRENT_BINARY_DIR via corrosion) to
# a sibling `include/nros/` dir that we add to nros-cpp-headers' INTERFACE.
# Same pattern as the nros-c sub-project (see Phase 137 comment block in
# `packages/core/nros-c/CMakeLists.txt`). The mirrored copy is searched
# BEFORE the source-tree stub, so per-variant storage sizes take effect
# under add_subdirectory(<repo-root>) consumption.
set(_nros_cpp_intree_include_dir "${CMAKE_CURRENT_BINARY_DIR}/include")
file(MAKE_DIRECTORY "${_nros_cpp_intree_include_dir}/nros")
if(TARGET cargo-build_nros_cpp)
    add_custom_command(TARGET cargo-build_nros_cpp POST_BUILD
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "${CMAKE_CURRENT_BINARY_DIR}/nros_cpp_config_generated.h"
            "${_nros_cpp_intree_include_dir}/nros/nros_cpp_config_generated.h"
        # Phase 241.D3-rev — ALSO mirror the C variant header. The C++ umbrella now
        # bundles nros-c, so nros-cpp's own corrosion build emits the C
        # `nros_config_generated.h` (the *_OPAQUE_U64S sizes) into this binary dir.
        # A C++ TU `#include <nros/...>` needs it, but cpp_talker links
        # nros-cpp-headers (NOT nros_c-static), so it has no build-order dependency on
        # nros-c's own mirror and would race to nros-c's stub header. Mirroring it
        # here — under the same POST_BUILD on cargo-build_nros_cpp, into the dir added
        # `BEFORE INTERFACE` below — makes it both present and ordered ahead of the stub.
        COMMAND ${CMAKE_COMMAND} -E copy_if_different
            "${CMAKE_CURRENT_BINARY_DIR}/nros_config_generated.h"
            "${_nros_cpp_intree_include_dir}/nros/nros_config_generated.h"
        BYPRODUCTS
            "${_nros_cpp_intree_include_dir}/nros/nros_cpp_config_generated.h"
            "${_nros_cpp_intree_include_dir}/nros/nros_config_generated.h"
        COMMENT "nros-cpp: mirroring nros_{,cpp_}config_generated.h for in-tree consumers"
        VERBATIM
    )
    target_include_directories(nros-cpp-headers BEFORE INTERFACE
        $<BUILD_INTERFACE:${_nros_cpp_intree_include_dir}>)
endif()

# Phase 241.D3-rev — the C++ umbrella (bundles nros-c + the backend) is the ONLY
# Rust staticlib on a C++ link line ⇒ `std` once ⇒ no `--allow-multiple-definition`.
# Linked NORMALLY (not whole-archive): whole-archiving a Rust staticlib drags its
# compiler-builtins intrinsics into a collision with the system libgcc. The surface
# is pulled lazily; the backend registers via the NanoRosLink.cmake stub.
target_link_libraries(nros-cpp-headers INTERFACE nros_cpp-static)

# Phase 241.D3-rev — pull NanoRos's C-side HEADERS + platform-compile-def + platform
# shim, but NOT its `nros_c-static` ARCHIVE: a C++ binary links ONLY libnros_cpp.a
# (already bundling nros-c). Linking the C archive too would put a SECOND Rust
# staticlib (a second `std`) on the line — the exact duplicate this phase removes.
# Deferred to the root dir where NanoRos + NanoRos::Platform are declared (after this
# add_subdirectory). 192.3: root via NANO_ROS_ROOT_DIR; relative fallback standalone.
if(NOT DEFINED NANO_ROS_ROOT_DIR)
    set(NANO_ROS_ROOT_DIR "${CMAKE_CURRENT_LIST_DIR}/../../..")
endif()
# C-side headers: pull from `nros_c-static`'s INTERFACE — it carries the public C
# headers AND the per-build variant header (`nros_config_generated.h`, the
# *_OPAQUE_U64S sizes). NanoRos's OWN INTERFACE_INCLUDE_DIRECTORIES does NOT contain
# them: NanoRos gets them transitively via its link to nros_c-static, and
# `$<TARGET_PROPERTY>` resolves only the named target's own property, not transitive
# usage requirements. Without the variant header the C++ TU reaches nros-c's stub
# `nros_config_generated.h` and fails (`*_OPAQUE_U64S not declared`). NuttX (no
# nros_c-static) falls back to NanoRos's manually-added include tree.
if(TARGET nros_c-static)
    cmake_language(DEFER DIRECTORY "${NANO_ROS_ROOT_DIR}" CALL
        target_include_directories nros-cpp-headers INTERFACE
        "$<TARGET_PROPERTY:nros_c-static,INTERFACE_INCLUDE_DIRECTORIES>")
else()
    cmake_language(DEFER DIRECTORY "${NANO_ROS_ROOT_DIR}" CALL
        target_include_directories nros-cpp-headers INTERFACE
        "$<TARGET_PROPERTY:NanoRos,INTERFACE_INCLUDE_DIRECTORIES>")
endif()
cmake_language(DEFER DIRECTORY "${NANO_ROS_ROOT_DIR}" CALL
    target_compile_definitions nros-cpp-headers INTERFACE
    "$<TARGET_PROPERTY:NanoRos,INTERFACE_COMPILE_DEFINITIONS>")
cmake_language(DEFER DIRECTORY "${NANO_ROS_ROOT_DIR}" CALL
    target_link_libraries nros-cpp-headers INTERFACE NanoRos::Platform)

# Phase 117.8: when the consumer asks for the Cyclone DDS RMW backend,
# pull in the standalone `nros-rmw-cyclonedds` library + Cyclone DDS
# itself transitively, and flip on the auto-register call inside
# `nros::init` via the `NROS_RMW_CYCLONEDDS` macro.
if(NANO_ROS_RMW STREQUAL "cyclonedds")
    # The root NanoRos umbrella owns Cyclone backend linkage. Linking
    # nros_rmw_cyclonedds directly here places the archive before the
    # umbrella's whole-archive group on CMake 3.22 and lets ld discard
    # the registration constructor object, leaving the runtime with an
    # empty backend registry.
    target_compile_definitions(nros-cpp-headers INTERFACE NROS_RMW_CYCLONEDDS=1)
elseif(NANO_ROS_RMW STREQUAL "xrce")
    # Phase 144.3 — XRCE via the `nros-rmw-xrce-cffi` Rust shim, which
    # is built+linked by the root `CMakeLists.txt` xrce branch (see
    # add_subdirectory path). Mirrors the dds/zenoh shape above: no
    # find_package required — the `nros_rmw_xrce_register()` symbol
    # lives in the staticlib and auto-registers via the `linkme`
    # distributed-slice (Phase 128.B.3). The compile define is kept
    # under the `_CFFI` suffix to distinguish from the legacy
    # `NROS_RMW_XRCE` define still emitted by `NanoRosCppTargets.cmake`
    # for installed find_package consumers.
    target_compile_definitions(nros-cpp-headers INTERFACE NROS_RMW_XRCE_CFFI=1)
elseif(NANO_ROS_RMW STREQUAL "zenoh")
    # Phase 115.L.2: zenoh-pico via the C vtable. Same shape as
    # the `dds` path above — `nros_rmw_zenoh_register()` lives
    # in the Rust shim.
    target_compile_definitions(nros-cpp-headers INTERFACE NROS_RMW_ZENOH_CFFI=1)
elseif(NANO_ROS_RMW STREQUAL "uorb")
    # Phase 140 — PX4 uORB backend (C++ port). Consumed via the
    # `integrations/px4/` shell which add_subdirectory's
    # `packages/px4/nros-rmw-uorb` and exposes the `nros_rmw_uorb`
    # target. Mirrors the cyclonedds shape above.
    if(TARGET nros_rmw_uorb)
        target_link_libraries(nros-cpp-headers INTERFACE nros_rmw_uorb)
    endif()
    target_compile_definitions(nros-cpp-headers INTERFACE NROS_RMW_UORB=1)
endif()

# Require at least C++14
target_compile_features(nros-cpp-headers INTERFACE cxx_std_14)

# Phase 140 — install rules deleted. nros-cpp is consumed in-tree
# via `add_subdirectory(<repo-root>)`; the build-dir variant header
# path is wired via `target_include_directories(nros-cpp-headers
# BEFORE INTERFACE …)` above. NanoRos::NanoRosCpp resolves directly
# against the in-tree `nros-cpp-headers` INTERFACE target.
