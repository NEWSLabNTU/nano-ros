---
id: 2051
title: "A Zephyr Rust west application cannot bake its bringup's lowered RMW file yet"
status: open
type: limitation
area: [zephyr, config, rmw]
severity: low
found: 2026-10-25
related: [phase-514, RFC-0109, issue-1288]
---

## What

RFC-0109 D8 (phase-514 W3a): a bringup whose backend reads a
`session-json5` file (zenoh's `rmw/zenoh.json5`) has it lowered on the host
and baked into each image. The carriers are:

- cargo roads (single-package leaf, workspace cargo image): `nros sync` /
  `nros build` write the lowered list into the image's cargo settings as
  `NROS_SESSION_PROPERTIES`, which `nros::main!` reads at expansion;
- cmake roads, including a C/C++ Zephyr west application: `nano_ros_entry()`
  passes the file to `nros codegen entry --rmw-config`.

A **Rust** Zephyr west application (`rust_cargo_application()`) is on
neither: the west lane has no `--config` seam of its own (issue 1288), so
nothing puts `NROS_SESSION_PROPERTIES` in front of the cargo build Zephyr's
Rust support runs. Baking nothing would boot the image on the board's default
router while the bringup says otherwise, so `nros build` REFUSES such an
image, naming this issue.

## Fix shape

Forward `NROS_SESSION_PROPERTIES` on the Zephyr module's cargo command, the
way the module's knob resolver forwards `NROS_SIZING_DESCRIPTOR` (issue
1407), then drop the refusal in `nros-cli-core`'s west branch
(`cmd/build.rs`, "issue 2051").
