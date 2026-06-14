# cmake/platform/nano-ros-nuttx.cmake
#
# Phase 138.2 / 144.6 — NuttX platform module. Single source of truth
# for NuttX platform-shim wiring under the Phase 137
# `add_subdirectory(<nano-ros-root>)` consumption shape.
#
# Unlike FreeRTOS / ThreadX, NuttX uses its own native build system
# (kconfig + make). cmake's job is **not** to rebuild the kernel —
# `nros_nuttx_build_example(...)` (in the layer-2 `nros-nuttx.cmake`
# helper) drives `cargo build` on a delegating FFI crate
# (`nros-nuttx-ffi`) whose build.rs invokes the NuttX toolchain on
# the user's main.c/main.cpp + codegen-generated sources, and links
# against the pre-built NuttX libraries via NUTTX_DIR / NUTTX_APPS_DIR.
#
# What this module composes:
#
#   * Re-exports the layer-2 helper functions (`nros_nuttx_validate`,
#     `nros_nuttx_set_cargo_target`, `nros_nuttx_build_example`) and
#     the layer-3 `nuttx_build_example(...)` backward-compat wrapper.
#     Implementation lives under `packages/core/nros-c/cmake/`; this
#     module just include()s so per-board overlays + per-example
#     CMakeLists.txt see the same function names regardless of
#     consumption shape.
#
#     without an install step (Phase 140 removed the legacy install path). The
#     latter is a no-op on NuttX (the FFI crate's Cargo.toml pulls
#     the RMW staticlib in directly) — keeping the function defined
#     lets examples call it without a per-platform `if`.
#
#   * Builds the native-C platform shim
#     `packages/core/nros-platform-nuttx/` (compiles the POSIX
#     `platform.c` / `net.c` / `timer.c` translation units — NuttX's
#     libc supplies pthread/clock/sched_yield natively). The shim
#     implements the canonical `nros_platform_*` ABI for NuttX and
#     is what `NanoRos::Platform` resolves to. The actual link of
#     this archive into the application binary happens inside cargo
#     via the FFI crate's transitive `nros-platform-nuttx` dep — the
#     CMake INTERFACE target exists for contract uniformity.
#
#   * Pulls in the per-board overlay
#     (`cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake`) — board
#     overlays own the NuttX FFI-crate path (`NUTTX_FFI_CRATE_DIR`),
#     the cargo target triple (e.g. `armv7a-nuttx-eabihf`), and
#     define `nros_board_link_app(target)` which redirects the
#     user's `add_executable(...)` target through
#     `nuttx_build_example(...)`.
#
#   * Defines `nros_platform_link_app(target)` — reads the user
#     target's SOURCES + LINK_LIBRARIES + INCLUDE_DIRECTORIES and
#     delegates to `nros_board_link_app(target)` (which calls
#     `nuttx_build_example(...)` under the hood).
#
# Contract (Phase 138 §A):
#   NanoRos::Platform                  — INTERFACE alias for the shim
#   nros_platform_nuttx_iface          — concrete INTERFACE behind it
#   nros_platform_link_app(<target>)   — per-app fixup
#   NROS_PLATFORM_LINK_FEATURES        — default link feature set

if(DEFINED _NROS_PLATFORM_NUTTX_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_NUTTX_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast
    CACHE STRING "Default link features for the NuttX platform")

# ---------------------------------------------------------------------------
# Layer-2 helpers (nros_nuttx_validate / nros_nuttx_set_cargo_target /
# nros_nuttx_build_example). Implementation lives under
# packages/core/nros-c/cmake/nros-nuttx.cmake.
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/nros-nuttx.cmake")

# ---------------------------------------------------------------------------
# User-facing nano-ros helpers (config + link).
#
# `nano_ros_link_rmw(<target> RMW <name>)` is a POSIX/FreeRTOS-style
# convenience that adds the per-RMW interface lib. On NuttX the FFI
# crate's `Cargo.toml` already declares `nros-rmw-zenoh` etc., so the
# RMW staticlib is dragged in by cargo, not by the `add_executable`
# target. Re-expose the helper anyway so per-example CMakeLists keep
# the same one-line shape across platforms — board overlay decides
# whether the call is a no-op (NuttX) or a real link (FreeRTOS).
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosLink.cmake")

# ---------------------------------------------------------------------------
# Codegen — provide `nros_generate_interfaces()` / `nros_find_interfaces()`.
# Mirrors the FreeRTOS module: the root CMake only includes the codegen
# module on the POSIX branch (it builds the codegen Rust tool via
# Corrosion in that branch). For cross-compile NuttX, consumers point
# `_NANO_ROS_CODEGEN_TOOL` at a host-side binary produced by a
# parallel POSIX configure (see the FreeRTOS module's comment for the
# pattern). The module's own `find_program(nros-codegen)` walks PATH
# when nothing is pre-set.
# ---------------------------------------------------------------------------
# Phase 195 audit (a) — switched off the retired
# `packages/codegen/.../nros-codegen-c` submodule copy (source-tree walk-up
# into the submodule Phase 195.D deletes) to the canonical in-tree module
# (Phase 137.2; identical `nros_generate_interfaces()` / `nros_find_interfaces()`
# surface). `nros_bootstrap_codegen()` still resolves the host codegen binary.
set(_NANO_ROS_PREFIX "${CMAKE_CURRENT_LIST_DIR}/../.." CACHE INTERNAL "")
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosBootstrapCodegen.cmake")
nros_bootstrap_codegen()
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosGenerateInterfaces.cmake")

# ---------------------------------------------------------------------------
# Per-board overlay — REQUIRED for NuttX. Overlays own the FFI crate
# path, the cargo target triple, and the `nros_board_link_app`
# implementation that drives `nuttx_build_example`. Today the only
# board is `nuttx-qemu-arm` (QEMU ARM virt, Cortex-A7).
# ---------------------------------------------------------------------------
if(NOT DEFINED NANO_ROS_BOARD)
    message(FATAL_ERROR
        "nano-ros-nuttx: NANO_ROS_BOARD is required for the NuttX "
        "platform (e.g. -DNANO_ROS_BOARD=nuttx-qemu-arm). Boards "
        "supply the FFI crate path, cargo target triple, and the "
        "`nros_board_link_app` implementation that drives "
        "`nuttx_build_example`.")
endif()

set(_nros_nuttx_board_module
    "${CMAKE_CURRENT_LIST_DIR}/../board/nano-ros-board-${NANO_ROS_BOARD}.cmake")
if(NOT EXISTS "${_nros_nuttx_board_module}")
    message(FATAL_ERROR
        "nano-ros-nuttx: no board overlay at "
        "${_nros_nuttx_board_module}. Add a "
        "cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake module or "
        "pick a supported board (e.g. nuttx-qemu-arm).")
endif()
include("${_nros_nuttx_board_module}")

# ---------------------------------------------------------------------------
# Native-C platform shim (`packages/core/nros-platform-nuttx`).
# Compiles the POSIX sibling sources — NuttX exposes a POSIX-compatible
# surface, the Rust `NuttxPlatform` similarly forwards to PosixPlatform.
# The shim's archive is consumed transitively by the NuttX FFI crate;
# we still build it here so the CMake link graph captures the
# dependency and a stand-alone `cmake --build` of the shim works.
# ---------------------------------------------------------------------------
# The C shim's `add_library(nros_platform_nuttx STATIC ...)` compiles
# the POSIX sibling sources (`packages/core/nros-platform-posix/src/{platform,
# net,timer}.c`) which #include `<arpa/inet.h>` / `<semaphore.h>` /
# `<signal.h>`. NuttX-the-OS supplies these headers via its own include
# tree (`$NUTTX_DIR/include`), but the cmake subproject doesn't pull
# that path in — it's the Rust FFI crate (`nros-nuttx-ffi`) that owns
# the NuttX include + link surface. The cargo build of the FFI crate
# is what actually links the platform glue into the app binary; the
# cmake archive was a host-side smoke check that doesn't survive a
# cross compile against `arm-none-eabi-gcc`. Skip the subdirectory
# when cross-compiling.
if(NOT TARGET nros_platform_nuttx AND NOT CMAKE_CROSSCOMPILING)
    add_subdirectory(
        "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-nuttx"
        nros_platform_nuttx)
endif()

# ---------------------------------------------------------------------------
# NanoRos::Platform alias. INTERFACE wrapper around the native-C shim
# (when present) or an empty stub on cross builds (the FFI crate's
# cargo build supplies the real glue).
# ---------------------------------------------------------------------------
add_library(nros_platform_nuttx_iface INTERFACE)
if(TARGET nros_platform_nuttx)
    target_link_libraries(nros_platform_nuttx_iface INTERFACE nros_platform_nuttx)
endif()
if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_nuttx_iface)
endif()

# ---------------------------------------------------------------------------
# nros_platform_link_app(<target>)
#
# Per-app NuttX fixup. The user wrote
#
#     add_executable(my_app src/main.c)
#     target_link_libraries(my_app PRIVATE std_msgs__nano_ros_c NanoRos::NanoRos)
#     nros_platform_link_app(my_app)
#
# On NuttX the actual ELF is the NuttX kernel image, built by `cargo
# build` on `nros-nuttx-ffi`. We treat the `add_executable` target as
# a declarative shape carrier (name + source + interface libs) and
# delegate to `nros_board_link_app(<target>)` which calls
# `nuttx_build_example(...)` under the hood. The `add_executable`
# target itself is not directly built (or rather, it is built as a
# stub for CMake bookkeeping; the cargo custom_target produces the
# real ELF alongside).
# ---------------------------------------------------------------------------
function(nros_platform_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_platform_link_app: '${target}' is not a CMake target.")
    endif()

    # Phase 249 P2b — generated STRONG `nros_app_register_backends` for every
    # C/C++ app (manifest-driven). On NuttX the backend is already whole-archived
    # (phase-243 #48 fix) so `nros_rmw_<x>_register` resolves; this makes the
    # explicit call universal rather than relying on the weak no-op fallback.
    # Idempotent. (Build-check tier — NuttX single-pass ld validated on its CI.)
    if(COMMAND nano_ros_link_rmw)
        nano_ros_link_rmw(${target})
    endif()

    if(COMMAND nros_board_link_app)
        nros_board_link_app(${target})
    else()
        message(FATAL_ERROR
            "nros_platform_link_app: board overlay for "
            "NANO_ROS_BOARD=${NANO_ROS_BOARD} did not define "
            "`nros_board_link_app(target)`.")
    endif()
endfunction()
