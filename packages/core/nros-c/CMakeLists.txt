# packages/core/nros-c/CMakeLists.txt
#
# Builds the nros-c static library via Corrosion (Cargo integration).
# Installs library (RMW + platform suffixed), headers, and CMake config package.
#
# This file is included from the top-level CMakeLists.txt via add_subdirectory().
# Two variables control the build:
#   NANO_ROS_RMW      — RMW backend: zenoh, xrce, or dds (default: zenoh)
#   NANO_ROS_PLATFORM — target platform: posix, freertos_armcm3, nuttx_armv7a
#                       (default: posix)

# ---- Platform (default: posix) ----
if(NOT DEFINED NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM "posix")
endif()

# Phase 123.A.11 — nros-c is RMW-agnostic at the Cargo level. The
# `cffi-{xrce,dds,zenoh}-*` features still exist as no-op aliases
# of `rmw-cffi` (Cargo backwards compat) but choosing a backend
# happens at LINK time via `find_package(NrosRmw<X>)` +
# `nano_ros_link_rmw`. Each backend's `.init_array` ctor (phase
# 104.A) registers it before `main()`; nros-c itself stays
# unchanged across all RMW selections.
#
# The `NANO_ROS_RMW` value still drives downstream linkage
# (NanoRosCTargets.cmake pulls the right standalone archive) +
# the install lib name suffix, but feeds NOTHING extra into
# nros-c's Cargo build. Result: one `libnros_c.a` per target
# triple.
if(NOT (NANO_ROS_RMW STREQUAL "zenoh" OR NANO_ROS_RMW STREQUAL "xrce"
        OR NANO_ROS_RMW STREQUAL "cyclonedds"
        OR NANO_ROS_RMW STREQUAL "uorb"
        OR NANO_ROS_RMW STREQUAL "none"))
    message(FATAL_ERROR
        "Unknown NANO_ROS_RMW: ${NANO_ROS_RMW} "
        "(expected: zenoh, xrce, cyclonedds, uorb, or none)")
endif()
# Phase 156 — `NANO_ROS_RMW=none` = bridge / multi-backend
# escape hatch. Root CMake skips RMW auto-pull; consumer pulls
# its own staticlibs + calls explicit `nros_rmw_<x>_register()`.
# nros-c still builds with `rmw-cffi` so umbrella Rust API
# surfaces compile; registry stays empty until explicit
# register() fires.
# Phase 241.D3-rev — single-runtime umbrella: bundle the selected Rust backend into
# libnros_c.a so it is the ONE Rust staticlib a C binary links. cyclonedds is a
# separate C++ lib (linked by the root), so the umbrella carries only the cffi shim.
if(NANO_ROS_RMW STREQUAL "zenoh")
    set(_rmw_features cffi-zenoh-cffi)
elseif(NANO_ROS_RMW STREQUAL "xrce")
    set(_rmw_features cffi-xrce-c)
else()
    set(_rmw_features rmw-cffi)
endif()

# ---- Map platform to Cargo features ----
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_platform_features std platform-posix)
elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3"
       OR NANO_ROS_PLATFORM STREQUAL "freertos")
    # FreeRTOS bare-metal: no std, alloc for internal Rust collections,
    # panic-halt for a minimal panic handler, link-tls/link-custom for zenoh-pico.
    # Phase 138 canonical "freertos" accepted alongside legacy
    # "freertos_armcm3" (Phase 138 collapsed board into NANO_ROS_BOARD).
    set(_platform_features alloc panic-halt platform-freertos)
elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a"
       OR NANO_ROS_PLATFORM STREQUAL "nuttx")
    # NuttX provides a POSIX-like libc, so std is available.
    # NOTE: CMake/Corrosion builds for NuttX are disabled — upstream libc
    # is missing _SC_HOST_NAME_MAX which blocks build-std. Use Rust examples
    # directly (just nuttx build) until this is upstreamed.
    # Phase 138 canonical "nuttx" accepted alongside legacy
    # "nuttx_armv7a" (Phase 138 collapsed board into NANO_ROS_BOARD).
    set(_platform_features std platform-nuttx)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_linux")
    # ThreadX Linux sim: host compiler, std available, ThreadX + NetX Duo via system libs.
    set(_platform_features std platform-threadx)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
    # ThreadX on RISC-V 64 (QEMU virt): no std, alloc for internal collections.
    set(_platform_features alloc panic-halt platform-threadx)
elseif(NANO_ROS_PLATFORM STREQUAL "esp_idf")
    # ESP-IDF: Espressif's FreeRTOS port (xtensa-esp* / riscv32-esp*).
    # IDF supplies its own libc + alloc, but nros-c stays no_std + alloc
    # to keep the Rust staticlib portable across xtensa / riscv32-esp.
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

set(_features ros-humble ${_rmw_features} ${_platform_features})

# Phase 241.D3-rev (W9) — enable the param / lifecycle service surfaces on the hosted
# umbrella so examples using them (the C++ executor headers call the gated C entry
# points) link. alloc-gated, so kept to hosted platforms; embedded stays opt-in.
if(NANO_ROS_PLATFORM STREQUAL "posix")
    list(APPEND _features param-services lifecycle-services)
endif()

# Phase 252 / issue 0073 — E2E message-integrity (CRC + seq gap/dup). Lowers a
# declared [safety] axis to the nros-c `safety-e2e` feature, which forwards to the
# zenoh backend's own safety-e2e (the CRC attach/validate) and surfaces
# nros_subscription_try_recv_validated + nros_integrity_status_t. zenoh-only (the
# only backend with the CRC path). Codegen sets NANO_ROS_SAFETY_E2E=ON for a
# `[safety]` system; default OFF keeps it a compile dimension.
option(NANO_ROS_SAFETY_E2E "Enable E2E message-integrity (CRC) — zenoh only" OFF)
if(NANO_ROS_SAFETY_E2E)
    if(NANO_ROS_RMW STREQUAL "zenoh")
        list(APPEND _features safety-e2e)
    else()
        message(WARNING
            "NANO_ROS_SAFETY_E2E=ON ignored: only the zenoh RMW carries the CRC "
            "path (got NANO_ROS_RMW=${NANO_ROS_RMW}).")
    endif()
endif()

# ---- Build nros-c via Corrosion ----
corrosion_import_crate(
    MANIFEST_PATH "${PROJECT_SOURCE_DIR}/Cargo.toml"
    CRATES        nros-c
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      ${_features}
    LOCKED
)

# ---- In-tree variant header (Phase 137) ----
# `nros-c`'s build.rs writes the per-build `nros_config_generated.h` to
# `${CORROSION_BUILD_DIR}/nros_config_generated.h` (== this file's
# `CMAKE_CURRENT_BINARY_DIR`). Consumers `#include <nros/nros_config_generated.h>`
# — i.e. through a `nros/` subdir. The install-time path materialises that
# layout under `<prefix>/include/nros_c/nros/`; for in-tree consumers
# (Phase 137 `add_subdirectory(...)`) we mirror the header into a sibling
# `include/nros/` dir and add it to `nros_c-static`'s INTERFACE so any
# target linking against `nros_c-static` (or the `NanoRos::NanoRos`
# umbrella declared at the root) gets the per-build sizes without an
# install step.
set(_nros_c_intree_include_dir "${CMAKE_CURRENT_BINARY_DIR}/include")
file(MAKE_DIRECTORY "${_nros_c_intree_include_dir}/nros")
# Issue 0088 — mirror as a first-class OUTPUT + custom target, NOT a POST_BUILD
# side effect. A `POST_BUILD` on `cargo-build_nros_c` with only `BYPRODUCTS`
# gives consumers no real Ninja edge to the mirrored file: `add_dependencies(
# <consumer> cargo-build_nros_c)` orders against the cargo *primary output*, so a
# consumer TU can compile before the POST_BUILD copy runs and pick up the in-tree
# stub header (intermittent by build order). An `OUTPUT` command + a custom target
# (`nros_c_config_header`) that consumers `add_dependencies` on is the proper
# producer→consumer edge.
set(_nros_c_gen_config_header "${_nros_c_intree_include_dir}/nros/nros_config_generated.h")
add_custom_command(
    OUTPUT "${_nros_c_gen_config_header}"
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        "${CMAKE_CURRENT_BINARY_DIR}/nros_config_generated.h"
        "${_nros_c_gen_config_header}"
    DEPENDS cargo-build_nros_c
    COMMENT "nros-c: mirroring nros_config_generated.h for in-tree consumers"
    VERBATIM
)
add_custom_target(nros_c_config_header DEPENDS "${_nros_c_gen_config_header}")
# Issue 0090 — export the absolute mirror-header path so NanoRosNodeRegister can set
# a FILE-level OBJECT_DEPENDS on it. The threadx carrier's link-time
# `add_dependencies(<carrier> nros_c_config_header)` does not stop a TU from
# compiling before the per-build header is mirrored (it then reads the in-tree stub
# → `*_OPAQUE_U64S undeclared`); a per-source OBJECT_DEPENDS is the hard edge.
set_property(GLOBAL PROPERTY NROS_C_CONFIG_HEADER_FILE "${_nros_c_gen_config_header}")
if(TARGET nros_c-static)
    target_include_directories(nros_c-static INTERFACE
        "${CMAKE_CURRENT_SOURCE_DIR}/../nros-platform-api/include"
        "${_nros_c_intree_include_dir}"
        "${CMAKE_CURRENT_SOURCE_DIR}/include"
    )
    # Issue 0123 — the mirror dir above is on nros_c-static's INTERFACE ahead of the
    # in-tree `#error` stub, but the `nros_c_config_header` mirror target is only built
    # if some target depends on it. The message-lib / entry / carrier paths wire that
    # dep explicitly (issues 0114/0090); a plain rclcpp/ament consumer that pulls
    # nros-c only transitively (e.g. the workspace-shadowing `consumer` via the
    # std_msgs binding) never does, so the mirror never runs during `all` → its dir
    # stays empty → the consumer reads the stub. Make nros_c-static ITSELF depend on
    # the mirror so ANY consumer that links it (directly or transitively) builds the
    # per-build header before compiling. (Verified: manually building the mirror then
    # the shadowing consumer compiles clean.)
    if(TARGET nros_c_config_header)
        add_dependencies(nros_c-static nros_c_config_header)
    endif()
endif()

# Phase 140 — all install(...) rules deleted. nros-c is consumed
# in-tree via `add_subdirectory(<repo-root>)`; the root CMake's
# `NanoRos::NanoRos` INTERFACE target wires up the build-dir variant
# header path (`target_include_directories` above) directly.
