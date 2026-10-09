# examples/zephyr — Zephyr RTOS examples

C, C++ and Rust examples built with `west`. Just module: **`zephyr`**
(`just/zephyr.just`, recipes in `just/zephyr-{setup,ci,dev}.just`).

## Prerequisites

```sh
source ./activate.sh
just setup zephyr             # west workspace (large download) + sources + patches
```

Rust examples additionally need ROS 2 sourced (`source /opt/ros/<distro>/setup.bash`)
so `nros` can generate the interface crates before the west build.

## RMW selection

An IMAGE of the leaf's `system.toml`, not a conf overlay (phase-481, RFC-0098
D10/D11). Every leaf declares one image per RMW — `[image.zephyr_zenoh]`,
`[image.zephyr_xrce]`, `[image.zephyr_cyclonedds]` — and a build selects one with
`-DNROS_IMAGE=<id>` (no selection builds `default_images`, zenoh). The nano-ros
Zephyr module renders the selected image into a Kconfig fragment merged after
every conf file: the RMW choice, the language API (from the package), the agent
endpoint (`locator`) and the image's `env` knobs (e.g. the XRCE session caps).
Each image's `conf` names the Zephyr-native fragment its RMW needs
(`prj-<rmw>.conf`: TCP, POSIX threads, heap), which the module adds ahead of
your own `EXTRA_CONF_FILE`. So no conf file in an example states a
`CONFIG_NROS_*` knob (`just check leaf-conf-nros-knobs` refuses one).

```sh
west build -b native_sim/native/64 examples/zephyr/c/talker -- \
    -DCONF_FILE=prj.conf -DNROS_IMAGE=zephyr_xrce \
    -DEXTRA_CONF_FILE=$PWD/cmake/zephyr/native-sim-line-3.7.conf
```

The `build-one` recipe wires this for you.

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

Backends: zenoh + xrce across all six roles per language; cyclonedds partial
(see the [coverage matrix](../README.md)).

## Gotchas

- Zephyr POSIX needs `CONFIG_MAX_PTHREAD_MUTEX_COUNT=32` /
  `CONFIG_MAX_PTHREAD_COND_COUNT=16` (zenoh-pico exhausts the default 5) —
  already set in the shipped `prj-zenoh.conf` files.
- The Zephyr line is selectable: `NROS_ZEPHYR_VERSION=4.4 just zephyr …`
  (default 3.7 LTS). See `docs/development/zephyr-version-support.md`.
