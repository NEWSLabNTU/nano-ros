---
id: 2248
title: "Every `nros build` of a west image re-ran west's CMake configure"
status: resolved
type: bug
area: [build, zephyr, configure]
found: 2026-11-04
related: [phase-530, rfc-0120]
---

## Symptom

A second `nros build zephyr` in `examples/workspaces/rust`, with nothing
changed, ran west's whole CMake configure again ("-- west build: generating
a build system", one `-- Configuring done`). A plain `west build -d <dir>` of
the same build dir re-ran nothing. RFC-0120 D3 has `nros build` configure like
cargo, so a no-op must be 0 configures.

## Cause

`cmd/build.rs` appended `--` plus the image's cmake options (`EXTRA_CONF_FILE`,
`-DNROS_IMAGE`, the module and `nano_ros_ROOT`, the board heap and rx args) to
every `west build` handoff. `west build` re-runs CMake whenever it is handed
cmake options, whatever their values.

## Fix

When the build dir's `CMakeCache.txt` already holds every one of those
`-D<NAME>=<VALUE>` options with the same value, the handoff passes none, and
ninja alone decides whether to re-configure, exactly as on a plain
`west build -d <dir>`. Any other argument, or a value spelled differently,
still passes them all. `EXTRA_CONF_FILE` is compared after dropping the
entries the module hook itself appends on every configure (the image's
`NROS_IMAGE_CONF_FILES` and its `<build>/nros/` rendering), which it strips
and re-adds idempotently in `zephyr/cmake/nros_image_kconfig.cmake`
(`cmake_args_already_cached`, unit test
`cmake_args_already_cached_only_when_every_value_matches`).

## Measured

`nros build zephyr_zenoh` on `examples/zephyr/cpp/talker` (build
`z-cpp-nrosbuild-r48`, 2026-11-05):

| build | configures | re-runs |
| --- | --- | --- |
| first (T0) | 2 (census host build, then the image) | 0 |
| second, nothing changed (T1) | **0** (was 1 before the fix) | 0 |

The knobs that reach the compiler are unchanged: `NROS_EXECUTOR_MAX_CBS=1`,
`NROS_MAX_PUBLISHERS=1`, `NROS_MAX_SUBSCRIBERS=0`. Recorded in the phase-530 W7
acceptance table.
