---
id: 2952
title: "On the CMake road an image's `log.max_level` reaches cargo only; a consumer's C/C++ code cannot derive its log floor from it"
status: open
type: enhancement
severity: low
area: [cmake, config, logging]
related: [phase-478, 2881, 2951, 1712]
found: 2026-11-24
---

## What is missing

On Zephyr an image's `log.max_level` becomes `CONFIG_NROS_LOG_MAX_LEVEL`, so a
ported package can pass the image's ceiling to the compile-time floors
issue 2881 added:
`-DRCLC_LOG_MIN_SEVERITY=CONFIG_NROS_LOG_MAX_LEVEL` (the numbering matches).

On the CMake road (FreeRTOS POSIX and AN536, S32Z2, linux census images)
there is no such value. The image's rows are written as a cargo config and
handed to cargo, and that is all:

- `packages/cli/nros-cli-core/src/cmd/leaf_system.rs`, `image_env_config` /
  `write_image_env`: render the image's layers (what `transport` implies,
  then `[image.<id>] env` and `[image.<id>.knobs]` as env rows) into a
  one-table cargo config, written by `nros dev image leaf-system
  --image-env-out`.
- `cmake/NanoRosPackageXml.cmake`, `nano_ros_read_leaf_system`: records that
  file as the GLOBAL property `NROS_IMAGE_ENV_CONFIG`, for the top-level
  project only (a workspace member gets a WARNING and nothing).
- `cmake/NanoRosImageEnv.cmake`: passes it as `--config=<file>` to every
  cargo command (`nros_image_env_attach`, `nros_image_env_cargo_flags`).

The configure itself never defines `NROS_LOG_MAX_LEVEL` as a cache variable
or a compile definition. The internal reader `nros_image_env_value` can pull
one row back out of the file, but it is not a documented consumer surface,
it returns the row's text as written (a level name is passed through
unchanged, issue 2951), and it is empty for a package that is not the
top-level project of its configure. Issue 2881's resolution records the gap:
"the CMake road hands image knobs to cargo only".

## Consumer impact

A consumer library that wants `RCLC_LOG_MIN_SEVERITY` /
`RCLCPP_LOG_MIN_SEVERITY` to follow the image (so that a call site the image
can never emit is compiled out in C/C++ as it already is in Rust) can do so on
Zephyr only. The island port (nano-ros pin
`55d9a6d761b64d4633d3ec6820ad37f68f2acfe7`) defaults its floor to INFO on
every CMake-road image, independent of the image's `log.max_level`, so an
image that states `warn` still compiles its INFO records in.

## Want

The image's resolved `log.max_level` exported to the configure on every
CMake platform, as an integer 0..6 with the `CONFIG_NROS_LOG_MAX_LEVEL`
numbering: for example a cache variable `NROS_LOG_MAX_LEVEL` plus a compile
definition of the same name on the nano-ros targets a consumer links
(`NanoRos::nros_c` / `nros_cpp`), so
`-DRCLC_LOG_MIN_SEVERITY=NROS_LOG_MAX_LEVEL` works the way the Zephyr
spelling does. Resolved as cargo resolves it (exported variable, then the
image row, then the board rung, then the builtin), so the floor and the
Rust ceiling cannot disagree. Workspace members of a census configure
included, since that is where the island's linux images build.

## Test

A cmake-road fixture image stating `log.max_level = "warn"` configures with
`NROS_LOG_MAX_LEVEL=3` visible to a consumer target; one stating nothing
gets the board's or the builtin value; the value matches the `MAX_LEVEL`
the `nros-log` build script generated for the same image.
