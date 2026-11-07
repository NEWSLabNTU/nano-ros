# `talker` — zephyr / rust

Standalone copy-out example: copy this directory anywhere, nothing above it
is required ([RFC-0026](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0026-example-directory-layout.md)).

## Build

A single-node workspace: the node package is `src/zephyr_talker_node`, and
`src/zephyr_talker_node_bringup` holds the `system.toml` that says what to build. `nros build`
generates the west application from it.

```bash
cp -r examples/zephyr/rust/talker ~/my-talker && cd ~/my-talker
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build zephyr_talker_node_bringup:zephyr_zenoh         # or zephyr_xrce, zephyr_cyclonedds
```

Each `[image.*]` is one build: `zephyr_<rmw>` targets `native_sim/native/64`
and `mps2_zenoh` targets `mps2/an385`. From the checkout,
`just zephyr build-one rust/talker zenoh` does the same.

## Run

Cross-built. SDK env comes from `source activate.sh` in the checkout;
QEMU / flashing steps live in the [zephyr README](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/zephyr/README.md).

## Config

Board, RMW, domain and locator: `src/zephyr_talker_node_bringup/system.toml`
(`[image.*]` + `[system]`, RFC-0098 D3/D5). The base Zephyr `prj.conf` of each
board is `boards/<board>/prj.conf` in the bringup, and each image's
`os_config` names its RMW fragments. Switching board is choosing another
image; no build command names a board.

Copy-out contract + the full example matrix: [`examples/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md).
