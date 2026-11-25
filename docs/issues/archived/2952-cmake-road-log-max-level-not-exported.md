---
id: 2952
title: "On the CMake road an image's `log.max_level` reaches cargo only; a consumer's C/C++ code cannot derive its log floor from it"
status: resolved
type: enhancement
severity: low
area: [cmake, config, logging]
related: [phase-478, 2881, 2951, 1712]
found: 2026-11-24
resolved: 2026-11-25
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

## Fix

`cmake/NanoRosLogLevel.cmake`, `nros_log_max_level_export(NanoRos)`, called
from the checkout's `CMakeLists.txt` on every non-Zephyr platform (the
`find_package(nano_ros)` road adds that same directory):

- **Value.** `nros config explain --get log.max_level` (new `--get`) prints
  the index from `PlatformsTree::resolve_log`, the same ladder the `nros-log`
  build script reads: exported variable, image row, board `[knobs.log]`,
  platform, builtin 0. The configure asks it with the inputs cargo gets: the
  board facts' `NROS_PLATFORM_NAME`, `NROS_BOARD_TOML` and
  `NROS_PLATFORMS_DIR`, and the image row (from the image-env file) as the
  env rung when nothing is exported. No platform resolved: a stated number,
  else 0; a stated name is refused rather than guessed.
- **Export.** `NROS_LOG_MAX_LEVEL` as a CACHE INTERNAL (an output) and as an
  INTERFACE compile definition on `NanoRos::NanoRos`, which
  `NanoRos::NanoRosCpp` reaches. (The issue named `nros_c` / `nros_cpp`; the
  umbrella is what a consumer links.)
- **Freshness (issue 1018).** The CLI and the board descriptor are configure
  dependencies; `system.toml` already was (`nano_ros_read_leaf_system`).
- **Workspace members.** One configure builds `nros-log` once, so a member
  gets that build's value; its own rows are not claimed, as the existing
  warning says. The island's images each have their own configure and one
  level, so this costs them nothing (NXP 175, 2026-11-25).

Documented in `docs/reference/c-api-cmake.md` and the rclcpp compatibility
table.


## Test

- `scripts/check/check-cmake-log-level-export.sh`
  (`just check cmake-log-level-export`), a real configure against a stub
  resolver: the board's level lands in the cache and the definition, and the
  resolver is asked for the board facts' platform and board (T1); editing the
  board descriptor re-exports on `cmake --build` (T2, mutation-tested: drop
  the configure dependency and it fails); the image row reaches the resolver
  (T3); an exported variable outranks it (T4); with no platform a number is
  taken and the builtin is 0 (T5), and a name is refused (T6).
- `config::tests::a_board_log_level_resolves_through_the_ladder`: a board
  descriptor's `warn` / `error` resolve to 3 / 4 through `resolve_ladder`;
  `ladder_get_prints_the_log_level_as_its_index` pins `--get`'s index.
