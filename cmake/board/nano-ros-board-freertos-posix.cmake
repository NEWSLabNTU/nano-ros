# cmake/board/nano-ros-board-freertos-posix.cmake
#
# phase-370 W2 — board overlay for the FreeRTOS POSIX simulator.
#
# Loaded by `cmake/platform/nano-ros-freertos.cmake` when
# NANO_ROS_BOARD=freertos-posix. The FreeRTOS kernel's `ThirdParty/GCC/Posix`
# port runs tasks as host pthreads with signal-driven preemption; everything
# below the kernel is the host, which is what makes this a BOARD variant rather
# than a platform (phase-292 W4.a's "go, small" scoping decision).
#
# What this overlay is NOT, and the three lines that say so:
#
#   * no cross toolchain — nothing here sets CMAKE_C_COMPILER or a linker
#     script. The default host compiler is correct, and that is the point: this
#     is the freertos family's first lane that CI can run without QEMU.
#   * no lwIP — `nros_freertos_build_lwip()` is not called and no `lwip` target
#     is declared. `NROS_PLATFORM_FREERTOS_WITH_NET=OFF` keeps the platform
#     shim from compiling `net.c`, which is the shim's own documented opt-out
#     for exactly this case ("boards that disable lwIP").
#   * no netif driver — sockets are the host's.
#
# What this overlay declares:
#
#   freertos_kernel   STATIC     — kernel + the Posix port + heap_3, built by
#                                  the layer-2 helper with the port's extra
#                                  `utils/wait_for_event.c`
#   freertos_platform INTERFACE  — the umbrella apps link (kernel + pthread)
#
# What this overlay exports (CACHE INTERNAL):
#
#   FREERTOS_STARTUP_SOURCE     — the board's entry + hooks, plus the family's
#                                 kernel-only task glue
#   FREERTOS_STARTUP_INCLUDES   — include dirs those TUs need
#
#   nros_board_link_app(<target>) — links pthread and the host libs the POSIX
#                                   port needs.

if(DEFINED _NROS_BOARD_FREERTOS_POSIX_INCLUDED)
    return()
endif()
set(_NROS_BOARD_FREERTOS_POSIX_INCLUDED TRUE)

set(_NROS_BOARD_ROOT  "${CMAKE_CURRENT_LIST_DIR}/../..")
set(_NROS_BOARD_DIR   "${_NROS_BOARD_ROOT}/packages/boards/nros-board-freertos-posix")
set(_NROS_BOARD_CONFIG_DIR "${_NROS_BOARD_DIR}/config")
set(_NROS_FREERTOS_FAMILY_DIR
    "${_NROS_BOARD_ROOT}/packages/boards/nros-board-freertos")

# The C/C++ lane compiles the family's kernel-only glue plus this board's own
# entry and hooks. `network_glue.c` and `freertos_hooks.c` are deliberately
# absent: the first includes lwIP unconditionally, the second is ARM inline
# assembly (semihosting, `wfi`, `SysTick_Handler`). phase-370 W1 split
# `freertos_task_glue.c` out of `network_glue.c` so the half that is about
# FreeRTOS rather than about lwIP is shared rather than copied.
#
# `freertos_run_tiers.c` carries over unchanged — it is FreeRTOS plus nros-cpp
# FFI and knows nothing about the chip; the unused function is dropped by
# --gc-sections for single-tier apps.
set(_NROS_BOARD_STARTUP_C
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_task_glue.c"
    "${_NROS_FREERTOS_FAMILY_DIR}/c/freertos_run_tiers.c"
    "${_NROS_BOARD_DIR}/c/freertos_posix_hooks.c"
    "${_NROS_BOARD_DIR}/c/freertos_posix_entry.c")

# ---------------------------------------------------------------------------
# Fail-fast asset checks, naming the file rather than letting the compiler
# report a missing include three frames later.
# ---------------------------------------------------------------------------
if(NOT EXISTS "${_NROS_BOARD_CONFIG_DIR}/FreeRTOSConfig.h")
    message(FATAL_ERROR
        "nano-ros-board-freertos-posix: FreeRTOSConfig.h not found at "
        "${_NROS_BOARD_CONFIG_DIR}/FreeRTOSConfig.h.")
endif()
foreach(_src IN LISTS _NROS_BOARD_STARTUP_C)
    if(NOT EXISTS "${_src}")
        message(FATAL_ERROR
            "nano-ros-board-freertos-posix: startup source not found at ${_src}.")
    endif()
endforeach()

set(FREERTOS_CONFIG_DIR "${_NROS_BOARD_CONFIG_DIR}" CACHE PATH
    "Directory containing FreeRTOSConfig.h for freertos-posix" FORCE)

# The port is a BOARD FACT here, so this board SETS it rather than defaulting it.
#
# Every other FreeRTOS overlay writes `if(NOT DEFINED FREERTOS_PORT AND NOT
# DEFINED ENV{FREERTOS_PORT})`, which reads a family-wide default that
# `activate.sh` exports for the Cortex-M boards:
#
#     just/sdk-env.just:4: export FREERTOS_PORT := env("FREERTOS_PORT", "GCC/ARM_CM3")
#
# On this board that default can never be right, and deferring to it is not a
# hypothetical: it compiled `portable/GCC/ARM_CM3/port.c` into a host binary and
# failed on `configMAX_SYSCALL_INTERRUPT_PRIORITY` — an NVIC macro, undeclared
# because the POSIX FreeRTOSConfig.h has no NVIC to describe. The error named a
# macro in a vendored header, three layers from the variable that chose it.
#
# The POSIX simulator has exactly one port, so an explicit disagreement is a
# mistake worth naming rather than overriding in silence. The ENV is not
# consulted at all: it is a Cortex-M default that says nothing about this board.
#
# The path carries `ThirdParty/` because upstream ships the port under
# `portable/ThirdParty/GCC/Posix` rather than `portable/GCC/…`.
set(_NROS_BOARD_FREERTOS_PORT "ThirdParty/GCC/Posix")
if(DEFINED FREERTOS_PORT AND NOT FREERTOS_PORT STREQUAL "${_NROS_BOARD_FREERTOS_PORT}")
    message(FATAL_ERROR
        "nano-ros-board-freertos-posix: FREERTOS_PORT is '${FREERTOS_PORT}', but this "
        "board has exactly one port: '${_NROS_BOARD_FREERTOS_PORT}' (the FreeRTOS POSIX "
        "simulator). Drop the -DFREERTOS_PORT, or pick a board whose port you meant.")
endif()
set(FREERTOS_PORT "${_NROS_BOARD_FREERTOS_PORT}" CACHE STRING
    "FreeRTOS portable-layer subdir for freertos-posix" FORCE)

# ---------------------------------------------------------------------------
# Kernel. Two departures from every other FreeRTOS board here, both required by
# the port rather than chosen:
#
#   * heap_3, not heap_4 — heap_3 wraps the host `malloc`/`free`. heap_4 would
#     carve a fixed `ucHeap[]` out of the process image and then enforce a
#     budget the host does not have, which is a simulator lying about a
#     constraint.
#   * `utils/wait_for_event.c` — the Posix port's own dependency (its
#     `port.c` builds task suspension on that event abstraction). Upstream
#     ships it beside the port rather than in `portable/`, so the generic
#     builder cannot infer it.
# ---------------------------------------------------------------------------
nros_freertos_validate(REQUIRE FREERTOS_PORT)

if(NOT TARGET freertos_kernel)
    nros_freertos_build_kernel(
        PORT "${FREERTOS_PORT}"
        HEAP heap_3
        EXTRA_SOURCES "${FREERTOS_DIR}/portable/ThirdParty/GCC/Posix/utils/wait_for_event.c")
endif()
if(TARGET freertos_kernel)
    # The Posix port casts away const in a few places upstream; the tree builds
    # with -Wcast-qual on and this is vendored code.
    target_compile_options(freertos_kernel PRIVATE -Wno-cast-qual)
    # A FreeRTOS task IS a pthread here, so the kernel itself needs the library.
    find_package(Threads REQUIRED)
    target_link_libraries(freertos_kernel PUBLIC Threads::Threads)
endif()

# ---------------------------------------------------------------------------
# The platform shim must not compile `net.c`: it includes <lwip/sockets.h> and
# <lwip/netdb.h>, and there is no lwIP in this build. FORCE because the shim
# declares it as an `option()`, whose default would otherwise win on a fresh
# cache.
# ---------------------------------------------------------------------------
set(NROS_PLATFORM_FREERTOS_WITH_NET OFF CACHE BOOL
    "freertos-posix: host sockets, no lwIP — skip the shim's net.c" FORCE)

# The kernel above is built with heap_3, so the shim's heap stats must not call
# `xPortGetFreeHeapSize` — heap_3 keeps no free-list and does not define it.
set(NROS_PLATFORM_FREERTOS_HEAP_3 ON CACHE BOOL
    "freertos-posix: heap_3 wraps the host malloc; heap stats read mallinfo2" FORCE)

# The host C library already provides `gethostname` and `clock_gettime`, and
# `__aeabi_read_tp`/`__tls_base` are an ARM bare-metal pairing with no meaning
# in a hosted process. Compiling the compat TU here fails on `__tls_base` (no
# linker script defines it) and would shadow glibc for the other two.
set(NROS_PLATFORM_FREERTOS_WITH_BAREMETAL_COMPAT OFF CACHE BOOL
    "freertos-posix: the host C library provides what cyclonedds_compat.c fills in" FORCE)

# Read by `cmake/platform/nano-ros-freertos.cmake` BEFORE it stages the Phase
# 186 Cyclone flags. Those set `WITH_FREERTOS`/`WITH_LWIP` on ddsrt, which
# selects a ddsrt whose sockets are lwIP's — against a build with no lwIP to
# link. This board wants the ddsrt the `posix` platform branch gets, which is
# the whole of W2's "zero new RMW work": host ddsrt, host sockets.
set(NROS_FREERTOS_BOARD_HAS_LWIP FALSE)

# ---------------------------------------------------------------------------
# freertos_platform — the umbrella apps link. No linker script, no
# `-nostartfiles`, no `--specs=nosys.specs`: this is a hosted link.
# ---------------------------------------------------------------------------
if(NOT TARGET freertos_platform)
    nros_freertos_compose_platform(
        COMPONENTS
            freertos_kernel)
endif()

# ---------------------------------------------------------------------------
# Startup sources + include dirs, compiled IN the app target so the
# per-build `nros/app_config.h` is visible to the entry TU (the same reason
# the mps2 overlay does it, and the reason issue 0434's note there matters:
# do NOT add the nros-c SOURCE include dir here — nros-c exports it as an
# INTERFACE include and adding it early shadows the generated headers).
# ---------------------------------------------------------------------------
set(FREERTOS_STARTUP_SOURCE
    ${_NROS_BOARD_STARTUP_C}
    CACHE INTERNAL "FreeRTOS / freertos-posix startup translation units")

set(FREERTOS_STARTUP_INCLUDES
    ${NROS_FREERTOS_INCLUDES}
    CACHE INTERNAL "Include dirs for FREERTOS_STARTUP_SOURCE TUs")

# ---------------------------------------------------------------------------
# nros_board_link_app(<target>)
#
# `nros_platform_link_app()` calls this after wiring the startup sources and
# `freertos_platform`. pthread comes through the kernel's PUBLIC link, but name
# it here too: the app's own TUs link directly against the port's pthread
# symbols when --gc-sections drops the kernel objects that would have pulled it.
# ---------------------------------------------------------------------------
function(nros_board_link_app target)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "nros_board_link_app: '${target}' is not a CMake target.")
    endif()
    find_package(Threads REQUIRED)
    target_link_libraries(${target} PRIVATE Threads::Threads)
endfunction()
