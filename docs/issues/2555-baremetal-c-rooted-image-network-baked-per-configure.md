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

## The shape (maintainer ruling 2026-11-14)

The image's identity is read at RUN time from a struct the generated entry
fills, not compiled into the archive.

- **The ABI is board-generic.** `<nros/board.h>` (the board C ABI, RFC-0054,
  `packages/boards/nros-board-cffi`) declares `nros_board_image_config_t`:
  `version` (`NROS_BOARD_IMAGE_CONFIG_VERSION`), `net` (the `NROS_IMAGE_NET`
  text, RFC-0103 D10f), `locator`, `domain_id`
  (`NROS_BOARD_IMAGE_DOMAIN_DEFAULT` = -1). `NULL` / -1 means "the board's
  default". The Rust mirror is the bindgen output in
  `nros-board-cffi/src/generated.rs`.
- **Each board's C start keeps its own name** and takes
  `const nros_board_image_config_t *` as its first argument. mps2-an385 adopts
  it first: `nros_board_mps2_an385_start(image, app)`.
- **Who fills it.** The board's `c/c_main.c`, which the build compiles once
  PER ENTRY target, builds a `static const` struct from three macros the entry
  target carries: `NROS_IMAGE_NET` and `NROS_ENTRY_LOCATOR` (string literals,
  `NanoRosEntry.cmake`, from the entry's own scope, the same shadowing issue
  2336 gave ThreadX's per-image `NROS_APP_CONFIG`) and `NROS_ENTRY_DOMAIN_ID`.
  An absent macro leaves `NULL` / -1; the entry's empty locator literal also
  reads as "not stated".
- **How the board reads it.** `c_start.rs` turns the pointer into
  `Config::from_image_parts(net, locator, domain)`, which starts from the
  compile-time `Config::from_image()` and replaces what the struct states. The
  network goes through the same `image_net::boot_interface` parser. A version
  the board does not know is refused (`EPROTO`) rather than read.
- **One carrier on the CMake road.** W6's interim `NROS_IMAGE_NET` cargo env row
  (in `nros_board_facts_env`) is removed again. The cargo road
  (`build/<image>/nros-cargo.toml` `[env]` rows, `Config::from_image`) is
  unchanged.

## Measured (2026-11-15, QEMU 11.0)

- **One configure, two images.** A scratch copy of `examples/workspaces/c`
  with two bare-metal images added to `service_bringup`
  (`baremetal_service_{server,client}`, `board-net-10.toml` /
  `board-net-11.toml`) builds both entries in ONE build dir
  (`build/bare-metal-zenoh-mps2-an385-baremetal-service_bringup/cmake`, one
  runtime archive). They boot `10.0.2.10` and `10.0.2.11`, and the client
  prints `Result of add_two_ints: 5`.
- **Negative control.** The same `c/service-{server,client}` pair with
  `c_main.c` passing `NULL` (and W6's cargo env row gone) boots `10.0.2.15`
  on both sides; the client reports `zenoh open: no session with
  tcp/10.0.2.2:10310`, the failure this issue describes.
- `test_qemu_baremetal_capi_e2e`: 6/6 with the struct, on one carrier.

**Status.** The runtime identity lands with this fix. The issue stays open
for the first follow-up below, which removes the copies its limitation
forced.

## Follow-ups

- Move the ten bare-metal roles into `examples/workspaces/{c,cpp}`'s bringups
  as bare-metal images, delete the copies, and resolve
  `test_qemu_baremetal_capi_e2e` from those rows (RFC-0130 D3).
- Adopt the struct in the other boards' C starts as each gains one. Today
  mps2-an385 is the only board whose C start is Rust (the only reader of
  `NROS_IMAGE_NET` through `option_env!`). The RTOS boards resolve the image
  network in C per entry already (`nros_board_net_c`, issue 2336) and need it
  only if a Rust half of their boot comes to read it.
