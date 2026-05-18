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
# ---------------------------------------------------------------------------
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
        DEFINES NROS_PLATFORM_BAREMETAL)
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
    nros_threadx_compose_platform(
        COMPONENTS    threadx_glue
                      virtio_net_netx
                      netxduo
                      threadx_kernel
        LINK_LIBS     c "${NROS_THREADX_LIBGCC_PATH}"
        LINK_OPTIONS  --allow-multiple-definition
                      -L${NROS_THREADX_PICOLIBC_LIB_DIR}
        DEFINES       NROS_PLATFORM_BAREMETAL)
endif()

# ---------------------------------------------------------------------------
# Per-app glue: the board's startup.c only (app_define.c is in
# threadx_glue STATIC above — RV64 needs it inside a static lib so the
# weak-override resolution for board's entry.s / trap.c works against
# the curated link order). Exporting THREADX_APP_DEFINE_SOURCE empty
# tells nros_platform_link_app to skip the per-app add.
# ---------------------------------------------------------------------------
set(THREADX_STARTUP_SOURCE
    "${_NROS_BOARD_STARTUP_C}"
    CACHE INTERNAL "ThreadX / riscv64-qemu startup TU")

set(THREADX_APP_DEFINE_SOURCE ""
    CACHE INTERNAL "Empty — RV64's app_define.c lives in threadx_glue STATIC")

set(THREADX_STARTUP_INCLUDES
    ${NROS_THREADX_INCLUDES}
    "${NETX_DIR}/common/inc"
    "${NETX_DIR}/addons/BSD"
    CACHE INTERNAL "Include dirs for THREADX_STARTUP_SOURCE TUs")

set(THREADX_GLUE_DEFINES
    ${NROS_THREADX_DEFINES}
    NX_INCLUDE_USER_DEFINE_FILE
    NROS_PLATFORM_BAREMETAL
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
# compiler_builtins' TLS-sensitive variants). `--allow-multiple-definition`
# is on threadx_platform's INTERFACE already.
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
