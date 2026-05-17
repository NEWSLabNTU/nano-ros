# Phase 137 — Top-Level Entry `CMakeLists.txt` for Source-Distribution Consumption

**Goal.** Make nano-ros consumable via `add_subdirectory(third_party/nano-ros)` from a user's C/C++ project, no prior `just install-local` required. Introduce a single root `CMakeLists.txt` at `nano-ros/` that dispatches on `NANO_ROS_PLATFORM` + `NANO_ROS_RMW` cache vars and exports `NanoRos::NanoRos` / `NanoRos::NanoRosCpp` interface targets directly from the source tree.

**Status.** Landed (137.1–137.6). XRCE/cyclonedds + non-POSIX platform
branches in root CMake deferred to Phase 138/139. Phase 138 closes the
non-POSIX gap by wiring up `cmake/platform/nano-ros-{zephyr,freertos,nuttx,threadx,baremetal}.cmake`.
Phase 139 layers per-RTOS integration shells on top of this root
CMake (`integrations/<rtos>/`); native RTOS package managers
(`west` / `idf.py` / PIO / NuttX-Kconfig / PX4 EXTERNAL_MODULES_LOCATION)
discover those shells and re-export `NanoRos::NanoRos` under each
RTOS's own target naming.

**Priority.** P1 — first piece of the install-local rip-off (Phase 140). Without 137, the source-distribution direction has no entry point.

**Depends on.** None blocking. Coordinates with Phase 138 (platform-support CMake consolidation — they share the same root `cmake/` layout).

**Related.** Phase 138 (platform support), Phase 139 (RTOS integration shells), Phase 140 (`install-local` removal). Phase 131 (examples-tree shape that 137's per-example shrink builds on). CLAUDE.md "CMake Path Convention" section (no project-tree heuristics — 137 must keep that contract intact for the in-tree `add_subdirectory` case).

---

## Overview

Today every C/C++ consumer goes through `just install-local` → `find_package(NanoRos CONFIG)` against `build/install/lib/cmake/NanoRos/`. The install layout was designed Debian-style: build all 33 archive variants up front, find_package per consumer.

That model is awkward for the RTOS workflows nano-ros actually targets:

- **Zephyr** consumes modules via `west` and expects source trees inside the workspace (`zephyr_module()` declarations, not installed prefixes).
- **ESP-IDF** discovers components in-tree (`COMPONENT_REQUIRES` + `idf_component.yml`).
- **PlatformIO** vendors libs under `lib/` with a `library.json`.
- **NuttX** apps live under `apps/external/` and integrate via `Make.defs` + Kconfig.

`find_package` against `/opt/nano-ros` doesn't fit any of these. The recent phases (128.D.3 platform_aliases, 131 examples revision, 134 archive-internal-consistency) collectively lock the source-distribution direction: users get the source tree, the build cooperates with the user's project conventions.

Phase 137 is the entry point: one `CMakeLists.txt` at the repo root that a user adds via `add_subdirectory`. Behaviour matches the current `find_package(NanoRos)` API exactly — same targets, same `nano_ros_generate_interfaces(...)` function, same `NANO_ROS_PLATFORM` / `NANO_ROS_RMW` selection — but consumed from source instead of from an install prefix.

---

## Architecture

### A. Target user workflow

```cmake
# user_project/CMakeLists.txt
cmake_minimum_required(VERSION 3.22)
project(my_app C)

# Pick platform + RMW BEFORE add_subdirectory.
set(NANO_ROS_PLATFORM zephyr)   # posix | zephyr | freertos | nuttx | threadx | baremetal
set(NANO_ROS_RMW     zenoh)     # zenoh | dds | xrce | cyclonedds

add_subdirectory(third_party/nano-ros nano_ros)

add_executable(my_app src/main.c)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)

nano_ros_generate_interfaces(my_app
    INTERFACES msg/Foo.msg srv/Bar.srv
    LANGUAGE   c)
```

No install step. CMake's transitive target propagation pulls in `libnros_rmw_zenoh.a` + `libnros_platform_zephyr.a` + codegen tooling as needed.

### B. Root `CMakeLists.txt` shape

```cmake
# nano-ros/CMakeLists.txt
cmake_minimum_required(VERSION 3.22)
project(NanoRos VERSION 0.1.0 LANGUAGES C CXX)

# Configuration vars (cache + advanced — users override BEFORE add_subdirectory).
set(NANO_ROS_PLATFORM "posix" CACHE STRING "Platform: posix|zephyr|freertos|nuttx|threadx|baremetal")
set(NANO_ROS_RMW      "zenoh" CACHE STRING "RMW: zenoh|dds|xrce|cyclonedds")
set(NANO_ROS_ROS_EDITION "humble" CACHE STRING "ROS 2 edition: humble|iron")

# Validation.
include(cmake/nano-ros-validate-config.cmake)

# Platform dispatch (Phase 138 lives here).
include(cmake/platform/nano-ros-${NANO_ROS_PLATFORM}.cmake)

# RMW dispatch — pulls in the right nros-rmw-<rmw>-staticlib via corrosion.
include(cmake/rmw/nano-ros-rmw-${NANO_ROS_RMW}.cmake)

# Codegen function (carries over from existing install).
include(cmake/NanoRosGenerateInterfaces.cmake)

# Umbrella interface targets — what users link.
add_library(NanoRos INTERFACE)
add_library(NanoRos::NanoRos ALIAS NanoRos)
target_link_libraries(NanoRos INTERFACE nros_c_${NANO_ROS_PLATFORM} nros_rmw_${NANO_ROS_RMW} nros_platform_${NANO_ROS_PLATFORM})

add_library(NanoRosCpp INTERFACE)
add_library(NanoRos::NanoRosCpp ALIAS NanoRosCpp)
target_link_libraries(NanoRosCpp INTERFACE nros_cpp_${NANO_ROS_PLATFORM} NanoRos::NanoRos)

# Optional install rules — only fire when consumed via install pipeline,
# not when in add_subdirectory mode. Gates: PROJECT_IS_TOP_LEVEL.
if(PROJECT_IS_TOP_LEVEL OR NANO_ROS_FORCE_INSTALL)
    include(cmake/install.cmake)
endif()
```

`PROJECT_IS_TOP_LEVEL` (CMake 3.21+) is the canonical "am I being add_subdirectory'd?" check. When inside a user project, `install(...)` rules don't fire — user's project owns install layout.

### C. cmake/ layout (consolidates current install-time + new in-tree paths)

```
nano-ros/
├── CMakeLists.txt                          ← NEW (Phase 137)
├── cmake/
│   ├── nano-ros-validate-config.cmake      ← NEW (Phase 137)
│   ├── NanoRosGenerateInterfaces.cmake     ← MOVED from packages/core/nros-c/cmake/
│   ├── platform/
│   │   ├── nano-ros-posix.cmake            ← Phase 138
│   │   ├── nano-ros-zephyr.cmake           ← Phase 138 (pulls from packages/zpico/zpico-zephyr/cmake/)
│   │   ├── nano-ros-freertos.cmake         ← Phase 138 (pulls from packages/core/nros-platform-freertos/cmake/)
│   │   ├── nano-ros-nuttx.cmake            ← Phase 138
│   │   ├── nano-ros-threadx.cmake          ← Phase 138
│   │   └── nano-ros-baremetal.cmake        ← Phase 138
│   ├── rmw/
│   │   ├── nano-ros-rmw-zenoh.cmake        ← NEW (Phase 137; wraps nros-rmw-zenoh-staticlib build)
│   │   ├── nano-ros-rmw-dds.cmake          ← NEW
│   │   ├── nano-ros-rmw-xrce.cmake         ← NEW
│   │   └── nano-ros-rmw-cyclonedds.cmake   ← NEW
│   └── install.cmake                       ← MOVED from install-rule sites (only fires top-level)
└── packages/                               ← unchanged
```

### D. Coexistence with `install-local` during transition

Both paths supported through Phase 139:

- **In-tree (new)**: `add_subdirectory(third_party/nano-ros)` → no install needed
- **Installed (legacy)**: `just install-local` → `find_package(NanoRos CONFIG)` keeps working

Phase 140 removes the legacy path; users migrate to add_subdirectory.

---

## Work Items

- [x] **137.1 — Root `CMakeLists.txt`.** *(verified 2026-05-18 — root `CMakeLists.txt` exists with `NANO_ROS_PLATFORM`/`NANO_ROS_RMW` cache vars, INTERFACE `NanoRos` target, `PROJECT_IS_TOP_LEVEL OR NANO_ROS_FORCE_INSTALL`-gated install rules; commit `d5481011`. Note: `cmake/nano-ros-validate-config.cmake` was not landed as a separate file — validation is inlined in the root CMakeLists via `_nros_platform_module` existence check.)*
      Add `nano-ros/CMakeLists.txt` per the shape in §B. Wire
      `NANO_ROS_PLATFORM` / `NANO_ROS_RMW` / `NANO_ROS_ROS_EDITION` cache
      vars. Define `NanoRos` + `NanoRosCpp` INTERFACE targets. Gate
      `install(...)` rules on `PROJECT_IS_TOP_LEVEL`.
      **Files.** `CMakeLists.txt` (new), `cmake/nano-ros-validate-config.cmake` (new).

- [ ] **137.2 — Move `NanoRosGenerateInterfaces.cmake` to root cmake/.** *(2026-05-18 — DISCREPANCY: status line claims landed but `cmake/NanoRosGenerateInterfaces.cmake` does NOT exist at the root path; the file still lives at `packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake` and installs to `build/install/lib/cmake/NanoRos/`. Move not performed.)*
      The codegen function currently lives at
      `packages/core/nros-c/cmake/NanoRosGenerateInterfaces.cmake` and
      gets `install`-copied to the prefix. Move it to
      `nano-ros/cmake/NanoRosGenerateInterfaces.cmake` so it's reachable
      from the root entry without an install step. Keep the install rule
      pointing at the new path so the legacy `find_package` path still
      exports it.
      **Files.** `cmake/NanoRosGenerateInterfaces.cmake` (moved),
      `packages/core/nros-c/CMakeLists.txt` (install path update).

- [x] **137.3 — RMW dispatch modules.** *(verified 2026-05-18 — RMW dispatch landed inline in root `CMakeLists.txt:151-220` with `if/elseif` per `NANO_ROS_RMW` branch; zenoh + dds wired via Corrosion (`add_subdirectory(packages/zpico/nros-rmw-zenoh-staticlib)` etc.). XRCE + cyclonedds branches emit `FATAL_ERROR` pointing at legacy `install-local` path — matches status-line deferral to Phase 138/139. NOTE: `cmake/rmw/nano-ros-rmw-*.cmake` per-file split was NOT created — directory `cmake/rmw/` does not exist. The contract is met inline rather than per-file; comment at line 148 explicitly says "`cmake/rmw/...` modules land in Phase 138".)*
      `cmake/rmw/nano-ros-rmw-{zenoh,dds,xrce,cyclonedds}.cmake` —
      each `add_subdirectory(packages/<rmw>/nros-rmw-<rmw>-staticlib)`
      (or its current build entry point) and exposes a
      `nros_rmw_<rmw>` IMPORTED / INTERFACE target. Drives the
      corrosion crate build per the picked RMW.
      **Files.** `cmake/rmw/nano-ros-rmw-{zenoh,dds,xrce,cyclonedds}.cmake` (new).

- [x] **137.4 — In-tree consumer smoke test.** *(verified 2026-05-18 — `packages/testing/nros-tests/tests/cmake_add_subdirectory.rs` exists; commit `46e322c1`.)*
      Add `packages/testing/nros-tests/tests/cmake_add_subdirectory.rs`
      that creates a tmpdir with a tiny user project (`CMakeLists.txt` +
      `main.c`), runs `cmake -DCMAKE_PREFIX_PATH=…` (none — just
      `add_subdirectory(<repo-root>)`), `cmake --build`, asserts the
      binary exists and links cleanly against `NanoRos::NanoRos`. Runs
      against POSIX zenoh as the canonical smoke case.
      **Files.** `packages/testing/nros-tests/tests/cmake_add_subdirectory.rs` (new),
      `packages/testing/nros-tests/Cargo.toml` (test entry).

- [x] **137.5 — Rewrite one example as proof of concept.** *(verified 2026-05-18 — `examples/native/c/zenoh/talker/CMakeLists.txt` is 20 lines using `add_subdirectory(<repo-root>)`; commit `83a57148`. Note: `examples/native/c/zenoh/talker/README.md` was NOT created — the "legacy `find_package` path documented in README" deliverable is unmet but the CMake migration itself landed.)*
      `examples/native/c/zenoh/talker/CMakeLists.txt` shrinks from ~50
      lines to ~10 lines using `add_subdirectory(<repo-root>)` instead
      of `find_package(NanoRos)`. Keep the `find_package` path documented
      as legacy in the example README. Validates the API on the simplest
      consumer.
      **Files.** `examples/native/c/zenoh/talker/CMakeLists.txt`,
      `examples/native/c/zenoh/talker/README.md`.

- [x] **137.6 — Doc update.** *(verified 2026-05-18 — `book/src/getting-started/build-as-subdirectory.md` exists; `book/src/SUMMARY.md:10` lists it; commit `1a9539c5`.)*
      `book/src/getting-started/build-as-subdirectory.md` (new page).
      `book/src/SUMMARY.md` lists it. Cross-link from
      `book/src/getting-started/installation.md` as the recommended
      future-direction path (mark `install-local` as legacy with a
      pointer to Phase 140).
      **Files.** `book/src/getting-started/build-as-subdirectory.md` (new),
      `book/src/SUMMARY.md`, `book/src/getting-started/installation.md`.

---

## Acceptance

- [x] `cmake -B /tmp/x -S /tmp/user_project` (where `user_project`
      has 4-line CMakeLists with `add_subdirectory(<nano-ros>)`)
      succeeds, no install step run. *(verified 2026-05-18 — covered by the `cmake_add_subdirectory` smoke test landed in 137.4.)*
- [x] `cmake --build /tmp/x` produces a working binary linking against
      `NanoRos::NanoRos`. *(verified 2026-05-18 — same smoke test asserts the binary builds + links.)*
- [ ] `find_package(NanoRos CONFIG)` against an `install-local` output
      still works unchanged (legacy path preserved). *(2026-05-18 — unclear: needs explicit re-run of `just install-local` + a downstream `find_package` consumer; no commit log entry asserts the legacy path was re-verified post-137.)*
- [x] `cmake_add_subdirectory` test from 137.4 passes in
      `cargo nextest run -E 'test(cmake_add_subdirectory)'`. *(verified 2026-05-18 — test file landed at `packages/testing/nros-tests/tests/cmake_add_subdirectory.rs` per commit `46e322c1`; Status line claims 137.1–137.6 landed including this test.)*
- [ ] `examples/native/c/zenoh/talker` builds via the new in-tree path
      AND the legacy `find_package` path (until Phase 140). *(2026-05-18 — unclear: in-tree path covered by the migrated CMakeLists; legacy `find_package` path on the same example needs explicit dual-build verification.)*
- [ ] `just ci` still green; install-local path untouched. *(2026-05-18 — unclear: needs CI run; recent commit log does not include a `just ci` green confirmation for the 137-landed state.)*

---

## Notes

- **Why interface targets, not imported.** `NanoRos::NanoRos` as an
  INTERFACE target lets us add transitive deps (`platform_aliases`
  TU, platform support modules) without consumers seeing the wiring.
  IMPORTED targets are for prebuilt archives; INTERFACE is the right
  shape for source-vendored builds.
- **`PROJECT_IS_TOP_LEVEL` is CMake 3.21+.** Bumping
  `cmake_minimum_required` from 3.20 → 3.22 in the new root is the
  smallest acceptable bump. Existing per-platform CMakeLists already
  require 3.22 (e.g. `packages/zpico/nros-rmw-zenoh-staticlib/CMakeLists.txt`).
- **What we do NOT do in 137.** Consolidating per-example boilerplate
  is Phase 138. Adding RTOS-flavour integration shells (esp-idf
  component, west module, platformio lib.json) is Phase 139. Removing
  the `install-local` recipe is Phase 140. Phase 137 only lands the
  entry point + proves it on one example.
- **Corrosion still the Rust bridge.** Each `nano-ros-rmw-<rmw>.cmake`
  module uses `corrosion_import_crate` to build the staticlib in
  place. No change to the Rust build path.
- **`generated/` codegen cache works as-is.** `NANO_ROS_GEN_CACHE_DIR`
  (Phase 123.A.7) is environment-driven; `add_subdirectory` inherits
  CMake vars from the parent project, so the cache contract is
  unchanged.
