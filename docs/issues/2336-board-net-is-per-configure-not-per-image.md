---
id: 2336
title: "An image's `os_config` network file is applied per cmake CONFIGURE, so images sharing one configure all get the last one's address"
status: open
type: bug
area: [build, cli, cmake]
severity: medium
found: 2026-11-07
related: [phase-535, 1299]
---

**Home: [phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md).**

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
