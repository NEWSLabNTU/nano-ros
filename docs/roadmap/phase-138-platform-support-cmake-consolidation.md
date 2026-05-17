# Phase 138 — Platform-Support CMake Consolidation

**Goal.** Move every per-platform CMake helper (link-script bootstrap, toolchain hints, platform-aliases.c emission decision, link-feature defaults) out of example trees and per-package CMakeLists into a single `nano-ros/cmake/platform/nano-ros-<plat>.cmake` per supported platform. Per-example CMake shrinks to ≤15 lines. Adding a new platform = adding one file, not edits scattered across 20+ examples.

**Status.** Not started.

**Priority.** P1 — directly enables Phase 137's "≤10-line per-example CMakeLists" promise. Without 138, the boilerplate that motivated `find_package(NanoRos)` just relocates to per-example `add_subdirectory(<repo>)` glue.

**Depends on.** Phase 137 (root `CMakeLists.txt` is the dispatch entry that includes these modules).

**Related.** Phase 139 (RTOS integration shells consume these modules under their RTOS-native build systems). Phase 140 (`install-local` removal — these modules become the single source of truth for platform setup).

---

## Overview

Per-platform CMake glue is scattered today:

- `packages/zpico/zpico-zephyr/cmake/` — Zephyr module integration
- `packages/core/nros-platform-freertos/cmake/` — FreeRTOS link helpers
- `packages/boards/nros-board-stm32f4/cmake/` — board-specific overlay
- `examples/*/cmake/<plat>-support.cmake` — duplicated platform-helper modules sitting next to many examples (CLAUDE.md "Examples = Standalone Projects" used to allow this; now the dup is the problem)
- Per-RTOS `Layer-2` modules (`nros-{threadx,freertos,nuttx}.cmake`) that ship via the install layout (`build/install/lib/cmake/NanoRos/`)

Six platform-helper variants exist; each example carries some subset. Adding a new platform requires editing every example that supports it. Phase 131's examples-tree revision normalised the directory layout; Phase 138 normalises the CMake.

After this phase, one file per supported platform owns ALL platform-specific CMake. Examples carry the same 3 lines regardless of platform.

---

## Architecture

### A. Target layout

```
nano-ros/
└── cmake/
    └── platform/
        ├── nano-ros-posix.cmake
        ├── nano-ros-zephyr.cmake
        ├── nano-ros-freertos.cmake
        ├── nano-ros-nuttx.cmake
        ├── nano-ros-threadx.cmake
        └── nano-ros-baremetal.cmake
```

Each module exposes a uniform contract:

```cmake
# cmake/platform/nano-ros-<plat>.cmake

# Required cache vars (set by the caller BEFORE include):
#   NANO_ROS_PLATFORM = "<plat>"
#   NANO_ROS_RMW      = "<rmw>"
#
# Provides:
#   IMPORTED target  NanoRos::Platform              ← link-time platform shim
#   INTERFACE target nros_platform_${NANO_ROS_PLATFORM}  ← linked into NanoRos::NanoRos
#   Function         nros_platform_link_app(target) ← per-app fixups (link script, startup, ISR vectors)
#   Variable         NROS_PLATFORM_LINK_FEATURES    ← default LinkFeatures for this platform
```

`nros_platform_link_app(my_app)` is the only call examples make besides `add_subdirectory` + `target_link_libraries`. It handles:

- Per-platform link script injection (Cortex-M3 `memory.x`, Cortex-M4 `link.x`, etc.)
- Startup object emission (e.g. `crt0.o` for bare-metal)
- ISR vector table linkage
- RTOS-specific final-link tweaks (Zephyr's `--no-undefined` exception list, NuttX's `--whole-archive` for module registration)

Today this work lives inline in every example. Phase 138 hoists it.

### B. Per-platform module skeleton

```cmake
# cmake/platform/nano-ros-zephyr.cmake — example
# (For native_sim, qemu_cortex_a9, fvp_baser_aemv8r etc.)

# Pull in the existing Zephyr module integration (was at packages/zpico/zpico-zephyr/).
add_subdirectory(${CMAKE_CURRENT_LIST_DIR}/../../packages/zpico/zpico-zephyr zpico_zephyr)

# Default link features for Zephyr.
set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast)

# Platform staticlib (libnros_platform_zephyr.a).
add_library(nros_platform_zephyr INTERFACE)
target_link_libraries(nros_platform_zephyr INTERFACE zpico_zephyr_static)

# IMPORTED alias used by NanoRos::NanoRos's INTERFACE_LINK_LIBRARIES.
add_library(NanoRos::Platform ALIAS nros_platform_zephyr)

# Per-app fixup. Zephyr requires nothing here (west handles link).
function(nros_platform_link_app target)
    # Zephyr's link is driven by zephyr-aware CMake outside this module —
    # no-op when invoked from a non-Zephyr CMakeLists. Examples building
    # against `add_subdirectory(<nano-ros>)` from a normal CMakeLists go
    # through the Phase 139 integration shell instead.
endfunction()
```

```cmake
# cmake/platform/nano-ros-baremetal.cmake — example (qemu-arm-baremetal et al.)

set(NROS_PLATFORM_LINK_FEATURES tcp udp_unicast udp_multicast)

add_library(nros_platform_baremetal INTERFACE)
# Pulls in the existing libnros_platform_posix.a-equivalent for bare-metal.

add_library(NanoRos::Platform ALIAS nros_platform_baremetal)

function(nros_platform_link_app target)
    if(NOT DEFINED NANO_ROS_BOARD)
        message(FATAL_ERROR "Bare-metal builds must set NANO_ROS_BOARD (mps2-an385, stm32f4, esp32-c3, ...)")
    endif()
    include(${CMAKE_CURRENT_LIST_DIR}/../board/nano-ros-board-${NANO_ROS_BOARD}.cmake)
    nros_board_link_app(${target})
endfunction()
```

### C. Example shrink

Before (per-example, ~50 lines including platform helpers):

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker C)
find_package(NanoRos REQUIRED CONFIG)
include(cmake/freertos-support.cmake)
nros_freertos_setup_toolchain()
add_executable(c_talker src/main.c)
nros_freertos_link_lwip(c_talker)
nros_freertos_link_fsp(c_talker)
target_link_libraries(c_talker PRIVATE NanoRos::NanoRos)
# ...20 lines of board-specific link script + startup file glue
```

After (Phase 138, ~10 lines):

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker C)
set(NANO_ROS_PLATFORM freertos)
set(NANO_ROS_RMW     zenoh)
set(NANO_ROS_BOARD   mps2-an385)
add_subdirectory(../../../../../ nano_ros)
add_executable(c_talker src/main.c)
target_link_libraries(c_talker PRIVATE NanoRos::NanoRos)
nros_platform_link_app(c_talker)
```

Everything platform-specific moves into `nano-ros/cmake/platform/`. Examples become pure user-code.

---

## Work Items

- [ ] **138.1 — Audit current platform-helper duplication.**
      Walk `examples/**/cmake/`, `packages/boards/*/cmake/`,
      `packages/core/nros-platform-*/cmake/`,
      `packages/zpico/zpico-*/cmake/`. Build a table: per file →
      callers → unique vs duplicated. Land table in this doc under
      "Notes". Drives 138.2's consolidation scope.
      **Files.** none (read-only audit, results land here).

- [ ] **138.2 — Create `cmake/platform/nano-ros-<plat>.cmake` modules.**
      One file per supported platform: posix, zephyr, freertos,
      nuttx, threadx, baremetal. Each conforms to the contract in
      §A. Content moves from the per-package and per-example sites
      identified in 138.1.
      **Files.** `cmake/platform/nano-ros-{posix,zephyr,freertos,nuttx,threadx,baremetal}.cmake` (new).

- [ ] **138.3 — Add board-overlay layer.**
      `cmake/board/nano-ros-board-<board>.cmake` per supported
      bare-metal board (mps2-an385, stm32f4, esp32-c3-qemu,
      riscv64-qemu, ...). Used by `nros_platform_link_app` when
      `NANO_ROS_PLATFORM=baremetal`. Content moves from
      `packages/boards/*/cmake/` and example-tree variants.
      **Files.** `cmake/board/nano-ros-board-*.cmake` (new).

- [ ] **138.4 — Delete per-example `cmake/<plat>-support.cmake`.**
      After 138.2 + 138.3 land, the per-example duplicates are
      redundant. Delete every `examples/**/cmake/` subdir. Each
      example's main `CMakeLists.txt` ends up at ≤15 lines.
      **Files.** `examples/**/cmake/` (deleted),
      `examples/**/CMakeLists.txt` (shrunk).

- [ ] **138.5 — Provide migration shim during transition.**
      `find_package(NanoRos)` consumers (legacy path) still need the
      `Layer-2` modules (`nros-freertos.cmake`, etc.) shipped to the
      install prefix. Update install rules so the same files at
      `cmake/platform/` get installed to `<prefix>/lib/cmake/NanoRos/`
      under the old names — single source of truth, dual surface.
      **Files.** `CMakeLists.txt` (install rule additions).

- [ ] **138.6 — Test parity across platforms.**
      Add `packages/testing/nros-tests/tests/cmake_platform_matrix.rs`
      that drives a tiny user project through each platform module,
      asserts the binary links cleanly. POSIX runs natively;
      cross-compile platforms (zephyr, freertos, nuttx, threadx)
      gated by toolchain presence (`[SKIPPED]` per CLAUDE.md if
      cross-toolchain missing).
      **Files.** `packages/testing/nros-tests/tests/cmake_platform_matrix.rs` (new).

- [ ] **138.7 — Doc update.**
      `book/src/porting/add-a-platform.md` updated: porting a new
      platform = adding one file at `cmake/platform/nano-ros-<plat>.cmake`,
      not editing every example. Replace existing porting walkthrough.
      **Files.** `book/src/porting/add-a-platform.md`,
      `book/src/SUMMARY.md`.

---

## Acceptance

- [ ] `find examples -path '*/cmake/*-support.cmake'` returns empty
      after 138.4.
- [ ] Every example's `CMakeLists.txt` is ≤15 lines.
- [ ] Per-platform module contract from §A holds: each
      `cmake/platform/nano-ros-<plat>.cmake` exposes
      `NanoRos::Platform`, `nros_platform_${NANO_ROS_PLATFORM}`,
      `nros_platform_link_app()`, `NROS_PLATFORM_LINK_FEATURES`.
- [ ] `cmake_platform_matrix` test passes for at least POSIX in CI
      (cross-toolchain platforms `[SKIPPED]` cleanly when toolchain
      absent).
- [ ] Legacy `find_package(NanoRos)` consumers (anything built via
      `just install-local`) still work — install rules from 138.5
      ship the modules under the old names.
- [ ] `just ci` green.

---

## Notes

- **Why six platforms now, more later.** posix, zephyr, freertos,
  nuttx, threadx, baremetal are the production set today. Each new
  platform = one new file in `cmake/platform/`. Phase 137's root
  CMake dispatches via `include(cmake/platform/nano-ros-${NANO_ROS_PLATFORM}.cmake)`,
  so adding a platform never touches the root.
- **Board ≠ platform.** STM32F4 + NUCLEO-F429ZI is a *board*
  (overlays a platform with pinmux, clock, memory layout). The
  board layer in `cmake/board/` sits below the platform layer. A
  baremetal-Cortex-M4 board uses `platform=baremetal` +
  `board=stm32f4-nucleo`. Phase 138.3 carves the board layer
  cleanly; today it's smeared across `packages/boards/` and
  per-example helpers.
- **Migration shim is temporary.** 138.5 keeps the install-time
  shape so `find_package(NanoRos)` users don't break mid-refactor.
  Phase 140 deletes the shim along with `install-local` itself.
- **Avoid the example-helper-from-source antipattern.** Examples
  must NOT include files from `nano-ros/cmake/platform/` directly;
  they include via the root `CMakeLists.txt`'s dispatch. Otherwise
  examples and the root CMake drift over time, same shape as the
  Phase 134 multicast bug.
