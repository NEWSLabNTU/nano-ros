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

# ---- Map RMW to Cargo features ----
if(NANO_ROS_RMW STREQUAL "zenoh")
    set(_rmw_features rmw-zenoh)
elseif(NANO_ROS_RMW STREQUAL "xrce")
    set(_rmw_features rmw-xrce xrce-udp)
elseif(NANO_ROS_RMW STREQUAL "dds")
    set(_rmw_features rmw-dds)
else()
    message(FATAL_ERROR "Unknown NANO_ROS_RMW: ${NANO_ROS_RMW} (expected: zenoh, xrce, or dds)")
endif()

# ---- Map platform to Cargo features ----
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_platform_features std platform-posix)
elseif(NANO_ROS_PLATFORM STREQUAL "freertos_armcm3")
    # FreeRTOS bare-metal: no std, alloc for internal Rust collections,
    # panic-halt for a minimal panic handler, link-tcp/link-udp for zenoh-pico.
    set(_platform_features alloc panic-halt platform-freertos link-tcp link-udp-unicast)
elseif(NANO_ROS_PLATFORM STREQUAL "nuttx_armv7a")
    # NuttX provides a POSIX-like libc, so std is available.
    # NOTE: CMake/Corrosion builds for NuttX are disabled — upstream libc
    # is missing _SC_HOST_NAME_MAX which blocks build-std. Use Rust examples
    # directly (just nuttx build) until this is upstreamed.
    set(_platform_features std platform-nuttx link-tcp link-udp-unicast)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_linux")
    # ThreadX Linux sim: host compiler, std available, ThreadX + NetX Duo via system libs.
    set(_platform_features std platform-threadx link-tcp link-udp-unicast)
elseif(NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
    # ThreadX on RISC-V 64 (QEMU virt): no std, alloc for internal collections.
    set(_platform_features alloc panic-halt platform-threadx link-tcp link-udp-unicast)
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

# ---- Install library ----
# posix uses the legacy unsuffixed name (libnros_c_<rmw>.a) for backwards
# compatibility.  All other platforms use a suffixed name so multiple
# variants coexist in the same prefix.
if(NANO_ROS_PLATFORM STREQUAL "posix")
    set(_lib_name "libnros_c_${NANO_ROS_RMW}.a")
else()
    set(_lib_name "libnros_c_${NANO_ROS_RMW}_${NANO_ROS_PLATFORM}.a")
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

# ---- CMake config-mode package ----
install(FILES
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosConfig.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosCTargets.cmake"
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/NanoRosReadConfig.cmake"
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
set(_PROJECT_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../..")

# FreeRTOS — board config + lan9118 driver + per-example startup.c
install(DIRECTORY
    "${_PROJECT_ROOT}/packages/boards/nros-board-mps2-an385-freertos/config/"
    DESTINATION share/nano_ros/boards/mps2-an385-freertos/config)
install(DIRECTORY
    "${_PROJECT_ROOT}/packages/drivers/lan9118-lwip/include"
    "${_PROJECT_ROOT}/packages/drivers/lan9118-lwip/src"
    DESTINATION share/nano_ros/drivers/lan9118-lwip)
install(FILES
    "${_PROJECT_ROOT}/examples/qemu-arm-freertos/cmake/startup.c"
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
