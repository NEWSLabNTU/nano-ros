---
id: 2073
title: "A ported executable on a board has no parameter source: argv is null, so -p and --params-file cannot reach it"
status: resolved
type: limitation
area: [cpp, params, cmake, boards]
severity: medium
found: 2026-10-26
related: [2057, 2043, phase-518, phase-482]
resolved: 2026-10-26
resolved_in: "PR #2700 (2026-10-26)"
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

## Fix (2026-10-26)

An image states its command line in `system.toml`:

```toml
[image.<id>]
params_files = ["config/x.param.yaml"]
ros_args = ["--ros-args", "-p", "update_rate:=10.0"]
```

Both keys fold `[image_defaults]` first (`nros_orchestration_ir::leaf_system`).
The CLI renders them into one image env row, `NROS_CPP_BAKED_ROS_ARGS`
(`leaf_settings::baked_ros_args`): every parameter file as `--ros-args
--params-file <name>`, then `ros_args`, plus each file's text, all hex-encoded.
That row rides the carrier every road already hands cargo (the cmake road's
image env file, the Zephyr module's `--config` file, the cargo road's
`nros-cargo.toml`). `nros-cpp/build.rs` bakes it as two constants.

`nros_cpp_install_argv_remaps` puts the baked arguments right after the
program name, so a host's real command line follows them and still wins, as a
later argument does in rcl. A `--params-file` whose name was baked is read
from the baked text on every target, which on a board is the only copy.
`rclcpp::init(argc, argv)` and `rclcpp::init()` reach it whenever the image
baked anything, with or without a `--ros-args` on the command line
(`nros_cpp_baked_ros_args_present`). A configure re-runs when a baked file
changes (`NROS_LEAF_ROS_ARGS_INPUT`, both the cmake road and the Zephyr
module).

Measured on phase-518 node 1 (`autoware_external_cmd_selector`, Linux, the
add_subdirectory road): with `params_files` naming its
`external_cmd_selector.param.yaml` and no command line, the node constructs
and exits 0 in census mode. The same sources without it throw
`unknown selector mode` from the constructor.

### Still open

- A C program (`rclc`) does not read the row: only the C++ runtime bakes it.
- The arguments are per IMAGE. A workspace with several executables in one
  image gets the same vector in each, which `--params-file`'s node keys
  (`<node>: ros__parameters:`) already scope.
