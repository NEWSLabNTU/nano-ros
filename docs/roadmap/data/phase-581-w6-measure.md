# Phase-581 W6 - measurements for the batch-2 renames

Measured 2026-11-20 in the `ros2` distrobox (Ubuntu 22.04, ROS 2 Humble), per
[phase-581 W6](../phase-581-ros-env-vars.md). "Before" is the W5 tip (the
commit that closed W5's second change), checked out as a scratch worktree
under `/mnt/wd/nt/` and removed afterwards; "after" is the W6 working tree.
Each side ran its own `just setup-cli` (and, before, `just
setup-launch-resolve`), then built the same images with no knob stated, so
every value below is a default or a derivation.

## `.config` (Zephyr 3.7, `native_sim/native/64`)

Two images, so that the zenoh symbols and the Cyclone symbols are both
present: `just zephyr build-one c/talker` (`nros_zephyr_talker_c_bringup:zephyr_zenoh`)
and `just zephyr build-one c/talker cyclonedds` (`...:zephyr_cyclonedds`). The
comparison maps each renamed symbol to its new name and then compares every
`CONFIG_*` line, value for value.

| Image | Lines before / after | Symbols before / after | Renamed symbols present | Differences after mapping |
| --- | --- | --- | --- | --- |
| zenoh | 1909 / 1907 | 1327 / 1325 | 13 | 0 |
| cyclonedds | 1864 / 1864 | 1282 / 1282 | 9 | 0 |

The two lines the zenoh image loses are the two hand symbols W6 removes
instead of renaming: `CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE=0` (the deprecated
alias of the sched-task stack, whose `0` meant "unset") and `# CONFIG_NROS_TRANSPORT_SERIAL is not set`.
Neither changes any other symbol: the stack it aliased stays 16384, and the
serial link stays off.

| Before | After | zenoh | cyclonedds |
| --- | --- | --- | --- |
| `CONFIG_NROS_ZENOH_LOCATOR` | `CONFIG_NROS_LOCATOR` | `"tcp/127.0.0.1:7447"` | - |
| `CONFIG_NROS_CYCLONE_DOMAIN_ID` | `CONFIG_NROS_CYCLONEDDS_DOMAIN_ID` | - | 0 |
| `CONFIG_NROS_CYCLONE_CONFIG_XML` | `CONFIG_NROS_CYCLONEDDS_CONFIG_XML` | - | `""` |
| `CONFIG_NROS_EPOCH_FROM_ZENOH` | `CONFIG_NROS_ZENOH_WALL_CLOCK_EPOCH` | n | - |
| `CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE` | `CONFIG_NROS_ZEPHYR_TASK_STACK_BYTES` | 16384 | 16384 |
| `CONFIG_NROS_ZEPHYR_SCHED_TASK_STACK_SIZE` | `CONFIG_NROS_ZEPHYR_SCHED_TASK_STACK_BYTES` | 16384 | - |
| `CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME` | `CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME_BYTES` | 13312 | - |
| `CONFIG_NROS_PARAM_STORE` | `CONFIG_NROS_PARAMS_STORE` | n | n |
| `CONFIG_NROS_MAX_ARRAY_LEN` | `CONFIG_NROS_PARAMS_MAX_ARRAY_LEN` | -1 | -1 |
| `CONFIG_NROS_MAX_PARAMETERS` | `CONFIG_NROS_PARAMS_MAX_PARAMETERS` | -1 | -1 |
| `CONFIG_NROS_MAX_STRING_ARRAY_LEN` | `CONFIG_NROS_PARAMS_MAX_STRING_ARRAY_LEN` | -1 | -1 |
| `CONFIG_NROS_PARAM_EVENTS_DEPTH` | `CONFIG_NROS_PARAMS_EVENTS_DEPTH` | 1 | 1 |
| `CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES` | `CONFIG_NROS_PARAMS_SERVICE_INBOX_BYTES` | -1 | -1 |
| `CONFIG_NROS_PARAM_SERVICE_INBOX_DEPTH` | `CONFIG_NROS_PARAMS_SERVICE_INBOX_DEPTH` | 1 | - |
| `CONFIG_NROS_GRAPH_MAX_ENTITIES` | `CONFIG_NROS_ZENOH_GRAPH_MAX_ENTITIES` | 0 | - |

The other renamed hand symbols (`..._EPOCH_FROM_ZENOH_INTERVAL_MS`,
`..._CONSOLE_WHOLE_LINES_BUFFER`, `..._EDMA_WARM_RESET_QUIESCE`) depend on
options neither image enables, so neither `.config` holds them on either side.

## A guest Zephyr project refuses an old symbol

A plain Zephyr application with no `system.toml` (a `main.c`, a
`CMakeLists.txt` that calls `find_package(Zephyr)`, and a `prj.conf` holding
`CONFIG_NROS=y` and `CONFIG_NROS_CYCLONE_CONFIG_XML=""`), configured with
`west build -b native_sim/native/64 ... -- -DZEPHYR_EXTRA_MODULES=<this checkout>`
against the Zephyr 3.7 workspace, stops before Kconfig runs:

```
CMake Error at <checkout>/zephyr/cmake/nros_image_kconfig.cmake:178 (message):
  nano-ros: a retired nano-ros setting is set:

    <app>/prj.conf: CONFIG_NROS_CYCLONE_CONFIG_XML is retired: set CONFIG_NROS_CYCLONEDDS_CONFIG_XML instead
```

The generated case list (`tests/zephyr-transient-overrides-tests.sh`, case G)
covers every retired Kconfig symbol, the 12 hand symbols and the locator
statement's included.

## `NROS_DOMAIN_ID` exported into a FreeRTOS fixture build

`NROS_DOMAIN_ID=7 bash scripts/build/workspace-fixtures-build.sh freertos rust --id freertos-rust-zenoh-talker`
stops at `nros configure`, exit 1:

```
Error: a retired build variable is set in this shell:
  NROS_DOMAIN_ID is retired at build time: state `domain_id` under `[system]` (or `[image.<id>]`) in system.toml (it reaches the build as NROS_ENTRY_DOMAIN_ID), and ROS_DOMAIN_ID overrides it at run time on a target with a process environment
```

A build `nros` did not start stops in `nros-node`'s build script with the same
sentence (`NROS_DOMAIN_ID=5 cargo build -p nros-node`). The same row with
nothing exported builds (`mps2_an385_freertos_entry`, 130 s).

## `NROS_LOCATOR` exported into a linux build, then at run time

`NROS_LOCATOR=tcp/127.0.0.1:7461 bash scripts/build/workspace-fixtures-build.sh linux rust --id native-rust-talker`
builds and prints one note:

```
warning: nros-node@0.6.0: note: NROS_LOCATOR=tcp/127.0.0.1:7461 is set in this build's shell. The build does not read it: it is read when a hosted image RUNS. To bake a value, state `locator` in system.toml (carried to the build as NROS_ENTRY_LOCATOR).
```

Run with a router listening on 7461 (`nros_router_exec tcp/127.0.0.1:7461`)
and `NROS_LOCATOR=tcp/127.0.0.1:7461`, the image opens its session there and
publishes:

```
[INFO] nros: boot config: domain 0 (baked), locator tcp/127.0.0.1:7461 (env), rmw (unique) (default)
[INFO] nros: session open
[INFO] Publishing: 'Hello World: 1'
```

The negative control, `NROS_LOCATOR=tcp/127.0.0.1:7462` with nothing
listening there, reports `locator tcp/127.0.0.1:7462 (env)` and fails to open
the session, so the value the image dialled is the variable's.
