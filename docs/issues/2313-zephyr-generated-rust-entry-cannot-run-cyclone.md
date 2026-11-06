---
id: 2313
title: "A generated Rust Zephyr entry cannot run Cyclone DDS: undeclared feature, no backend registered, then PublisherCreationFailed"
status: open
type: bug
area: [cli, zephyr, rmw-cyclonedds]
severity: medium
found: 2026-11-07
related: [phase-535, 0155, 0163, 1288]
---

## What happens

Phase-535 W4 gave the Rust workspace's behaviour bringups
`zephyr_<role>_cyclonedds` images. They are the first GENERATED Rust Zephyr
entries on Cyclone: every in-tree Rust Zephyr Cyclone image before them was a
hand-written leaf (`examples/zephyr/rust/<role>`, `zephyr_component_main!`).
Three defects stack, each hidden by the one before it:

1. **The entry does not build.** `nros build` writes the entry's `Cargo.toml`
   with no `[features]` row when the image's RMW has no rlib
   (`rlib_dep = ""` in `nros-rmw-cyclonedds/nros-rmw.toml`). The selection
   facade's `__nros_register_backends!(gated)` still emits
   `#[cfg(feature = "rmw-cyclonedds")]`, so the image's `-D warnings` clippy
   pass refuses it with `unexpected cfg condition value: rmw-cyclonedds`.
   Declaring the feature off (a `[features] rmw-cyclonedds = []` row from
   `builder::west_app` / `builder::entry`) clears it.
2. **No backend is registered.** With the feature declared and off, nothing
   registers Cyclone: `nros::main!`'s Zephyr arm calls only the facade's
   gated Rust `register()`, and Cyclone has none on this road. The leaf road
   works because `zephyr_component_main!` also calls the module's C hook
   `nros_app_register_backends()` (issue 0155). Calling that hook from the
   Zephyr arm of `nros::main!` too makes the boot reach `session open`.
3. **Publisher creation fails.** The boot then stops at
   `node declaration failed — NodeError::Transport(PublisherCreationFailed)`.
   The generated entry enables `nros/needs-type-descriptors` (the leaf did
   not), and the Zephyr arm never calls `__nros_register_type!`, whose
   Cyclone arm names `::nros_rmw_cyclonedds::register::<T>()`, a crate the
   Zephyr road does not link. Not investigated further.

Fixes 1 and 2 were measured on the native_sim images and then backed out of
the W4 PR, because without fix 3 nothing exercises them.

## What W4 did instead

The Rust Cyclone pairs stay leaves: `examples/zephyr/rust/{listener,
service-server,service-client,action-server,action-client}` keep their
`cyclonedds` rows, and `tests/zephyr.rs`'s `resolve_example` reaches them
through `zephyr-dds-rs-<case>`. Their zenoh and XRCE rows moved to the
workspace images like every other language's. No coverage moved.

## Fix

Make a generated Rust Zephyr entry register Cyclone and its message types
(items 1 to 3), add the six `zephyr_<role>_cyclonedds` images to
`examples/workspaces/rust/src/{pubsub,service,action}_bringup`, and retire the
five leaves (phase-535 W6).
