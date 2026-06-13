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
if(NANO_ROS_RMW STREQUAL "zenoh")
    set(_rmw_features cffi-zenoh-cffi)
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

# RFC-0042 D3 / phase-241.D slice 4 — in the multi-archive C/C++ link the root
# CMakeLists sets NROS_CFFI_EXTERNAL_REGISTRY; reference the cffi `REGISTRY` as an
# undefined external (the provider archive defines it once) so `libnros_c.a` emits
# no duplicate def. Inherited from the parent scope; unset/off on NuttX.
if(NROS_CFFI_EXTERNAL_REGISTRY)
    list(APPEND _features external-registry)
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
add_custom_command(TARGET cargo-build_nros_c POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        "${CMAKE_CURRENT_BINARY_DIR}/nros_config_generated.h"
        "${_nros_c_intree_include_dir}/nros/nros_config_generated.h"
    BYPRODUCTS "${_nros_c_intree_include_dir}/nros/nros_config_generated.h"
    COMMENT "nros-c: mirroring nros_config_generated.h for in-tree consumers"
    VERBATIM
)
if(TARGET nros_c-static)
    target_include_directories(nros_c-static INTERFACE
        "${CMAKE_CURRENT_SOURCE_DIR}/../nros-platform-api/include"
        "${_nros_c_intree_include_dir}"
        "${CMAKE_CURRENT_SOURCE_DIR}/include"
    )
endif()

# Phase 140 — all install(...) rules deleted. nros-c is consumed
# in-tree via `add_subdirectory(<repo-root>)`; the root CMake's
# `NanoRos::NanoRos` INTERFACE target wires up the build-dir variant
# header path (`target_include_directories` above) directly.
