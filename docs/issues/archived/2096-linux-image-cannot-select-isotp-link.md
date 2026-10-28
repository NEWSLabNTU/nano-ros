---
id: 2096
title: "A Linux image cannot select the ISO-TP (or multicast CAN) link: nothing maps a link onto zpico-sys's link-isotp / link-can feature, so the ROS ISO-TP cells have no build path"
status: resolved
type: limitation
area: [zenoh, can, cli, boards]
severity: medium
found: 2026-10-27
resolved: 2026-10-27
resolved_in: "the PR that archives this file (fix/2096-link-features)"
related: [phase-514, 2091, 0083]
---

**Home: [phase-514](../../roadmap/archived/phase-514-network-config-layers.md)** — the
layer that states a board's `links` (W1) and an image's locator (W2/W4).

## Measured

`scripts/test/isotp-ros-interop.sh` and `scripts/test/isotp-ros-params.sh` need
native nano-ros nodes with zenoh-pico's ISO-TP link compiled in. On main there
is no way to build one:

* `Z_FEATURE_LINK_ISOTP` / `Z_FEATURE_LINK_CAN` come only from
  `CARGO_FEATURE_LINK_ISOTP` / `CARGO_FEATURE_LINK_CAN`
  (`nros-board-common::policy::LinkFeatures::from_env`), i.e. from
  `zpico-sys`'s `link-isotp` / `link-can` cargo features, forwarded by
  `nros-rmw-zenoh`.
* No CLI code names either feature. The generated selection package of a
  `examples/linux/rust/*` leaf and the generated entry of an image-driven
  workspace depend on `nros-board-linux` / `nros-platform` only, so nothing
  reaches `nros-rmw-zenoh/link-isotp`.
* The last route was a per-package `link-isotp` passthrough in the features
  workspace's params entry; phase-383 W10.a (`0e943eacfc`) deleted that entry
  when the workspace became image-driven, and `examples/native/` (the service
  and action leaves the scripts name) is now `examples/linux/`.
* The linux board already states `links = ["tcp", "udp", "serial", "can",
  "isotp"]` (phase-514 W1), and an image can bake `locator = "isotp/..."` —
  but neither the board's `links` nor the locator's scheme selects a feature.

To run the cells on 2026-10-27 the feature had to be added BY HAND to the
generated manifests (an `nros-rmw-zenoh = { …, features = ["link-isotp"] }`
row in `build/nros/selection/Cargo.toml`, or in the generated entry's
`Cargo.toml`), and the two `cargo` commands `nros build --dry-run` prints run
directly. With that, every cell passed against a CAN-enabled zenoh-c 1.8.0:

| cell | result |
| --- | --- |
| service server (ros2 calls nano-ros) | PASS, `sum=42`, 141 frames |
| service client (nano-ros calls ros2) | PASS, `2 + 3 = 5`, 454 frames |
| action server | PASS, `[0,1,1,2,3,5]`, 532 frames |
| action client | PASS, 9 feedback samples, 983 frames |
| action cancel | PASS, both ends agree, 521 frames |
| parameters (via `rmw_zenohd`) | PASS, list + get 120 + set 250 + get 250, 911 frames |

So the links work; it is the road to them that is missing.

## Fix direction (not decided)

The link a session can open is a fact the image already states — its
`locator` scheme, and its board's `links`. Derive the `zpico-sys` link
features from it, on every road (cargo leaf, workspace entry, cmake, west)
the way `transport = "serial"` already implies `ZPICO_NO_SMOLTCP` /
`NROS_LINK_IP=0`: a locator whose scheme is `isotp` turns `link-isotp` on, one
whose scheme is `can` turns `link-can` on, and a board whose `links` omits the
kind refuses the locator. Not a per-package passthrough again — that is the
shape phase-383 removed, and a fact authored per package drifts from the one
the image states.

## Resolution

The link is implied from the wiring, on every road, through one table
beside its reader: `nros_board_common::policy::LINK_BUILD_KNOBS` maps an
endpoint scheme to a build knob (`can` → `NROS_LINK_CAN`, `isotp` →
`NROS_LINK_ISOTP`), `link_build_knobs` reads an endpoint list against it,
and `LinkFeatures::from_env` takes the knob beside `zpico-sys`'s cargo
feature (`nros-zpico-build` reruns when either knob changes). The CLI
implies each knob — IMPLIES, so `[image.<id>] env` and the calling
environment still win — from:

* the image's `locator`: `leaf_settings::transport_implications` (the
  single-package cargo and cmake roads) and `image_block_layers` (both
  workspace roads);
* the `connect` / `listen` endpoints of its lowered RMW file, where the
  file applies: the same leaf function, guarded so it reads a file only when
  one can apply (no locator beside it, RFC-0109 D9; an `rmw/` directory or an
  `rmw_config` key) — and next to the two workspace roads' existing
  `session_text` lowering.

Not a fifth `TRANSPORT_KINDS` entry: that list is what a board registers a
driver for (`nros_platform::LinkKind`), and on a hosted target the kernel
owns a CAN interface.

Measured: `examples/linux/rust/service-server` builds without
`_z_open_isotp` by default, with it under `NROS_LINK_ISOTP=1`, and with it
when its `[image.linux] locator` is `isotp/vcan0#…` (the settings row
written). All six ROS ISO-TP cells then PASS with their nodes built by
`NROS_LINK_ISOTP=1 nros build` — no hand-edited manifest — as both scripts'
headers now say.
