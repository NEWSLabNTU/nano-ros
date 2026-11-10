---
id: 2336
title: "An image's `os_config` network file is applied per cmake CONFIGURE, so images sharing one configure all get the last one's address"
status: resolved
type: bug
area: [build, cli, cmake]
severity: medium
found: 2026-11-07
related: [phase-535, 1299]
---

**Home: [phase-535](../../roadmap/phase-535-example-reorg-and-cadence-ci.md).**

## What happens

Phase-535 W5 gave the threadx-riscv64 Cyclone pair two images in one
bringup: `riscv_threadx_talker_cyclonedds` with `os_config` naming
`board-net-40.toml` (10.0.2.40, MAC `:56`) and
`riscv_threadx_listener_cyclonedds` with `board-net-41.toml` (10.0.2.41,
`:57`). Both rows built into one configure
(`build/threadx-riscv64-cyclonedds-rv-virt-threadx/cmake`). The two-QEMU
test then heard nothing: the talker published, the listener subscribed, and
no sample crossed.

Both images carried 10.0.2.41 / `:57`. The configure holds ONE
`nano_ros/nros_app_config_def.c`, generated from the `NROS_IMAGE_NET` cache
entry, and each `nros build` of a row overwrites it. The last image built
wins, and every executable of the configure links the same definition.

On zenoh this is invisible: each QEMU has its own SLIRP network, so two
images with one address never meet. It matters wherever two images share a
link (Cyclone's two-QEMU pairs) or wherever the address is the identity.

## Workaround in tree

None that keeps the images. A row's `build_subdir` is where the fixture
builder LOOKS (and how it groups rows into build units); `nros build` picks
the configure dir from (platform, rmw, board) itself, so two rows naming two
subdirs still configure one directory, now concurrently, and the configure
fails. So the threadx-riscv64 Cyclone pairs stay on the leaves
(`examples/rv-virt-threadx/{c,cpp}/{talker,listener}`, one configure each)
until this is fixed.

## Fix direction

Make the network definition per image: one config TU per executable (as the
generated `<image>_nros_main_generated.c` already is), or refuse two rows
with different `os_config` net files in one configure. Either needs a check
that would have caught this: two images of one configure, different net
files, one address in both artifacts.

## Resolution (2026-11-10)

The first fix direction: one config TU per executable.

- `nros_threadx_app_config_def(<board> <out>)` (`cmake/NanoRosBoardNet.cmake`)
  is the one writer of the ThreadX `NROS_APP_CONFIG` TU. It reads the image
  network in scope through `nros_board_net_c`, and writes only when the
  content changes.
- Both ThreadX board files (`rv-virt-threadx`, `threadx-linux`) write the
  configure's shared TU through it. That TU still serves a target with no
  image of its own (a hand-written example).
- `nano_ros_entry` writes `<entry>_nros_app_config_def.c` for a ThreadX entry
  whose image states a network. It does this inside the entry's scope, where
  the image's `NROS_IMAGE_NET` shadows the configure's, and marks the target
  `NROS_OWN_APP_CONFIG`.
- The ThreadX `nros_platform_link_app` then leaves the shared TU off that
  target. That link is deferred to the end of the directory, which is why the
  TU cannot be written there: the entry's scope, and its network, are gone by
  then.

Measured on a copy of `examples/workspaces/c` with two `rv-virt-threadx`
Cyclone images in one bringup (`os_config` naming the leaves' `host = 40`,
`:56` and `host = 41`, `:57` files). One configure,
`threadx-riscv64-cyclonedds-rv-virt-threadx-pubsub_bringup`, now holds:

- `rx_talker_cy_entry_nros_app_config_def.c`: `10.0.2.40`, `:56`;
- `rx_listener_cy_entry_nros_app_config_def.c`: `10.0.2.41`, `:57`;
- each target's `build.make` compiles its own TU and not the shared
  `nano_ros/nros_app_config_def.c`.
- both executables build and link (`cmake --build … --target
  rx_talker_cy_entry rx_listener_cy_entry`), with no second definition of
  `NROS_APP_CONFIG`.

The check this issue asked for: `just check threadx-app-config-per-image`
(`tests/threadx-app-config-per-image-tests.sh`).

- It drives the writer from two scopes with these two networks, using the
  real CLI, and requires two TUs with their own addresses.
- A reconfigure with the same network must not rewrite a TU.
- It also checks the reach: the entry's call, the target property, the
  link's exclusion, and both board files on the one writer.
- Mutation-tested: a writer that ignores the scoped network fails it.

The threadx-riscv64 Cyclone pairs can now move from the leaves to one
bringup (phase-535's work, not done here).
