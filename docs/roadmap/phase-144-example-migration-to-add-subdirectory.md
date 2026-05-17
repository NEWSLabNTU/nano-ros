# Phase 144 — Migrate All C/C++ Examples to `add_subdirectory`

**Goal.** Replace every `find_package(NanoRos REQUIRED CONFIG)` in
`examples/**/CMakeLists.txt` (85 occurrences as of 2026-05-18) with
the Phase 137 `add_subdirectory(<repo-root>)` consumption shape.
Per-example helpers under `examples/**/cmake/<plat>-support.cmake`
get deleted in lock-step (Phase 138.4's narrowed scope completes
here per-example).

**Status.** 1 of 86 examples migrated (Phase 137.5:
`examples/native/c/zenoh/talker/`). 85 outstanding.

**Priority.** P1 — blocks Phase 140 (`install-local` rip-off cannot
remove `find_package(NanoRos)` while 85 consumers depend on it).

**Depends on.** Phase 137 (root `add_subdirectory` entry point),
Phase 138 (per-platform CMake modules), Phase 139 (RTOS shells —
where useful for the RTOS-specific examples). All landed.

**Related.** Phase 140 (rip-off — Phase 144 is the prerequisite),
Phase 138.4 (per-example helper deletion narrowed to "delete when
example migrates" — Phase 144 triggers each deletion).

---

## Overview

Phase 137.5 migrated one example end-to-end as proof of concept.
The remaining 85 examples still consume nano-ros via
`find_package(NanoRos REQUIRED CONFIG)` against a pre-installed
`build/install/lib/cmake/NanoRos/`. That contract breaks the moment
Phase 140 deletes `install-local`.

Phase 144 grinds through the example tree, swapping each
`find_package` for `add_subdirectory` per the Phase 137.5 pattern.
Per-example boilerplate (per-platform helper includes, NANO_ROS_*
cache var setup) gets standardised against the Phase 138 platform
modules; per-example `cmake/<plat>-support.cmake` helpers get
deleted as their consumer migrates.

### Distribution (2026-05-18 snapshot)

```
35  examples/native/{c,cpp}/{dds,xrce,zenoh}/<example>/
12  examples/threadx-linux/{c,cpp}/zenoh/<example>/
12  examples/qemu-riscv64-threadx/{c,cpp}/zenoh/<example>/
12  examples/qemu-arm-nuttx/{c,cpp}/zenoh/<example>/
12  examples/qemu-arm-freertos/{c,cpp}/zenoh/<example>/
 2  examples/templates/multi-package-workspace/src/pkg_{c_talker,cpp_listener}/
─────
85  total
```

Rust examples (`examples/*/rust/`) use `[patch.crates-io]` in
`.cargo/config.toml`, NOT `find_package`. They're unaffected by
Phase 144; their Phase-140 migration is a separate concern (drop
the patch table entries that point at the install-local-built
sources once those sources move).

---

## Architecture

### A. The migration pattern (per Phase 137.5)

Before (typical native C example):

```cmake
cmake_minimum_required(VERSION 3.16)
project(c_talker VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

find_package(NanoRos REQUIRED CONFIG)

nros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

add_executable(c_talker src/main.c)
target_link_libraries(c_talker PRIVATE std_msgs__nano_ros_c NanoRos::NanoRos)

install(TARGETS c_talker RUNTIME DESTINATION bin)
```

After (Phase 137.5 shape):

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker VERSION 0.1.0 LANGUAGES C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW     zenoh)
add_subdirectory(../../../../../ nano_ros)

nano_ros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nano_ros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

add_executable(c_talker src/main.c)
target_link_libraries(c_talker PRIVATE std_msgs__nano_ros_c NanoRos::NanoRos)
nros_platform_link_app(c_talker)
```

Notable changes:

- `find_package(NanoRos REQUIRED CONFIG)` →
  `set(NANO_ROS_PLATFORM …) + set(NANO_ROS_RMW …) +
  add_subdirectory(<repo-root>)`
- `nros_generate_interfaces` → `nano_ros_generate_interfaces`
  (same function; canonical name post-138)
- `nros_platform_link_app(target)` call from Phase 138 — per-app
  fixup hook (no-op for POSIX; per-board for bare-metal)
- `cmake_minimum_required` bumped 3.16 → 3.22 to match root

The relative path depth varies per example (`../../../../../` for
5-deep `examples/native/c/zenoh/talker/`, `../../../../` for
4-deep, etc.). Per-example arithmetic; not generalisable.

### B. RTOS-shell-using examples

Examples under `qemu-arm-{freertos,nuttx}`, `qemu-riscv64-threadx`,
`threadx-linux` could in principle consume via their Phase 139
integration shell. But the shells (`integrations/zephyr/`,
`integrations/nuttx/`, etc.) are designed for EXTERNAL users — they
expect to be discovered by `west update` / `idf.py
add-dependency` from a downstream workspace. Internal examples
have direct access to the repo root and can just
`add_subdirectory(<repo-root>)` straight.

So in-tree examples use raw `add_subdirectory` (137.5 pattern);
external users use the 139 shells. Both flows hit the same root
`CMakeLists.txt`.

### C. Per-example helper deletion (Phase 138.4 follow-through)

When an example migrates, its `examples/<plat>/<example>/cmake/`
helpers (if any) get deleted. Phase 138 already provides
equivalents at `cmake/platform/nano-ros-<plat>.cmake` +
`cmake/board/nano-ros-board-<board>.cmake`. The per-example
helpers were redundant the moment Phase 138 landed; Phase 144 ships
the actual deletions per-example.

### D. Migration as bisectable commits

One commit per `<plat>/<lang>/<rmw>` group (12-35 examples) so
`git bisect` localises any regression to a small set. Within a
group, every example shares the same relative path depth + same
platform module, so the diff is mechanical.

---

## Work Items

Grouped by platform (most homogeneous within group):

- [ ] **144.1 — `examples/native/c/zenoh/*` (9 examples).**
      All consume POSIX + zenoh. Same depth 5, same modules.
      **Files.** 9 `CMakeLists.txt` files.

- [ ] **144.2 — `examples/native/c/dds/*` (6 examples).**
      POSIX + dds. Same depth.
      **Files.** 6 `CMakeLists.txt` files.

- [ ] **144.3 — `examples/native/c/xrce/*` (6 examples).**
      POSIX + xrce. Same depth. NOTE: Phase 137 left an
      `xrce` FATAL_ERROR stub in root CMake — coordinate with the
      Phase 137.X follow-up that fills it in (Phase 138/139 punted
      to user-action).
      **Files.** 6 `CMakeLists.txt` files.

- [ ] **144.4 — `examples/native/cpp/{zenoh,dds}/*` (14 examples).**
      POSIX + cpp. Uses `NanoRos::NanoRosCpp` target.
      **Files.** 14 `CMakeLists.txt` files.

- [ ] **144.5 — `examples/qemu-arm-freertos/{c,cpp}/zenoh/*` (12 examples).**
      `NANO_ROS_PLATFORM=freertos` + `NANO_ROS_BOARD=mps2-an385-freertos`.
      Phase 138.3 punted this board overlay — land it here under 144.5
      so the examples have a target.
      **Files.** 12 `CMakeLists.txt` files, possibly
      `cmake/board/nano-ros-board-mps2-an385-freertos.cmake` (new).

- [ ] **144.6 — `examples/qemu-arm-nuttx/{c,cpp}/zenoh/*` (12 examples).**
      `NANO_ROS_PLATFORM=nuttx` + `NANO_ROS_BOARD=nuttx-qemu-arm`.
      Phase 138.3 punted this board overlay too.
      **Files.** 12 `CMakeLists.txt` files, possibly
      `cmake/board/nano-ros-board-nuttx-qemu-arm.cmake` (new).

- [ ] **144.7 — `examples/qemu-riscv64-threadx/{c,cpp}/zenoh/*` (12 examples).**
      `NANO_ROS_PLATFORM=threadx` + `NANO_ROS_BOARD=riscv64-qemu`.
      Board overlay exists (138.3); only example migration needed.
      **Files.** 12 `CMakeLists.txt` files.

- [ ] **144.8 — `examples/threadx-linux/{c,cpp}/zenoh/*` (12 examples).**
      `NANO_ROS_PLATFORM=threadx` + `NANO_ROS_BOARD=threadx-linux`.
      Phase 138.3 punted this board overlay.
      **Files.** 12 `CMakeLists.txt` files, possibly
      `cmake/board/nano-ros-board-threadx-linux.cmake` (new).

- [ ] **144.9 — `examples/templates/multi-package-workspace/src/{pkg_c_talker,pkg_cpp_listener}/` (2 examples).**
      Pattern A workspace template. These intentionally model the
      external-user shape; consider whether they should switch to
      `add_subdirectory(../../../..)` or stay on `find_package` to
      demonstrate that path. Decision in 144.9.
      **Files.** 2 `CMakeLists.txt` files (decision pending).

- [ ] **144.10 — Per-example helper sweep (138.4 close-out).**
      `find examples -path '*/cmake/*-support.cmake'` lists every
      per-example helper. After 144.1-144.8, run the deletion +
      verify no migrated example needs them.
      **Files.** `examples/**/cmake/` (deleted).

- [ ] **144.11 — Per-example smoke build.**
      For each `<plat>/<lang>/<rmw>` group, spot-build at least one
      example end-to-end on the dev box (POSIX always; cross-platform
      where toolchains present). Skip cleanly when toolchain absent.
      Catch any per-example `target_*` quirk before the group commit
      lands.
      **Files.** none (verification step).

- [ ] **144.12 — Drop `find_package(NanoRos)` from CLAUDE.md
      example-template guidance.**
      CLAUDE.md "## Practices" + "### Examples = Standalone Projects"
      reference `find_package` patterns. Update wording to the
      `add_subdirectory` shape.
      **Files.** `CLAUDE.md`.

---

## Acceptance

- [ ] `git grep -lE 'find_package\s*\(\s*NanoRos' examples/` returns
      empty (or only the 2 templates if 144.9 keeps them).
- [ ] Every migrated example builds via `cmake -B <bld> -S . &&
      cmake --build <bld>` from the example dir, no `CMAKE_PREFIX_PATH`
      flag needed.
- [ ] No example references `build/install/lib/cmake/NanoRos`.
- [ ] `find examples -path '*/cmake/*-support.cmake'` returns empty
      (138.4 close-out).
- [ ] `just ci` green — examples still build via per-platform CI lanes.

---

## Notes

- **Why per-group commits, not per-example.** 85 individual commits
  is noise. 8 group commits (144.1-144.8) gives bisect granularity
  matching the homogeneity within each group (same platform, same
  path depth, same template).
- **The board-overlay work bundled into 144.5/.6/.8.** Phase 138.3
  punted four bare-metal-board overlays (`mps2-an385-freertos`,
  `nuttx-qemu-arm`, `threadx-linux`, plus `esp32-qemu` not needed
  here). 144 lands them as the examples migrate — minimum-deletion
  principle from Phase 138's note "Add them per-board when the
  corresponding example migrates."
- **Two-templates decision (144.9).** The
  `multi-package-workspace` template intentionally models the
  external-user `find_package` workflow because that's how a
  downstream Pattern A workspace consumes a packaged release.
  Phase 140 will need to rewrite this template anyway to model the
  post-rip-off workflow. Defer the decision to 144.9 — likely
  outcome: keep as `find_package` demo until Phase 140, then
  rewrite as `add_subdirectory` demo (which is `examples/native/c/zenoh/talker/`
  redux, so maybe delete the template entirely).
- **Rust examples deliberately out of scope.** They consume
  nano-ros via `[patch.crates-io]` in `.cargo/config.toml`, not via
  CMake. Phase 144 is the C/C++ migration only. Rust-side
  `install-local` consumers (if any) get addressed in Phase 140's
  audit step (140.1).
