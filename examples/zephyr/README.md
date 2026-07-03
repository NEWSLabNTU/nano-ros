# examples/zephyr — Zephyr RTOS examples

C, C++ and Rust examples built with `west`. Just module: **`zephyr`**
(`just/zephyr.just`, recipes in `just/zephyr-{setup,ci,dev}.just`).

## Prerequisites

```sh
source ./activate.sh
just zephyr setup             # west workspace (large download) + sources + patches
```

Rust examples additionally need ROS 2 sourced (`source /opt/ros/<distro>/setup.bash`)
so `nros` can generate the interface crates before the west build.

## RMW selection

Kconfig overlay, not a Cargo/CMake flag: `-DCONF_FILE="prj.conf;prj-<rmw>.conf"`
with `prj-zenoh.conf`, `prj-xrce.conf`, `prj-cyclonedds.conf` shipped per
example. The `build-one` recipe wires this for you.

## Build & run one example

```sh
just zephyr build-one cpp/talker zenoh            # board default: native_sim/native/64
just zephyr build-one rust/listener xrce
just zephyr build-one c/talker cyclonedds

# run a native_sim binary (zenoh: start `just native zenohd` first)
just zephyr talker            # = ./build-talker/zephyr/zephyr.exe --seed=$RANDOM
```

Copy-out check: `just zephyr check-copy-out <lang>/<case> <rmw> [board]`.
Test lanes: `just zephyr test`, `test-all`, `test-xrce`.

## Cases

| Role | c | cpp | rust |
| --- | --- | --- | --- |
| talker / listener | yes | yes | yes |
| service-server / service-client | yes | yes | yes |
| action-server / action-client | yes | yes | yes |
| carve-out | – | `cyclonedds/talker-aemv8r` (FVP AEMv8-R) | `cyclonedds/talker-aemv8r` |

Backends: zenoh + xrce across all six roles per language; cyclonedds partial
(see the [coverage matrix](../README.md)).

## Gotchas

- Zephyr POSIX needs `CONFIG_MAX_PTHREAD_MUTEX_COUNT=32` /
  `CONFIG_MAX_PTHREAD_COND_COUNT=16` (zenoh-pico exhausts the default 5) —
  already set in the shipped `prj-zenoh.conf` files.
- The Zephyr line is selectable: `NROS_ZEPHYR_VERSION=4.4 just zephyr …`
  (default 3.7 LTS). See `docs/development/zephyr-version-support.md`.
