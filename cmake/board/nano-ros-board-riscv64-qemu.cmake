# cmake/board/nano-ros-board-riscv64-qemu.cmake
#
# Phase 138.3 / 144.7 — board overlay for ThreadX RISC-V 64-bit
# QEMU virt. Bare-metal port: NetX Duo TCP/IP stack + virtio-net
# driver + picolibc + rust-lld link toolchain. Mirrors the legacy
# `packages/core/nros-c/cmake/threadx-riscv64-support.cmake` shape,
# with paths pointed at the in-tree board crate
# (`packages/boards/nros-board-threadx-qemu-riscv64`) rather than the
# install prefix.
#
# Loaded by `cmake/platform/nano-ros-threadx.cmake` when
# NANO_ROS_BOARD=riscv64-qemu. The platform module is what
# `add_subdirectory(<nano-ros-root>)` reaches first; this overlay only
# runs once we know we are targeting ThreadX-on-RV64-QEMU.
#
# Required cmake variables (env or -D, all auto-defaulted to vendored
# in-tree paths if unset):
#   THREADX_DIR        — ThreadX kernel source root
#                        (default: third-party/threadx/kernel)
#   NETX_DIR           — NetX Duo source root
#                        (default: third-party/threadx/netxduo)
#   THREADX_CONFIG_DIR — tx_user.h / nx_user.h / nx_port.h / link.lds
#                        (default: board crate's config/)
#   THREADX_BOARD_DIR  — board crate C dir (entry.s, app_define.c,
#                        tx_thread_*.S overrides)
#                        (default: board crate's c/)
#   VIRTIO_DRIVER_DIR  — virtio-net-netx driver source dir
#                        (default: packages/drivers/virtio-net-netx)
#
# What this overlay declares:
#
#   threadx_kernel       STATIC    — ThreadX kernel (risc-v64/gnu port,
#                                    with board-supplied tx_thread_*.S
#                                    overrides + QEMU virt example_build
#                                    board.c/plic.c/uart.c)
#   netxduo              STATIC    — NetX Duo common stack + BSD addon
#   virtio_net_netx      STATIC    — virtio-net NetX Duo driver
#   threadx_glue         STATIC    — board C glue (app_define.c,
#                                    entry.s, syscalls.c, trap.c,
#                                    hwtimer.c)
#   threadx_platform     INTERFACE — umbrella linking glue + driver +
#                                    netxduo + kernel +
#                                    nros_platform_threadx + picolibc +
#                                    libgcc, plus the link script via
#                                    nros_board_link_app
#
# What this overlay exports (CACHE INTERNAL):
#
#   THREADX_LINKER_SCRIPT      — full path to link.lds
#   THREADX_STARTUP_SOURCE     — list of .c files added to the app target
#                                (board's startup.c — calls
#                                nros_threadx_set_config /
#                                nros_threadx_set_app_main /
#                                tx_kernel_enter from main())
#   THREADX_STARTUP_INCLUDES   — include dirs the startup TUs need
#   THREADX_GLUE_DEFINES       — TX_INCLUDE_USER_DEFINE_FILE etc.
#                                (NB: app_define.c lives in
#                                threadx_glue STATIC here — RV64 has
#                                no archive-ordering issue because the
#                                board crate's link layout is curated.)
#
#   nros_board_link_app(<target>) — applies the linker script + bare-metal
#                                   flags (--nmagic --gc-sections
#                                   -nostartfiles -u app_main and
#                                   --undefined=memset/memcpy/memmove
#                                   to keep weak overrides reachable).

if(DEFINED _NROS_BOARD_RISCV64_QEMU_INCLUDED)
    return()
endif()
set(_NROS_BOARD_RISCV64_QEMU_INCLUDED TRUE)

# ---------------------------------------------------------------------------
# Resolve in-tree asset paths.
# ---------------------------------------------------------------------------
set(_NROS_BOARD_ROOT  "${CMAKE_CURRENT_LIST_DIR}/../..")
set(_NROS_BOARD_DIR
    "${_NROS_BOARD_ROOT}/packages/boards/nros-board-threadx-qemu-riscv64")
set(_NROS_BOARD_CONFIG_DIR "${_NROS_BOARD_DIR}/config")
set(_NROS_BOARD_C_DIR      "${_NROS_BOARD_DIR}/c")
set(_NROS_BOARD_STARTUP_C  "${_NROS_BOARD_DIR}/startup.c")
set(_NROS_BOARD_CXX_COMPAT_DIR "${_NROS_BOARD_DIR}/cxx-compat")
set(_NROS_VIRTIO_DRIVER_DIR
    "${_NROS_BOARD_ROOT}/packages/drivers/virtio-net-netx")

# Default vendored locations — overridable via -D/env.
if(NOT DEFINED THREADX_DIR)
    if(DEFINED ENV{THREADX_DIR})
        set(THREADX_DIR "$ENV{THREADX_DIR}"
            CACHE PATH "ThreadX kernel source root (from env)")
    else()
        set(THREADX_DIR "${_NROS_BOARD_ROOT}/third-party/threadx/kernel"
            CACHE PATH "ThreadX kernel source root")
    endif()
endif()
if(NOT DEFINED NETX_DIR)
    if(DEFINED ENV{NETX_DIR})
        set(NETX_DIR "$ENV{NETX_DIR}"
            CACHE PATH "NetX Duo source root (from env)")
    else()
        set(NETX_DIR "${_NROS_BOARD_ROOT}/third-party/threadx/netxduo"
            CACHE PATH "NetX Duo source root")
    endif()
endif()
# THREADX_CONFIG_DIR is board-specific (each board ships its own
# tx_user.h / nx_user.h / link.lds). The .env file ships a single
# legacy value pointing at the threadx-linux config dir — ignore it
# and FORCE the right per-board path here.
#
# Phase 155.D — also patch the cmake process env so cargo
# invocations spawned by corrosion (and any other subprocess)
# see the RISC-V board's config dir instead of inheriting the
# stale .envrc value. cmake CACHE variables don't propagate to
# subprocess env automatically. NETX_CONFIG_DIR uses the same
# board dir today (both `tx_user.h` and `nx_user.h` live next
# to each other under the board's `config/`); patch it too so
# the cmake-driven C / C++ fixture rebuild path sees the right
# `nx_port.h`.
set(THREADX_CONFIG_DIR "${_NROS_BOARD_CONFIG_DIR}"
    CACHE PATH "Directory with tx_user.h / nx_user.h / link.lds" FORCE)
set(NETX_CONFIG_DIR "${_NROS_BOARD_CONFIG_DIR}"
    CACHE PATH "Directory with nx_user.h / nx_port.h" FORCE)
set(ENV{THREADX_CONFIG_DIR} "${_NROS_BOARD_CONFIG_DIR}")
set(ENV{NETX_CONFIG_DIR}    "${_NROS_BOARD_CONFIG_DIR}")
# Phase 155.D — same propagation for `THREADX_PORT` +
# `THREADX_EXTRA_INCLUDES`. RISC-V Rust examples set these in
# per-example `.cargo/config.toml [env]`; cmake-driven cargo
# (corrosion) doesn't see those, so propagate via process env.
set(ENV{THREADX_PORT}            "risc-v64/gnu")
set(ENV{THREADX_EXTRA_INCLUDES}
    "${THREADX_DIR}/ports/risc-v64/gnu/example_build/qemu_virt")
if(NOT DEFINED THREADX_BOARD_DIR AND NOT DEFINED ENV{THREADX_BOARD_DIR})
    set(THREADX_BOARD_DIR "${_NROS_BOARD_C_DIR}"
        CACHE PATH "ThreadX RV64 board C dir (entry.s, app_define.c, ...)")
endif()
if(NOT DEFINED VIRTIO_DRIVER_DIR AND NOT DEFINED ENV{VIRTIO_DRIVER_DIR})
    set(VIRTIO_DRIVER_DIR "${_NROS_VIRTIO_DRIVER_DIR}"
        CACHE PATH "virtio-net-netx driver source dir")
endif()

# ---------------------------------------------------------------------------
# Validate vendored asset presence (fail fast with a clear pointer at
# the missing pieces, rather than a downstream `tx_api.h: No such file`).
# ---------------------------------------------------------------------------
if(NOT EXISTS "${_NROS_BOARD_CONFIG_DIR}/tx_user.h")
    message(FATAL_ERROR
        "nano-ros-board-riscv64-qemu: tx_user.h not found at "
        "${_NROS_BOARD_CONFIG_DIR}/tx_user.h.")
endif()
if(NOT EXISTS "${_NROS_BOARD_CONFIG_DIR}/link.lds")
    message(FATAL_ERROR
        "nano-ros-board-riscv64-qemu: linker script not found at "
        "${_NROS_BOARD_CONFIG_DIR}/link.lds.")
endif()
if(NOT EXISTS "${_NROS_BOARD_STARTUP_C}")
    message(FATAL_ERROR
        "nano-ros-board-riscv64-qemu: startup.c not found at "
        "${_NROS_BOARD_STARTUP_C}.")
endif()
if(NOT EXISTS "${_NROS_BOARD_C_DIR}/board_threadx_qemu_riscv64.c")
    message(FATAL_ERROR
        "nano-ros-board-riscv64-qemu: board_threadx_qemu_riscv64.c not found at "
        "${_NROS_BOARD_C_DIR}/board_threadx_qemu_riscv64.c (Phase 152.2.B.1 renamed from app_define.c).")
endif()

# ---------------------------------------------------------------------------
# picolibc must be set up *before* nros_threadx_build_kernel so the
# kernel/netxduo/virtio-net STATIC libs see picolibc headers via the
# global -isystem we install on CMAKE_C_FLAGS / CMAKE_CXX_FLAGS.
# ---------------------------------------------------------------------------
nros_threadx_validate(REQUIRE NETX_DIR THREADX_BOARD_DIR VIRTIO_DRIVER_DIR)
nros_threadx_setup_picolibc(CXX_COMPAT_DIR "${_NROS_BOARD_CXX_COMPAT_DIR}")

# ---------------------------------------------------------------------------
# Build kernel + NetX Duo + virtio-net via the layer-2 helpers.
# Board overrides the soft-float-incompatible context-switch / scheduler
# assembly with ULONG=4 layout fixes (board's c/ directory).
#
# The port + overrides are RISC-V assembly (.S). The ament-shape leaf declares
# only `LANGUAGES C CXX` (byte-identical CMakeLists — phase-287 W6; the old
# leaves carried `ASM` themselves), so the BOARD enables ASM: without it cmake
# SILENTLY drops every .S source and the kernel links with undefined
# `_tx_thread_stack_build` / `_tx_thread_system_return`.
# ---------------------------------------------------------------------------
enable_language(ASM)
if(NOT TARGET threadx_kernel)
    nros_threadx_build_kernel(
        PORT          "risc-v64/gnu"
        BOARD_DIR     "${THREADX_BOARD_DIR}"
        BOARD_OVERRIDES
            tx_initialize_low_level.S
            tx_thread_schedule.S
            tx_thread_context_save.S
            tx_thread_context_restore.S
            tx_thread_stack_build.S
            tx_thread_system_return.S
        QEMU_VIRT_DIR "${THREADX_DIR}/ports/risc-v64/gnu/example_build/qemu_virt"
        QEMU_VIRT_EXCLUDE
            trap.c
            hwtimer.c
            demo_threadx.c
        EXTRA_INCLUDES "${NETX_DIR}/common/inc"
                       "${NETX_DIR}/addons/BSD"
                       "${VIRTIO_DRIVER_DIR}/include"
        EXTRA_DEFINES NX_INCLUDE_USER_DEFINE_FILE NROS_PLATFORM_BAREMETAL)
endif()

if(NOT TARGET netxduo)
    nros_threadx_build_netstack_netxduo(
        NETX_DIR      "${NETX_DIR}"
        DRIVER_DIR    "${VIRTIO_DRIVER_DIR}"
        EXTRA_DEFINES NROS_PLATFORM_BAREMETAL)
endif()

nros_threadx_setup_rust_lld()

# ---------------------------------------------------------------------------
# threadx_platform composition. The shim (`nros_platform_threadx`)
# is added later by the platform module via `add_subdirectory`. Here
# we compose the rest: glue (app_define.c + entry.s + trap.c + ...) +
# virtio driver + NetX Duo + kernel + picolibc + libgcc.
#
# RV64 has no archive-ordering issue from Phase 112.E.fix because the
# board crate's link layout is curated (Rust-side link ordering is
# controlled by board's build.rs for Rust examples; the C/C++ link
# pulls these in via threadx_platform's INTERFACE which lists them in
# reverse-dependency order).
# ---------------------------------------------------------------------------
if(NOT TARGET threadx_glue)
    # Phase 152.2.B.1 — app_define.c renamed to per-board
    # `board_threadx_qemu_riscv64.c` (shared `tx_application_define`
    # + byte pool / app thread plumbing moved into
    # `nros-board-common/c/threadx_hooks.c`).
    set(_glue_srcs
        "${THREADX_BOARD_DIR}/board_threadx_qemu_riscv64.c"
        "${_NROS_BOARD_ROOT}/packages/boards/nros-board-common/c/threadx_hooks.c"
        "${THREADX_BOARD_DIR}/entry.s"
        "${THREADX_BOARD_DIR}/trap.c"
        "${THREADX_BOARD_DIR}/syscalls.c"
        "${THREADX_BOARD_DIR}/hwtimer.c")
    nros_threadx_build_glue(
        SOURCES ${_glue_srcs}
        # Phase 155.E — `NX_BSD_ENABLE_NATIVE_API` flips the
        # `nxd_bsd.h` typedef-alias chain from
        # `nx_bsd_suseconds_t = suseconds_t` (line 209;
        # collides with picolibc's `suseconds_t` typedef) to
        # `typedef LONG nx_bsd_suseconds_t` (native NetX type,
        # no collision). Same flag the Rust-side build sets via
        # threadx nros-platform.toml; missing here because the cmake
        # glue compile defaulted to TX_INCLUDE_USER_DEFINE_FILE
        # alone. `NX_INCLUDE_USER_DEFINE_FILE` ditto so nx_user.h's
        # define block is honoured for the bare-metal C glue.
        DEFINES NROS_PLATFORM_BAREMETAL
                NX_BSD_ENABLE_NATIVE_API
                NX_INCLUDE_USER_DEFINE_FILE)
    # The kernel's includes (qemu_virt board, netxduo BSD, virtio) are
    # already on threadx_kernel's INTERFACE — pull them onto threadx_glue
    # so app_define.c finds <nx_bsd.h>, <virtio_net.h>, etc.
    target_link_libraries(threadx_glue PUBLIC threadx_kernel)
    if(TARGET virtio_net_netx)
        target_link_libraries(threadx_glue PUBLIC virtio_net_netx)
    endif()
    if(TARGET netxduo)
        target_link_libraries(threadx_glue PUBLIC netxduo)
    endif()
endif()

if(NOT TARGET threadx_platform)
    # phase-251 W1 — `--allow-multiple-definition` removed. The only dup it masked
    # was the board's STRONG `memset/memcpy/memmove` (startup.c byte loops, dodging
    # compiler_builtins' TLS-sensitive variants) vs compiler_builtins' WEAK ones;
    # strong-over-weak resolves with no flag. A duplicate defined symbol is now a
    # link error (the wrong-copy hazard the flag hid).
    nros_threadx_compose_platform(
        COMPONENTS    threadx_glue
                      virtio_net_netx
                      netxduo
                      threadx_kernel
        LINK_LIBS     c "${NROS_THREADX_LIBGCC_PATH}"
        LINK_OPTIONS  -L${NROS_THREADX_PICOLIBC_LIB_DIR}
        DEFINES       NROS_PLATFORM_BAREMETAL)
endif()

# ---------------------------------------------------------------------------
# Phase 214.P follow-up — `NROS_APP_CONFIG` source-side definition for the
# cmake-driven consumer path.
#
# `packages/boards/nros-board-threadx-qemu-riscv64/build.rs::emit_nros_app_config`
# (Phase 212.M-F.10.3, `a488e51db`) bakes a TU defining the universal
# `const nros_app_config_t NROS_APP_CONFIG = { ... };` symbol into
# `libnros_app_config_def.a`, then propagates the link via
# `cargo:rustc-link-lib=static=nros_app_config_def`. That works for the
# pure-cargo Rust path and for corrosion-imported staticlibs whose
# `.build_script_output()` is walked.
#
# The C / C++ cmake-driven examples (and the CMake/Corrosion Cyclone
# Rust talker at `examples/qemu-riscv64-threadx/rust/talker/CMakeLists.txt`)
# all `add_subdirectory(<repo-root>)` → `nros_platform_link_app(<target>)`
# → this overlay's `nros_board_link_app`. None of them traversed
# corrosion's link-arg side channel for the *board* crate (corrosion
# imports `nros-c`/`nros-cpp`/`nros-rmw-zenoh-staticlib` only — the
# board's `nros_app_config_def` staticlib never made the link line). So
# every C / C++ threadx-rv64 example failed at link time with
# `rust-lld: error: undefined symbol: NROS_APP_CONFIG`, blocking the
# whole `just threadx_riscv64 build-fixtures` flow — including the two
# CycloneDDS fixtures Track P needs.
#
# Fix: emit the same TU body the rust build.rs writes into a
# per-configure file under the binary dir, and prepend it to
# `THREADX_STARTUP_SOURCE` so `nros_platform_link_app` (via
# `target_sources`) compiles it into every example target. The TU
# `#include`s `<nros/app_config.h>`, reachable through the platform
# overlay's include set; the values mirror the Rust path verbatim, so
# both paths bake byte-identical defaults into the running firmware.
# Phase 214.P — per-fixture network identity overrides. Phase 212.M.10
# (`55f36c6a9`) retired the per-example `nros.toml` files, which had let
# `examples/qemu-riscv64-threadx/c/listener` carry `ip = 10.0.2.41 / mac
# `…:57` distinct from `talker`'s `…:40 / …:56`. With the toml gone every
# fixture now collapses onto the board default below (10.0.2.40, MAC :56),
# so the threadx-rv64 two-QEMU Cyclone e2e talker + listener boot on
# IDENTICAL IPs over a shared L2 segment → unicast SEDP / data plane can't
# disambiguate the peer → listener never receives. Phase 177.26's "21/21
# received" verification 2026-05-26 predated the M.10 sweep on 2026-06-02
# (3 weeks later), so the e2e regressed silently.
#
# Make the IP + MAC trailing octet overridable via cmake cache vars so each
# fixture can carve its own L2 / L3 identity. Defaults match the pre-M.10
# `qemu-riscv64-threadx/c/talker/nros.toml` values, so a fixture that
# omits the override keeps the historical talker identity. The threadx
# riscv64 `test_threadx_riscv64_cyclonedds_two_qemu_pubsub` test passes
# `NROS_APP_NET_IP_LAST=41` + `NROS_APP_NET_MAC_LAST=0x57` for the
# listener via `examples/fixtures.toml`-driven cmake `-D` (Phase 181.5.e
# pattern); the talker keeps the defaults. Wider host-byte / netmask
# overrides aren't needed today — every fixture sits in 10.0.2.0/24.
set(NROS_APP_NET_IP_LAST 40
    CACHE STRING "Trailing octet of NROS_APP_CONFIG.network.ip (default 40 → 10.0.2.40)")
set(NROS_APP_NET_MAC_LAST "0x56"
    CACHE STRING "Trailing octet of NROS_APP_CONFIG.network.mac (default 0x56)")

set(_NROS_APP_CONFIG_DEF_C
    "${CMAKE_CURRENT_BINARY_DIR}/nros_app_config_def.c")
file(WRITE "${_NROS_APP_CONFIG_DEF_C}"
"/*\n"
" * Auto-generated by cmake/board/nano-ros-board-riscv64-qemu.cmake\n"
" * Phase 214.P follow-up to Phase 212.M-F.10.3 — `NROS_APP_CONFIG`\n"
" * source-side emission for the cmake-driven consumer path. Mirrors\n"
" * the rust-side body in packages/boards/nros-board-threadx-qemu-riscv64\n"
" * /build.rs so a cmake / cargo build of the same board produce a\n"
" * byte-identical `NROS_APP_CONFIG` symbol (modulo per-fixture\n"
" * NROS_APP_NET_IP_LAST / NROS_APP_NET_MAC_LAST overrides — see the\n"
" * Phase 214.P block in nano-ros-board-riscv64-qemu.cmake).\n"
" */\n"
"\n"
"#include <stdint.h>\n"
"#include <nros/app_config.h>\n"
"\n"
"const nros_app_config_t NROS_APP_CONFIG = {\n"
"    .zenoh = {\n"
"        .locator   = \"tcp/10.0.2.2:7553\",\n"
"        .domain_id = 0,\n"
"    },\n"
"    .network = {\n"
"        .ip      = { 10, 0, 2, ${NROS_APP_NET_IP_LAST} },\n"
"        .mac     = { 0x52, 0x54, 0x00, 0x12, 0x34, ${NROS_APP_NET_MAC_LAST} },\n"
"        .gateway = { 10, 0, 2, 2 },\n"
"        .netmask = { 255, 255, 255, 0 },\n"
"        .prefix  = 24,\n"
"    },\n"
"    .scheduling = {\n"
"        .app_priority            = 0,\n"
"        .zenoh_read_priority     = 0,\n"
"        .zenoh_lease_priority    = 0,\n"
"        .poll_priority           = 0,\n"
"        .app_stack_bytes         = 0,\n"
"        .zenoh_read_stack_bytes  = 0,\n"
"        .zenoh_lease_stack_bytes = 0,\n"
"        .poll_interval_ms        = 0,\n"
"    },\n"
"};\n")

# ---------------------------------------------------------------------------
# Per-app glue: the board's startup.c only (app_define.c is in
# threadx_glue STATIC above — RV64 needs it inside a static lib so the
# weak-override resolution for board's entry.s / trap.c works against
# the curated link order). Exporting THREADX_APP_DEFINE_SOURCE empty
# tells nros_platform_link_app to skip the per-app add.
# ---------------------------------------------------------------------------
set(THREADX_STARTUP_SOURCE
    "${_NROS_BOARD_STARTUP_C}"
    "${_NROS_APP_CONFIG_DEF_C}"
    CACHE INTERNAL "ThreadX / riscv64-qemu startup TU + NROS_APP_CONFIG def")

set(THREADX_APP_DEFINE_SOURCE ""
    CACHE INTERNAL "Empty — RV64's app_define.c lives in threadx_glue STATIC")

set(THREADX_STARTUP_INCLUDES
    ${NROS_THREADX_INCLUDES}
    "${NETX_DIR}/common/inc"
    "${NETX_DIR}/addons/BSD"
    CACHE INTERNAL "Include dirs for THREADX_STARTUP_SOURCE TUs")

# phase-241 C.2 — derive the capability defines (e.g. NROS_PLATFORM_HAS_MALLOC)
# from this board's `[board.capabilities]` in nros-board.toml (the SSoT), instead
# of the hand-set `-D` the issue-0038 fix added here. `heap = true` for this
# bare-metal-with-CFFI-heap board → NROS_PLATFORM_HAS_MALLOC, so nros-cpp's
# HeapString/HeapSequence (used by generated message types) compile.
include("${_NROS_BOARD_ROOT}/cmake/NanoRosCapabilities.cmake")
nros_board_capability_defines("${_NROS_BOARD_DIR}" _NROS_BOARD_CAP_DEFINES)

set(THREADX_GLUE_DEFINES
    ${NROS_THREADX_DEFINES}
    NX_INCLUDE_USER_DEFINE_FILE
    NROS_PLATFORM_BAREMETAL
    ${_NROS_BOARD_CAP_DEFINES}
    CACHE INTERNAL "Compile defines for THREADX_STARTUP_SOURCE TUs")

set(THREADX_LINKER_SCRIPT "${_NROS_BOARD_CONFIG_DIR}/link.lds"
    CACHE FILEPATH "ThreadX RISC-V linker script")

# nros-platform-threadx/src/net.c needs upstream NetX Duo BSD declarations
# AND the user-define-file flag so NX_BSD_ENABLE_NATIVE_API in nx_user.h
# shadows the un-prefixed `select` / `fd_set` / `suseconds_t` declarations
# that otherwise collide with picolibc's <sys/types.h>. The platform
# module reads these cache vars after add_subdirectory(...).
set(NROS_THREADX_EXTRA_INCLUDES
    "${NETX_DIR}/common/inc"
    "${NETX_DIR}/addons/BSD"
    CACHE INTERNAL "Extra include dirs for nros_platform_threadx")
set(NROS_THREADX_EXTRA_DEFINES
    TX_INCLUDE_USER_DEFINE_FILE
    NX_INCLUDE_USER_DEFINE_FILE
    NROS_PLATFORM_BAREMETAL
    CACHE INTERNAL "Extra compile defines for nros_platform_threadx")

# ---------------------------------------------------------------------------
# nros_board_link_app(<target>)
#
# Apply the linker script + bare-metal RV64 flags. `-u app_main`
# keeps the app entry symbol live; `--undefined=memset/memcpy/memmove`
# pulls in the board's strong memset/memcpy/memmove overrides
# (startup.c provides byte-loop versions to dodge Rust
# compiler_builtins' TLS-sensitive variants). The board defs are STRONG and
# compiler_builtins' are WEAK, so they resolve with no `--allow-multiple-definition`
# (phase-251 W1 removed it from threadx_platform's INTERFACE).
# ---------------------------------------------------------------------------
function(nros_board_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_board_link_app: '${target}' is not a CMake target.")
    endif()
    target_link_options(${target} PRIVATE
        "-T${THREADX_LINKER_SCRIPT}"
        "--nmagic"
        "-Wl,--gc-sections"
        "-nostartfiles"
        "-u" "app_main"
        "-Wl,--undefined=memset"
        "-Wl,--undefined=memcpy"
        "-Wl,--undefined=memmove")
endfunction()

# ---------------------------------------------------------------------------
# Issue #205 step 3 — one-call CMake seam for the Rust CycloneDDS app shape.
#
# Collapses the boilerplate every `examples/qemu-riscv64-threadx/rust/*`
# CMakeLists used to carry after `add_subdirectory(<nano-ros>)`: the corrosion
# staticlib import, the (formerly hand-written `src/cyclonedds_app.c`) empty
# link-anchor TU — now GENERATED into the build dir — the executable, and the
# platform/RMW link calls. Interface generation stays in the example (it is
# the app's own interface declaration).
#
#   nros_generate_interfaces(std_msgs "msg/String.msg" LANGUAGE C SKIP_INSTALL)
#   nros_threadx_rv64_rust_cyclone_app(riscv64_threadx_rust_talker_cyclonedds
#       CRATE qemu-riscv64-threadx-talker
#       LINK  std_msgs__nano_ros_c)
#
# CRATE is the cargo package name; the corrosion target is derived from it
# (`-` → `_`). The Rust crate must expose `app_main` (the board crate's
# `cyclonedds_app_main!(register)` macro emits it).
# ---------------------------------------------------------------------------
function(nros_threadx_rv64_rust_cyclone_app target)
    cmake_parse_arguments(_A "" "CRATE;DOMAIN" "LINK" ${ARGN})
    if(NOT _A_CRATE)
        message(FATAL_ERROR
            "nros_threadx_rv64_rust_cyclone_app(${target}): CRATE <cargo-package> is required.")
    endif()
    string(REPLACE "-" "_" _crate_target "${_A_CRATE}")

    corrosion_import_crate(
        MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
        CRATES "${_A_CRATE}"
        CRATE_TYPES staticlib
        NO_DEFAULT_FEATURES
        FEATURES rmw-cyclonedds)

    # Issue #214 — DOMAIN bake for the Rust `Config::default()` (drives the
    # Executor/Cyclone participant; mirrors the C fixtures' `-DNROS_DOMAIN_ID`).
    # Reaches `option_env!("NROS_DOMAIN_ID")` via corrosion build env; falls
    # back to the configure's `-DNROS_DOMAIN_ID` cache var. The NetX wire
    # identity is NOT set here — it comes from the `NROS_APP_NET_{IP,MAC}_LAST`
    # cache vars (set them BEFORE `add_subdirectory(<nano-ros>)`; they feed the
    # generated `NROS_APP_CONFIG` TU that startup.c applies pre-kernel).
    if(DEFINED _A_DOMAIN)
        corrosion_set_env_vars(${_crate_target} "NROS_DOMAIN_ID=${_A_DOMAIN}")
    elseif(DEFINED NROS_DOMAIN_ID)
        corrosion_set_env_vars(${_crate_target} "NROS_DOMAIN_ID=${NROS_DOMAIN_ID}")
    endif()

    # Empty TU so the executable has a C compilation unit for the link driver;
    # the real entry is the Rust staticlib's `app_main`.
    set(_anchor "${CMAKE_CURRENT_BINARY_DIR}/${target}_link_anchor.c")
    if(NOT EXISTS "${_anchor}")
        file(WRITE "${_anchor}"
            "/* Generated by nros_threadx_rv64_rust_cyclone_app — link anchor only. */\n"
            "void ${target}_link_anchor(void) {}\n")
    endif()

    add_executable(${target} "${_anchor}")
    target_link_libraries(${target} PRIVATE
        ${_crate_target}
        ${_A_LINK}
        NanoRos::NanoRos)
    nros_platform_link_app(${target})
    nano_ros_link_rmw(${target} RMW cyclonedds)
endfunction()
