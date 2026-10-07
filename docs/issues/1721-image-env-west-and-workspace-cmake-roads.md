---
id: 1721
title: "`[image.<id>] env` is still dropped on the Zephyr west road and the workspace cmake road"
status: open
type: bug
area: [build, config, zephyr, cmake]
severity: medium
found: 2026-10-06
related: [1712, rfc-0049, rfc-0098, phase-445, phase-481]
---

## Plan (2026-10-07)

Owned by [phase-481](../roadmap/phase-481-image-config-one-source.md), from
[RFC-0098](../design/0098-generated-leaf-build-config.md) D10-D12. The
"precedence decision" below was the wrong question: on Zephyr the image's `env`
and the leaf's `prj.conf` are two sources for one fact, so the fix removes one
-- nano-ros knobs, the RMW choice and the deploy endpoint move to
`system.toml` and are rendered into a Kconfig fragment (D11), with a gate on
conf files and the Zephyr examples migrated. On the workspace road, images
whose configuration differs get their own configure (D12).

## Summary

Issue 1712 gave a standalone C/C++ leaf's `[image.<id>] env` (RFC-0049's APP
rung) a carrier on the cmake road: a `--config` cargo file, below an exported
variable. Two roads still parse the rows and deliver nothing:

- **Zephyr west** — both a standalone Zephyr Rust leaf
  (`examples/zephyr/rust/*`, resolved by `leaf_settings::resolve_west`) and a
  Zephyr C/C++ leaf. Since 1712 the C/C++ configure WARNS
  (`nano_ros_read_leaf_system`); the Rust leaf says nothing.
- **Workspace cmake images** — `nros build` on a `Driver::CMake` image emits
  `build/<coord>/` and never reads `image.env` (`cmd/build.rs`, the CMake arm),
  and a workspace member's own `system.toml` env gets the same 1712 warning.

## Why this is not 1712's one-line carrier

**West.** The nros cargo commands are built in `zephyr/cmake/nros_cargo_build.cmake`
while Zephyr loads its modules — inside `find_package(Zephyr)`, before the
application's `find_package(nano_ros)` reads `system.toml` — and every knob the
lane resolves is baked into `cmake -E env` by `_nros_resolve_knob`, whose ladder
is env (configure-time) > Kconfig > derived > builtin. A `--config` `[env]` file
there would sit BELOW Kconfig (a `cmake -E env` row always beats an unforced
`[env]` row), and a Kconfig `int` always states a value, so the image rung would
lose to every Kconfig default. Placing it correctly means an app rung inside
`_nros_resolve_knob` itself, read through the CLI from `APPLICATION_SOURCE_DIR`
(the way `nros_resolve_board_facts` already reads the board), and deciding how
it ranks against the application's own `prj.conf` — which is ALSO a per-image
statement. That is a precedence decision, not a missing edge. zephyr-lang-rust's
`rust_cargo_application` (the Rust leaves) is a third command with no seam yet.

**Workspace cmake.** One configure under `build/<coord>/` serves every image on
that coordinate, and nros-c / nros-cpp are built once for all of them, so a
per-IMAGE env has no single cargo build to attach to unless each image gets its
own coordinate or configure.

## Workaround

West: state the knob in the application's Kconfig (`CONFIG_NROS_*`), or export
it in the shell that configures. Workspace cmake: export it.

## Acceptance

A Zephyr leaf and a workspace cmake image that state a knob in `[image.<id>]
env` build an artifact that read it, with an exported variable still winning,
measured on a built image as 1712 did.
