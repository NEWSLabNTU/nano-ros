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
        OR NANO_ROS_RMW STREQUAL "dds" OR NANO_ROS_RMW STREQUAL "cyclonedds"
        OR NANO_ROS_RMW STREQUAL "uorb"))
    message(FATAL_ERROR
        "Unknown NANO_ROS_RMW: ${NANO_ROS_RMW} "
        "(expected: zenoh, xrce, dds, cyclonedds, or uorb)")
endif()
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
elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a")
    # NuttX provides a POSIX-like libc, so std is available.
    # NOTE: CMake/Corrosion builds for NuttX are disabled — upstream libc
    # is missing _SC_HOST_NAME_MAX which blocks build-std. Use Rust examples
    # directly (just nuttx build) until this is upstreamed.
    set(_platform_features std platform-nuttx)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_linux")
    # ThreadX Linux sim: host compiler, std available, ThreadX + NetX Duo via system libs.
    set(_platform_features std platform-threadx)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
    # ThreadX on RISC-V 64 (QEMU virt): no std, alloc for internal collections.
    set(_platform_features alloc panic-halt platform-threadx)
else()
    message(FATAL_ERROR "Unknown NANO_ROS_PLATFORM: ${NANO_ROS_PLATFORM} "
            "(expected: posix, freertos_armcm3, nuttx_armv7a, or threadx_linux)")
endif()

set(_features ros-humble ${_rmw_features} ${_platform_features})

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
        "${_nros_c_intree_include_dir}"
        "${CMAKE_CURRENT_SOURCE_DIR}/include"
        "${CMAKE_CURRENT_SOURCE_DIR}/../nros-platform-cffi/include"
    )
endif()

# ---- Install library ----
# Phase 128.C.4 — the Cargo build is RMW-agnostic (only `rmw-cffi`
# is enabled), so the install lib name no longer carries the RMW
# suffix. Single canonical `libnros_c.a` per (platform, ROS edition).
# Backend selection happens at the consumer's link step via
# `target_link_libraries(... NanoRos::Rmw::<name>)` (see
# `NanoRosLink.cmake`).
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_lib_name "libnros_c.a")
else()
    set(_lib_name "libnros_c_${NANO_ROS_PLATFORM}.a")
endif()

install(FILES "$<TARGET_FILE:nros_c-static>"
    DESTINATION lib
    RENAME "${_lib_name}"
)

# Strip soft-float compiler_builtins from RISC-V archive after installing.
# See: https://github.com/rust-lang/rust/issues/83229
if(NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
    execute_process(COMMAND rustc --print sysroot
        OUTPUT_VARIABLE _RUST_SYSROOT OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
    find_program(_LLVM_AR llvm-ar
        PATHS "${_RUST_SYSROOT}/lib/rustlib/x86_64-unknown-linux-gnu/bin" NO_DEFAULT_PATH)
    if(_LLVM_AR)
        set(_strip_script "${PROJECT_SOURCE_DIR}/cmake/strip-compiler-builtins.sh")
        install(CODE "execute_process(COMMAND bash \"${_strip_script}\" \"${_LLVM_AR}\" \"\${CMAKE_INSTALL_PREFIX}/lib/${_lib_name}\")")
    endif()
endif()

set(_PROJECT_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../..")

# ---- Headers ----
# Per-example `nros/app_config.h` lives in the build dir (emitted by
# `nano_ros_generate_config_header()`); the Zephyr Kconfig synthesis
# variant ships under `include/nros/zephyr/app_config.h` and the
# Zephyr module adds that to the include path conditionally. Both
# stay out of the default `<prefix>/include/nros/` namespace so the
# install fallback can't shadow per-example codegen on toolchains
# where the install include path appears before the build dir
# (NuttX cc-rs).
install(DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}/include/nros"
    DESTINATION include
    FILES_MATCHING PATTERN "*.h"
)
install(DIRECTORY "${_PROJECT_ROOT}/packages/core/nros-platform-cffi/include/nros"
    DESTINATION include
    FILES_MATCHING PATTERN "*.h"
)

# Phase 119.2: install variant-specific generated header so each cmake
# build's exact storage sizes are preserved. nros-c's build.rs writes
# the per-build header to `${CORROSION_BUILD_DIR}/nros_config_generated.h`
# (== this CMakeLists.txt's CMAKE_CURRENT_BINARY_DIR — corrosion sets
# that env var to point here).
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_c_variant_subdir "nros_c")
else()
    set(_c_variant_subdir "nros_c_${NANO_ROS_PLATFORM}")
endif()
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/nros_config_generated.h"
    DESTINATION "include/${_c_variant_subdir}/nros"
    OPTIONAL
)

# ---- CMake config-mode package ----
install(FILES
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosConfig.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosCTargets.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosReadConfig.cmake"
    # Phase 123.A.6: user-facing platform / RMW link helpers.
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosLink.cmake"
    # Phase 128.C.4: per-backend NanoRos::Rmw::<name> interface targets.
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosRmwInterfaces.cmake"
    # Phase 91.E1a: per-RTOS cmake support modules. Per-platform example
    # support files (examples/<plat>/cmake/<plat>-support.cmake) reach
    # these via include(nros-threadx) etc. once `find_package(NanoRos)`
    # has put this directory on cmake's module path.
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/nros-rtos-helpers.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/nros-threadx.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/nros-freertos.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/nros-nuttx.cmake"
    # Phase 112.E: layer-3 support cmake — examples include via
    # `include(<plat>-support)` after `find_package(NanoRos)`.
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/freertos-support.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/nuttx-support.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/threadx-support.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/threadx-riscv64-support.cmake"
    DESTINATION lib/cmake/NanoRos
)

# Phase 112.E: platform assets for layer-3 support cmake. Installed
# under share/nano_ros/ — paths match those resolved by the *-support
# files above.

# FreeRTOS — board config + lan9118 driver + per-example startup.c
install(DIRECTORY
    "${_PROJECT_ROOT}/packages/boards/nros-board-mps2-an385-freertos/config/"
    DESTINATION share/nano_ros/boards/mps2-an385-freertos/config)
install(DIRECTORY
    "${_PROJECT_ROOT}/packages/drivers/lan9118-lwip/include"
    "${_PROJECT_ROOT}/packages/drivers/lan9118-lwip/src"
    DESTINATION share/nano_ros/drivers/lan9118-lwip)
install(FILES
    "${_PROJECT_ROOT}/packages/boards/nros-board-mps2-an385-freertos/startup.c"
    "${_PROJECT_ROOT}/packages/core/nros-platform-freertos/src/net.c"
    DESTINATION share/nano_ros/platform/freertos)

# ThreadX-Linux — startup.c
install(FILES
    "${_PROJECT_ROOT}/examples/threadx-linux/cmake/startup.c"
    DESTINATION share/nano_ros/platform/threadx-linux)

# ThreadX-RISCV64 — startup.c + cxx-compat headers
install(FILES
    "${_PROJECT_ROOT}/examples/qemu-riscv64-threadx/cmake/startup.c"
    DESTINATION share/nano_ros/platform/threadx-riscv64)
install(DIRECTORY
    "${_PROJECT_ROOT}/examples/qemu-riscv64-threadx/cmake/cxx-compat"
    DESTINATION share/nano_ros/platform/threadx-riscv64)

# NuttX — FFI crate is NOT installed under share/. The crate's
# `Cargo.toml` carries path deps on workspace board crates
# (`../../../../packages/boards/nros-board-nuttx-qemu-arm` etc.) that
# would dangle once relocated to <prefix>/share/nano_ros/. NuttX
# self-containment requires a workspace-flattening installer (out of
# scope for Phase 112.E). `nuttx-support.cmake` resolves the crate
# via `NUTTX_FFI_CRATE_DIR` (env or -D) — examples set it to the
# in-tree path for now.

# Toolchain files — out-of-tree examples reference these via
# -DCMAKE_TOOLCHAIN_FILE=<prefix>/share/nano_ros/toolchains/<plat>.cmake.
install(FILES
    "${_PROJECT_ROOT}/cmake/toolchain/arm-freertos-armcm3.cmake"
    "${_PROJECT_ROOT}/cmake/toolchain/armv7a-nuttx-eabi.cmake"
    "${_PROJECT_ROOT}/cmake/toolchain/riscv64-threadx.cmake"
    DESTINATION share/nano_ros/toolchains)
install(PROGRAMS
    "${_PROJECT_ROOT}/cmake/toolchain/riscv64-lld-wrapper.sh"
    DESTINATION share/nano_ros/toolchains)

# Helper script that nros_threadx_strip_builtins() invokes (RISC-V
# soft-float / hard-float compiler_builtins workaround). Ships next to
# the cmake modules so the function can resolve it via
# CMAKE_CURRENT_LIST_DIR at install time.
install(PROGRAMS
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/strip-compiler-builtins.sh"
    DESTINATION lib/cmake/NanoRos
)

# Phase 112.D: typed app-config header template consumed by
# nano_ros_generate_config_header(). Ships into share/nano_ros/templates
# so the install-time path-search in NanoRosReadConfig.cmake finds it.
install(FILES
    "${CMAKE_CURRENT_SOURCE_DIR}/../../../cmake/templates/nros_app_config.h.in"
    DESTINATION share/nano_ros/templates
)
