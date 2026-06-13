# cmake/platform/nano-ros-threadx.cmake
#
# Phase 138.2 / 144.7-8 — ThreadX platform module. Single source of
# truth for ThreadX platform-shim wiring under the Phase 137
# `add_subdirectory(<nano-ros-root>)` consumption shape. Used by both
# `qemu-riscv64-threadx` (NANO_ROS_BOARD=riscv64-qemu) and
# `threadx-linux` (NANO_ROS_BOARD=threadx-linux).
#
# What this module composes:
#
#   * Re-exports the layer-2 helper functions
#     (`nros_threadx_validate`, `nros_threadx_build_kernel`,
#     `nros_threadx_build_netstack_nsos`,
#     `nros_threadx_build_netstack_netxduo`, `nros_threadx_build_glue`,
#     `nros_threadx_setup_picolibc`, `nros_threadx_setup_rust_lld`,
#     `nros_threadx_strip_builtins`, `nros_threadx_compose_platform`) —
#     the implementation lives under
#     `packages/core/nros-c/cmake/nros-threadx.cmake` and stays the
#     single source of truth.
#
#   * Pulls in the per-board overlay
#     (`cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake`) EAGERLY —
#     overlays for ThreadX need to declare `threadx_kernel`,
#     `netxduo`/`nsos_netx`, optional driver libs, and compose
#     `threadx_platform` BEFORE this module wires it into the umbrella.
#
#     without an install step (Phase 140 removed the legacy install path).
#
#   * Defines `nros_platform_link_app(target)` — links
#     `threadx_platform` onto the app target, appends the board's
#     startup translation units, then delegates to the board overlay's
#     `nros_board_link_app(target)` for linker-script + per-toolchain
#     flag fixups (RISC-V `-T<link.lds>` --nmagic -u app_main vs
#     Linux-host pthread no-op).
#
# Contract (Phase 138 §A):
#   NanoRos::Platform                — INTERFACE alias for the shim
#   nros_platform_threadx_iface      — concrete INTERFACE behind it
#   nros_platform_link_app(<target>) — per-app fixup
#   NROS_PLATFORM_LINK_FEATURES      — default link feature set

if(DEFINED _NROS_PLATFORM_THREADX_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_THREADX_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast
    CACHE STRING "Default link features for the ThreadX platform")

# ---------------------------------------------------------------------------
# Layer-2 helpers (kernel / netstack / glue / compose). Implementation
# lives under packages/core/nros-c/cmake/; re-include here so per-board
# overlays + per-example CMakeLists.txt see the same function names
# regardless of consumption shape.
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-c/cmake/nros-threadx.cmake")

# ---------------------------------------------------------------------------
# User-facing nano-ros helpers (config + link).
# ---------------------------------------------------------------------------
include("${CMAKE_CURRENT_LIST_DIR}/../NanoRosLink.cmake")

# Phase 246 — the Phase-212.H.4 ThreadX system-codegen baker
# (`NanoRosThreadxSystemCodegen.cmake`, the NULL-context `nros_system_main`
# stand-in) is retired. ThreadX C/C++ now routes through the unified TYPED
# carrier (`nano_ros_node_register(TYPED)` → `nros codegen entry --typed`,
# RFC-0043 real-callback components); ThreadX Rust through `nros::main!()` /
# `ExecutorNodeRuntime`. No `nros_threadx_codegen_system` / `nros_threadx_link_app`.

# ---------------------------------------------------------------------------
# Codegen — provide `nros_generate_interfaces()` / `nros_find_interfaces()`.
# The root CMakeLists.txt only includes the codegen module on the POSIX
# branch (it builds the codegen Rust tool via Corrosion in that branch).
# For cross-compile branches (ThreadX RV64, etc.) consumers point
# `_NANO_ROS_CODEGEN_TOOL` at a host-side binary produced by a parallel
# POSIX configure (see the FreeRTOS module comment for the pattern).
# threadx-linux runs on the host so a system-built tool resolves
# automatically via PATH.
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
# Per-board overlay — REQUIRED for ThreadX. Unlike POSIX, ThreadX apps
# need a board-supplied tx_user.h / nx_user.h, app_define.c (creates
# byte pool + app thread), a netstack (NetX Duo + driver for bare-metal,
# nsos-netx shim for Linux-host) and — on RV64 — a linker script +
# startup assembly. The overlay declares threadx_kernel + the netstack
# static libs and composes threadx_platform.
# ---------------------------------------------------------------------------
if(NOT DEFINED NANO_ROS_BOARD)
    message(FATAL_ERROR
        "nano-ros-threadx: NANO_ROS_BOARD is required for the ThreadX "
        "platform (e.g. -DNANO_ROS_BOARD=riscv64-qemu or "
        "-DNANO_ROS_BOARD=threadx-linux). Boards supply tx_user.h, "
        "nx_user.h, app_define.c, netstack glue, and (RV64) the linker "
        "script + startup asm.")
endif()

set(_nros_threadx_board_module
    "${CMAKE_CURRENT_LIST_DIR}/../board/nano-ros-board-${NANO_ROS_BOARD}.cmake")
if(NOT EXISTS "${_nros_threadx_board_module}")
    message(FATAL_ERROR
        "nano-ros-threadx: no board overlay at "
        "${_nros_threadx_board_module}. Add a "
        "cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake module or "
        "pick a supported board (e.g. riscv64-qemu, threadx-linux).")
endif()
include("${_nros_threadx_board_module}")

# ---------------------------------------------------------------------------
# Phase 186 — CycloneDDS self-provision flags (ThreadX RISC-V64 + NetX Duo).
#
# Mirrors the retired scripts/cyclonedds/threadx-cross-probe.sh: when the Cyclone
# backend self-provisions from source (no prebuilt install — nros_provide_cyclonedds()),
# the Cyclone add_subdirectory needs WITH_THREADX + LTO off (the ddsrt ThreadX
# port's ops-walker trips under LTO / the xcdr opt_size fast-path — Phase 177.22/.23,
# gated inside Cyclone on DDSRT_WITH_THREADX) + the NetX/picolibc include paths.
# Board-gated to riscv64-qemu (cross rv64); the threadx-linux board is host-linked
# and resolves Cyclone differently, so it's excluded here.
# ---------------------------------------------------------------------------
if(NANO_ROS_RMW STREQUAL "cyclonedds"
   AND NANO_ROS_BOARD STREQUAL "riscv64-qemu"
   AND NOT DEFINED NROS_CYCLONE_THREADX_FLAGS_STAGED)
    set(NROS_CYCLONE_THREADX_FLAGS_STAGED TRUE)
    foreach(_off BUILD_SHARED_LIBS BUILD_IDLC BUILD_TESTING BUILD_IDLC_TESTING
                 BUILD_EXAMPLES BUILD_DDSPERF BUILD_DOCS ENABLE_SECURITY ENABLE_SSL
                 ENABLE_SHM ENABLE_IPV6 ENABLE_LTO CMAKE_INTERPROCEDURAL_OPTIMIZATION)
        set(${_off} OFF CACHE BOOL "Cyclone ThreadX cross trim (Phase 186)" FORCE)
    endforeach()
    set(WITH_THREADX ON CACHE BOOL "Cyclone ddsrt ThreadX port (Phase 186)" FORCE)
    set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
    # cmake/platform/ glue legitimately knows the repo layout (CLAUDE.md).
    if(NOT THREADX_DIR)
        set(THREADX_DIR "${CMAKE_CURRENT_LIST_DIR}/../../third-party/threadx/kernel")
    endif()
    if(NOT NETX_DIR)
        set(NETX_DIR "${CMAKE_CURRENT_LIST_DIR}/../../third-party/threadx/netxduo")
    endif()
    if(NOT THREADX_CONFIG_DIR)
        set(THREADX_CONFIG_DIR "${_NROS_BOARD_CONFIG_DIR}")
    endif()
    set(_tx_port_inc "${THREADX_DIR}/ports/risc-v64/gnu/inc")
    # picolibc sysroot (host tool query) — match the cross-probe's resolution.
    execute_process(
        COMMAND riscv64-unknown-elf-gcc -march=rv64gc -mabi=lp64d --specs=picolibc.specs -print-sysroot
        OUTPUT_VARIABLE _tx_picolibc OUTPUT_STRIP_TRAILING_WHITESPACE ERROR_QUIET)
    if(NOT _tx_picolibc OR NOT EXISTS "${_tx_picolibc}/include")
        set(_tx_picolibc "/usr/lib/picolibc/riscv64-unknown-elf")
    endif()
    # Phase 203 — use directory-level include_directories/add_compile_options
    # instead of mutating CMAKE_C_FLAGS. cyclonedds (via add_subdirectory) calls
    # `project(CycloneDDS ...)`, which **re-runs CMake's compiler init and resets
    # CMAKE_C_FLAGS to the toolchain's initial value** — any threadx flags
    # appended to CMAKE_C_FLAGS before project() get dropped from ddsc target's
    # per-target compile commands. Directory properties (include dirs, compile
    # options, compile definitions) survive the nested project() reset and
    # propagate to every subdirectory target (ddsc, ddsrt, …).
    include_directories(SYSTEM "${_tx_picolibc}/include")
    include_directories(
        "${THREADX_CONFIG_DIR}"
        "${THREADX_DIR}/common/inc"
        "${_tx_port_inc}"
        "${NETX_DIR}/common/inc"
        "${NETX_DIR}/addons/BSD")
    add_compile_options(-ffunction-sections -fdata-sections -fno-builtin -fno-lto)
    add_compile_definitions(TX_INCLUDE_USER_DEFINE_FILE NX_INCLUDE_USER_DEFINE_FILE)
endif()

# Phase 186.6.3 — threadx-linux is host-linked (x86): ThreadX runs as a Linux
# process and Cyclone uses the *host posix* ddsrt (not the rv64 WITH_THREADX
# port). So self-provision with the same host trims as native (nano-ros-posix.cmake):
# ENABLE_*/SHM off + a static ddsc linked into the app — no build/install, no
# runtime libddsc.so / system-substitution. No WITH_THREADX, no cross includes.
if(NANO_ROS_RMW STREQUAL "cyclonedds"
   AND NANO_ROS_BOARD STREQUAL "threadx-linux"
   AND NOT DEFINED NROS_CYCLONE_THREADX_FLAGS_STAGED)
    set(NROS_CYCLONE_THREADX_FLAGS_STAGED TRUE)
    set(ENABLE_SECURITY OFF CACHE BOOL "Cyclone: no DDS Security (Phase 186)" FORCE)
    set(ENABLE_SSL OFF CACHE BOOL "Cyclone: no TLS (Phase 186)" FORCE)
    set(ENABLE_SHM OFF CACHE BOOL "Cyclone: no Iceoryx SHM (Phase 186)" FORCE)
    set(BUILD_SHARED_LIBS OFF CACHE BOOL "Cyclone: static ddsc for self-provision (Phase 186)" FORCE)
endif()

# ---------------------------------------------------------------------------
# Native-C platform shim (`packages/core/nros-platform-threadx`). The
# board overlay declared `threadx_kernel` (+ `netxduo` or `nsos_netx`)
# and wired them in. The shim CMakeLists picks them up via the
# `THREADX_KERNEL_TARGET` / `NETXDUO_TARGET` cache vars. Disable its
# install rules — the umbrella project owns install layout.
# ---------------------------------------------------------------------------
set(THREADX_KERNEL_TARGET threadx_kernel CACHE STRING "" FORCE)
# Pick whichever netstack the board overlay declared. `nros-platform-threadx`'s
# net.c needs the BSD addon headers, which both `netxduo` and `nsos_netx`
# export — point NETXDUO_TARGET at whichever surfaced.
if(TARGET netxduo)
    set(NETXDUO_TARGET netxduo CACHE STRING "" FORCE)
elseif(TARGET nsos_netx)
    set(NETXDUO_TARGET nsos_netx CACHE STRING "" FORCE)
endif()
set(NROS_PLATFORM_THREADX_INSTALL OFF CACHE BOOL
    "Skip nros-platform-threadx install rules (umbrella owns install)" FORCE)
if(NOT TARGET nros_platform_threadx)
    add_subdirectory(
        "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-threadx"
        nros_platform_threadx)
    # The shim's CMakeLists links ${THREADX_KERNEL_TARGET} PUBLIC, but the
    # kernel target keeps its includes PRIVATE (nros_build_rtos_static_lib
    # default), so platform.c / timer.c / net.c can't find <tx_api.h>.
    # Push the layer-2 helper's resolved include list onto the shim — same
    # set the kernel itself was built with.
    if(TARGET nros_platform_threadx AND DEFINED NROS_THREADX_INCLUDES)
        target_include_directories(nros_platform_threadx PUBLIC
            ${NROS_THREADX_INCLUDES})
    endif()
    # Per-board extra include dirs + compile defines for the shim.
    # Board overlays populate NROS_THREADX_EXTRA_INCLUDES with upstream
    # NetX paths needed by net.c (the BSD addon's nxd_bsd.h declares
    # nx_bsd_inet_addr / nx_bsd_socket / ..., the port dir provides
    # nx_port.h) and NROS_THREADX_EXTRA_DEFINES with
    # NX_INCLUDE_USER_DEFINE_FILE so nx_user.h fires (its
    # NX_BSD_ENABLE_NATIVE_API in turn shadows the unprefixed BSD
    # declarations that otherwise collide with glibc <sys/select.h>).
    # Belt-and-braces: also auto-push the standard NetX paths when
    # `netxduo` is the netstack and no explicit override.
    if(TARGET nros_platform_threadx AND DEFINED NROS_THREADX_EXTRA_INCLUDES)
        target_include_directories(nros_platform_threadx PUBLIC
            ${NROS_THREADX_EXTRA_INCLUDES})
    elseif(TARGET nros_platform_threadx AND TARGET netxduo
           AND DEFINED NETX_DIR
           AND EXISTS "${NETX_DIR}/addons/BSD/nxd_bsd.h")
        target_include_directories(nros_platform_threadx PUBLIC
            "${NETX_DIR}/common/inc"
            "${NETX_DIR}/addons/BSD")
    endif()
    if(TARGET nros_platform_threadx AND DEFINED NROS_THREADX_EXTRA_DEFINES)
        target_compile_definitions(nros_platform_threadx PUBLIC
            ${NROS_THREADX_EXTRA_DEFINES})
    endif()
endif()

# ---------------------------------------------------------------------------
# NanoRos::Platform alias. `threadx_platform` is the INTERFACE umbrella
# the board overlay composed (kernel + netstack + glue). The
# `nros_platform_threadx` shim provides the canonical `nros_platform_*`
# ABI on top; link it INTO the umbrella so any consumer of
# NanoRos::Platform gets both.
# ---------------------------------------------------------------------------
if(TARGET threadx_platform AND TARGET nros_platform_threadx)
    target_link_libraries(threadx_platform INTERFACE nros_platform_threadx)
endif()

add_library(nros_platform_threadx_iface INTERFACE)
if(TARGET threadx_platform)
    target_link_libraries(nros_platform_threadx_iface INTERFACE threadx_platform)
endif()
# Phase 246 — propagate the board capability defines (e.g.
# NROS_PLATFORM_HAS_MALLOC from `heap = true`) onto the public platform
# INTERFACE so EVERY consumer inherits them — not just the app target that
# `nros_platform_link_app` touches. Without this, a separately-compiled
# Component static lib (the TYPED carrier's `<pkg>_<exec>_component`) parses
# nros-cpp's heap_string/heap_sequence headers on bare-metal (where
# NROS_PLATFORM_BAREMETAL suppresses the auto-malloc) WITHOUT the
# `nros_platform_malloc`/`free` declaration and fails to compile. Empty for the
# threadx-linux host board (hosted → malloc auto), so a no-op there.
if(DEFINED _NROS_BOARD_CAP_DEFINES AND _NROS_BOARD_CAP_DEFINES)
    target_compile_definitions(nros_platform_threadx_iface
        INTERFACE ${_NROS_BOARD_CAP_DEFINES})
endif()
if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_threadx_iface)
endif()

# ---------------------------------------------------------------------------
# nros_platform_link_app(<target>)
#
# Per-app ThreadX fixups. The board overlay populates the
# THREADX_STARTUP_SOURCE / THREADX_STARTUP_INCLUDES / THREADX_APP_DEFINE_SOURCE
# cache vars; we apply them to <target> here. Compiling startup.c +
# app_define.c IN the app target (rather than baking them into a library)
# keeps the example's per-build `nros/app_config.h` (APP_IP / APP_MAC,
# etc.) visible to startup.c and avoids the static-lib-extraction
# ordering problem from Phase 112.E.fix where `app_define.c`'s
# undef refs to `nros_platform_threadx_*` couldn't be resolved once
# the archive landed after NanoRos::NanoRos on the link line.
# ---------------------------------------------------------------------------
function(nros_platform_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_platform_link_app: '${target}' is not a CMake target.")
    endif()

    if(DEFINED THREADX_STARTUP_SOURCE)
        target_sources(${target} PRIVATE ${THREADX_STARTUP_SOURCE})
    endif()
    if(DEFINED THREADX_APP_DEFINE_SOURCE)
        target_sources(${target} PRIVATE ${THREADX_APP_DEFINE_SOURCE})
    endif()
    if(DEFINED THREADX_STARTUP_INCLUDES)
        target_include_directories(${target} PRIVATE ${THREADX_STARTUP_INCLUDES})
    endif()
    if(DEFINED THREADX_GLUE_DEFINES)
        target_compile_definitions(${target} PRIVATE ${THREADX_GLUE_DEFINES})
    endif()
    if(TARGET threadx_platform)
        target_link_libraries(${target} PRIVATE threadx_platform)
    endif()

    # Issue #20 — threadx-linux C++ + CycloneDDS: resolve the cyclone
    # backend's back-reference to `nros_rmw_cffi_register_named`.
    #
    # The root CMakeLists whole-archives `libnros_rmw_cyclonedds.a`
    # (which references `nros_rmw_cffi_register_named`, U) AFTER the
    # Corrosion staticlibs `libnros_c.a` / `libnros_cpp.a` (which DEFINE
    # it, T). With ld's single-pass archive semantics the cffi member is
    # only extracted if something references the symbol BEFORE those
    # archives are scanned. The native (POSIX) C++ examples get that for
    # free: their `main.cpp` calls `nros::init()` → `CffiSession::open`/
    # `nros_rmw_cffi_lookup`, which live in the SAME Rust object as
    # `nros_rmw_cffi_register_named`, so the member is pulled early. The
    # threadx-linux examples drive bring-up from a C `main.c` + the
    # ThreadX system-codegen, whose pre-cyclone references never touch
    # that object — so the member is left in the archive and the later
    # whole-archived cyclone group fails to resolve the symbol.
    #
    # Fix: add `-u nros_rmw_cffi_register_named` so the linker carries the
    # symbol as undefined from the start of the link and extracts the
    # defining member when it first scans `libnros_c.a` / `libnros_cpp.a`
    # (both of which already precede the cyclone group). The cyclone
    # group's pending U-reference then resolves against that pulled
    # member — no archive duplication, no link-order surgery. Scoped to
    # the threadx-linux board + cyclonedds RMW; other boards/RMWs are
    # unaffected.
    if(NANO_ROS_RMW STREQUAL "cyclonedds"
       AND NANO_ROS_BOARD STREQUAL "threadx-linux")
        target_link_options(${target} PRIVATE
            "LINKER:-u,nros_rmw_cffi_register_named")
    endif()

    # Delegate per-board fixup (linker script, --nmagic, -u app_main,
    # pthread, etc.) to the overlay.
    if(COMMAND nros_board_link_app)
        nros_board_link_app(${target})
    endif()
endfunction()
