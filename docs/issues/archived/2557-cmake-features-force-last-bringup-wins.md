---
id: 2557
title: "Two bringups in one CMake workspace each FORCE-write NROS_FEATURES, so the last configure's capability set wins for every entry"
status: resolved
type: bug
area: [cmake, codegen, workspace]
severity: low
found: 2026-11-14
resolved_in: "fix(issue 2557): an entry of another bringup is refused unless the configure carries its capability axes"
related: [2556, 0353, phase-555]
---

Filed from RFC-0066's open questions by phase-555 W3 (ruling A1).

## Measured (phase-331 W2, 2026-08-02)

With three bringups in the cpp workspace, `managed_bringup`'s empty capability
set erased `param_services` and `lifecycle` for the whole workspace. It was
worked around by declaring the union in every bringup. Issue 0353 had fixed
the single-bringup path only.

## Still in the code (read 2026-11-14, not re-measured)

Both generators of the declared capability axes write the cache entry with
`FORCE`:

- `packages/cli/nros-cli-core/src/cmd/codegen_system.rs`
  (`nros dev codegen system`):
  `set(NROS_FEATURES "…" CACHE STRING "nano-ros capability axes" FORCE)`
- `packages/cli/nros-cli-core/src/cmd/config.rs`
  (`nros config show --format cmake`): the same line.

A `FORCE`d cache write is last-writer-wins per configure. Whether a
multi-bringup workspace still reaches two of them in one configure was not
re-measured for this filing.

## Done when

A configure of a workspace with two bringups that declare different
capabilities either gives each entry its own system's set or refuses, naming
both bringups. A test builds that workspace and asserts it.

## Resolution (2026-11-18)

Measured against main before fixing:

- **The generated road cannot reach two writers in one configure.**
  `nros build` gives each bringup its own cmake root
  (`cmake_build_dir_for` includes the bringup), and `nano_ros_workspace()`
  takes one `SYSTEM`. Nothing in the tree `include()`s codegen-system's
  `system_config.cmake`. So the two `FORCE` writes each run once per
  configure, for that configure's one bringup, and stay as they are.
- **A hand-written root still could.** `nano_ros_entry(BRINGUP <other> …)`
  under `nano_ros_workspace(SYSTEM <one>)` built the other bringup's entry
  against the first bringup's `NROS_FEATURES`, and nothing compared them.

The fix is one agreement check, shared with the west road:

- `nros_axes_missing()` (`cmake/NanoRosEntryAgreement.cmake`) is issue 1261's
  rule in one place: a declared axis the library build lacks is refused, and
  a superset is allowed. `nros_resolve_image_features` (the west road) now
  calls it instead of its own loop.
- `nano_ros_workspace()` records which bringup the configure's libraries are
  built for.
- `nano_ros_entry(BRINGUP …)` calls `nros_entry_axes_agree`. The same
  bringup, compared by path, returns at once with no CLI call. Another
  bringup's axes are read through the CLI's one accessor
  (`nros config show --format cmake`), and a missing axis is refused, naming
  both bringups, the missing axes and both fixes (its own build directory,
  or the axes declared in the configure's bringup).

Test: `tests/cmake-workspace-system-required-tests.sh` I–L, in
`just check workspace-system-required`.
- I: the entry is the configure's own bringup.
- J: another bringup needs `lifecycle` and is refused, naming both.
- K: another bringup's axes are covered by the configure.
- L: a real configure of `SYSTEM demo_bringup` refuses `extra_bringup`'s
  entry.
Mutation-checked: without the recorded property, L fails.
