---
id: 2363
title: "A west_overrides knob removed from a fixture row stays in the build dir's CMakeCache, so the transient override persists"
status: open
type: bug
area: [zephyr, cmake, testing]
severity: medium
found: 2026-11-08
related: [2362]
---

**Home: [phase-531 W6](../roadmap/phase-531-workspace-first-project-layout.md)**, found in batch 8.

RFC-0103 D2 makes a knob override from the command line
(`-DNROS_OVERRIDE_<ID>=<v>`, a fixture row's `west_overrides`) **transient**:
it belongs to that configure and nothing else. On an incremental build it is
not transient. CMake caches every `-D`, so once a row stops passing the
override, the old value is still in the build directory's `CMakeCache.txt`
and the image keeps building with it.

## Repro (measured 2026-11-08)

1. Build `zephyr-cortex-m-c-boot-report-zenoh` with
   `west_overrides = { EXECUTOR_ARENA_SIZE = "32" }`.
2. Remove that line from `examples/fixtures.toml` (issue 2362 did this to
   park the row).
3. Rebuild that row:
   `NROS_ZEPHYR_FIXTURE_FILTER='^build-cortex-m-c-boot-report' just zephyr build-fixtures`.

What happens:

- `<build>/CMakeCache.txt` still holds
  `NROS_OVERRIDE_EXECUTOR_ARENA_SIZE:UNINITIALIZED=32`.
- The configure still prints `knob override(s) from the command line:
  EXECUTOR_ARENA_SIZE (transient; RFC-0103 D2)`.
- The build fails exactly as before the edit, in the arena oracle at 32 B.

The row signature (`overrides=` in `zephyr-fixture-leaves.sh`) does change,
so the emitter knows the inputs moved. Nothing then drops the stale cache
entry.

## Fix direction

When the module reads `NROS_OVERRIDE_*`, it should also clear every cached
override the current command line did not pass. The nros module already does
this kind of bookkeeping for `EXTRA_CONF_FILE`, through
`NROS_IMAGE_CONF_FILES` in `zephyr/cmake/nros_image_kconfig.cmake`. The
other way is to have the fixture runner pass `-U NROS_OVERRIDE_*` before its
`-D`s. Do NOT wipe the build directory: a re-configure is the escape hatch
(CLAUDE.md, the `rm -rf` antipattern). The workaround is
`cmake -U NROS_OVERRIDE_<ID> <build-dir>`.

Acceptance: the repro above builds with the derived arena, and a test pins
it (configure with an override, then without, and the second configure
reports none).
