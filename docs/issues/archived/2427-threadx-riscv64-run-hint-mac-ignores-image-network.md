---
id: 2427
title: "The ThreadX riscv64 run plan boots every image with peer 0's MAC, whatever its network states"
status: resolved
type: bug
area: [cli]
severity: low
found: 2026-11-10
related: [phase-535, 2336]
---

**Home: [phase-535](../../roadmap/phase-535-example-reorg-and-cadence-ci.md).**

## What happens

`nros build` of `pubsub_bringup:riscv_threadx_listener_cyclonedds` in
`examples/workspaces/c` prints a QEMU line for the listener with
`-device virtio-net-device,…,mac=52:54:00:12:34:56`. The image's network
file (`board-net-41.toml`) states MAC `52:54:00:12:34:57`, and the image's
own config TU carries `.mac = {0x52, 0x54, 0x00, 0x12, 0x34, 0x57}`.
`:56` is the talker's MAC. A user who pastes the printed line for both
roles of the two-QEMU Cyclone pair runs both with one MAC on one L2 link.
`nros run` reads the same plan, so it boots the listener the same way.

## Why

The run plan (`cmd::run_plan::resolve_run`) gives a fixed `nros-qemu-ladder`
board its `mac` arg from `ladder_board`, which always answers
`threadx_riscv64_mac(0)` (peer 0, the firmware's `Config::default()`). It
never read the image's network, which the table-driven QEMU boards already
use for their `-nic` MAC (`image_nic`).

## Resolution

`resolve_run`'s ladder arm takes the MAC of the image network's primary
interface when the image states one (provenance `image`) and keeps the
ladder's peer-0 default otherwise.
`run_plan::tests::a_ladder_board_takes_the_image_networks_mac` holds both
cases.
