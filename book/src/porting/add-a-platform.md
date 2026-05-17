# Adding a Platform (CMake Layer)

> **Scope.** This page covers the **CMake-side** glue: how the build
> system picks the right platform shim, links the right archives, and
> dispatches per-app fixups (linker scripts, startup files, ISR vectors).
> For the runtime traits a new platform must implement in Rust /
> platform-cffi, see [Custom Platform](./custom-platform.md).

Since Phase 138, porting a new platform to nano-ros is **one new file**:

```
nano-ros/cmake/platform/nano-ros-<plat>.cmake
```

Adding a platform no longer touches the root `CMakeLists.txt`, never
edits per-example trees, and never duplicates platform helpers across
20+ examples.

## Module contract (Phase 138 §A)

Every `cmake/platform/nano-ros-<plat>.cmake` must expose:

| Symbol | Kind | Purpose |
|---|---|---|
| `NanoRos::Platform` | `ALIAS` library | Aliased to the platform-specific `INTERFACE` target; linked into `NanoRos::NanoRos` umbrella by the root CMakeLists. |
| `nros_platform_<plat>_iface` | `INTERFACE` library | Carries the platform staticlib + host-system libs + transitive deps. |
| `nros_platform_link_app(target)` | function | Per-app fixups: linker script, startup objects, ISR vectors, RTOS-specific final-link tweaks. Empty for POSIX. |
| `NROS_PLATFORM_LINK_FEATURES` | cache variable | Default link-feature set for this platform (e.g. `tcp udp_unicast`). |

The root `CMakeLists.txt` dispatches via:

```cmake
include("${CMAKE_CURRENT_SOURCE_DIR}/cmake/platform/nano-ros-${NANO_ROS_PLATFORM}.cmake")
```

When the user's project sets `NANO_ROS_PLATFORM=foo` before
`add_subdirectory(nano-ros)`, your module at
`cmake/platform/nano-ros-foo.cmake` runs and supplies the four contract
elements above.

## Minimal skeleton

```cmake
# cmake/platform/nano-ros-foo.cmake

if(DEFINED _NROS_PLATFORM_FOO_INCLUDED)
    return()
endif()
set(_NROS_PLATFORM_FOO_INCLUDED TRUE)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast
    CACHE STRING "Default link features for the Foo platform")

# Build / pull in the platform staticlib. May be add_subdirectory(...)
# into packages/core/nros-platform-foo/ for a Cargo + cmake hybrid, or
# may declare an IMPORTED target pointing at a prebuilt RTOS archive.
add_subdirectory(
    "${CMAKE_CURRENT_LIST_DIR}/../../packages/core/nros-platform-foo"
    nros_platform_foo_build)

add_library(nros_platform_foo_iface INTERFACE)
if(TARGET nros_platform_foo)
    target_link_libraries(nros_platform_foo_iface INTERFACE nros_platform_foo)
endif()

if(NOT TARGET NanoRos::Platform)
    add_library(NanoRos::Platform ALIAS nros_platform_foo_iface)
endif()

function(nros_platform_link_app target)
    # Per-app fixups go here. Linker scripts, startup objects, ISR
    # vectors. Delegate to cmake/board/nano-ros-board-${NANO_ROS_BOARD}.cmake
    # when the platform supports multiple boards.
    if(DEFINED NANO_ROS_BOARD)
        include("${CMAKE_CURRENT_LIST_DIR}/../board/nano-ros-board-${NANO_ROS_BOARD}.cmake")
        if(COMMAND nros_board_link_app)
            nros_board_link_app(${target})
        endif()
    endif()
endfunction()
```

That's it. The root CMakeLists picks up the new platform automatically
— the validation block resolves `cmake/platform/nano-ros-foo.cmake` and
fatals out only when no such file exists.

## Board overlays

When your platform spans multiple boards (different MCUs, link scripts,
peripheral inits), carve a board overlay:

```
nano-ros/cmake/board/nano-ros-board-<board>.cmake
```

Board overlays provide `nros_board_link_app(target)` and run
**inside** `nros_platform_link_app` when the user sets `NANO_ROS_BOARD`.
The board layer owns:

- Linker script selection (`target_link_options(${target} PRIVATE -T<script>)`)
- Startup object emission (`crt0.o`, vector table, etc.)
- MCU-specific final-link flags

See `cmake/board/nano-ros-board-mps2-an385.cmake` for a working example.

## Legacy `find_package(NanoRos)` support

The Phase 138 modules are dual-installed under
`<prefix>/lib/cmake/NanoRos/cmake/platform/` so consumers still using
the legacy `find_package(NanoRos CONFIG)` path can pick them up after
`just install-local`. Once Phase 140 retires `install-local` entirely,
the dual-install rule in the root `CMakeLists.txt` goes away and
`add_subdirectory(nano-ros)` becomes the only entry point.

## Where the existing layer-2 helpers live

For RTOS ports that compose the kernel + netstack + glue inside CMake
(FreeRTOS, ThreadX, NuttX), the per-RTOS helper functions
(`nros_freertos_build_kernel`, `nros_threadx_compose_platform`, …)
stay at `packages/core/nros-c/cmake/nros-<rtos>.cmake`. The Phase 138
modules `include(...)` those helpers — the per-platform file is the
**dispatch entry**, not the implementation.

## See also

- [Custom Platform](./custom-platform.md) — Rust-side trait
  implementations (clock, alloc, threading, networking)
- [Custom Board Package](./custom-board.md) — the Rust board crate that
  carries the linker script and board-specific drivers
- [Build as a CMake subdirectory](../getting-started/build-as-subdirectory.md)
  — user-facing intro to `add_subdirectory(nano-ros)`
