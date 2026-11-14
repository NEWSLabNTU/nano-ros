---
id: 2555
title: "A C-rooted bare-metal configure bakes ONE image's network into its Rust staticlib, so two images of one bringup cannot share a configure"
status: open
type: limitation
severity: medium
area: [boards, cmake, examples]
related: [phase-553, phase-531, rfc-0130, rfc-0103]
found: 2026-11-14
---

**Found by** phase-553 W6, while moving the ten bare-metal C/C++ roles
(`examples/mps2-an385-baremetal/{c,cpp}/{listener,service-*,action-*}`) onto the
CMake road.

## What happens

On a C-rooted mps2-an385 image the board boots through its C start,
`nros_board_mps2_an385_start`, which calls `run_bare(Config::from_image(), …)`.
`Config::from_image` reads the image's network identity (`NROS_IMAGE_NET`: MAC,
IP, gateway) and locator with `option_env!`, i.e. at the Rust COMPILE of the
image's runtime staticlib (`nros-c` / `nros-cpp` with the board crate bundled,
`cmake/NanoRosImageCrate.cmake`).

That staticlib is built once per CMake configure. A language workspace builds a
bringup's images in ONE configure per board: FreeRTOS's
`workspace-c-freertos-service-{server,client}` rows share
`build/freertos-zenoh-mps2-an385-freertos-service_bringup/cmake`. FreeRTOS takes
each image's network in C, per entry (`nros_board_net_c`, shadowed in each
entry's scope by `NanoRosEntry.cmake`), so this works there. On bare metal the
identity is in the shared Rust archive, so every image of the configure would
boot the same MAC and IP, and the second of a pair on one QEMU network would
never open its session.

## The measurement that found it

Before phase-553 W6, `NROS_IMAGE_NET` did not reach the staticlib's cargo build
at all (it reached only CMake's C lane). Every C-rooted image booted the board
default (`MAC 02:00:00:00:00:00`, `IP 10.0.2.15`). With the ten roles migrated,
`test_qemu_baremetal_capi_e2e` failed 6/6: the first image of each pair connected
and the second reported
`zenoh open: no session with tcp/10.0.2.2:<port> in one attempt`. Alone, either
image connected. W5's 6/6 had passed only because every pair still had one
cargo-built side, which applied its own network.

W6 made `nros_board_facts_env` (`cmake/NanoRosBoardFacts.cmake`) hand the
configure's `NROS_IMAGE_NET` to the cargo build, which fixes a one-image
configure: the pairs boot `10.0.2.12`/`10.0.2.13` and pass 6/6. A configure
holding two images still has one value to hand over.

## Consequence

RFC-0130 D3 places the ten roles "in the language workspaces' bringups". Under
this limitation they cannot share a bringup configure, so W6 keeps each role a
one-package workspace in its own directory (the shape W5 gave the talkers), with
a copy of the language workspace's component.

## Fix direction

Take the network at RUN time from the entry instead of baking it into the
archive: the generated entry passes the image's identity to the board's C start
(as FreeRTOS's C lane already resolves it per entry), and `Config::from_image`
becomes a fallback. Then the bare-metal images can join the language
workspaces' bringups and the copies go.
