---
id: 2073
title: "A ported executable on a board has no parameter source: argv is null, so -p and --params-file cannot reach it"
status: open
type: limitation
area: [cpp, params, cmake, boards]
severity: medium
found: 2026-10-26
related: [2057, 2043, phase-518, phase-482]
---

## What

On a host, a ported program gets its parameters the way upstream's does:
`--ros-args -p name:=value` or `--params-file <yaml>` on the command line.
`Executor::install_argv_params` (phase-482 W5) applies them when the node
declares each parameter.

On a board, `ROS2_MAIN` renames the program's `main` to `nros_ported_main`, and
the board's start-up calls it with `argc = 0, argv = NULL`
(`NROS_APP_MAIN_REGISTER_VOID` / `_ZEPHYR` in `rclc/detail/app_main.h`).
Nothing else supplies parameters to an image built this way:

- the `[[component]] params` / `params_files` of `system.toml` reach a
  launch-built entry (`nano_ros_entry(... LAUNCH ...)`), not a ported `main`;
- there is no knob that bakes an argument vector into the image.

So a `declare_parameter<T>(name)` with no default can only read the type's zero
value. Upstream throws `ParameterUninitializedException` there, and nano-ros
aborts.

## How it showed

Phase 518 node 1 (`autoware_external_cmd_selector`) declares `update_rate` and
`initial_selector_mode` with no default. Autoware's launch passes both from a
YAML file. On FreeRTOS the node got `update_rate = 0.0`, and
`rclcpp::Rate(0.0)` gave a timer that could not run. The porter had to write
defaults into the node's source.

## Direction

Bake the arguments at build time and hand them to `nros_ported_main`, for
example `[image.<id>] ros_args = ["--ros-args", "-p", "update_rate:=10.0"]` or
`params_files = ["config/x.param.yaml"]` in the leaf's `system.toml`. The
values would be embedded the way a launch-built entry's parameters are. The
store and the override path already exist; only the carrier is missing.
