---
id: 2313
title: "A generated Rust Zephyr entry cannot run Cyclone DDS: undeclared feature, no backend registered, then PublisherCreationFailed"
status: open
type: bug
area: [cli, zephyr, rmw-cyclonedds]
severity: medium
found: 2026-11-07
related: [phase-535, 0155, 0163, 1288, phase-548]
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md).**

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

## Progress (2026-11-09, issue 2354)

- Item 1 is fixed on `main`: the generated entry declares `rmw-cyclonedds`
  off by default (`builder::west_app`).
- Item 2 is fixed by issue 2354: `nros::main!`'s Zephyr arm calls
  `nros_app_register_backends()`, and the module's Rust hook references
  Cyclone strongly.
- Item 3 did NOT reproduce on a talker: a generated `--lang rust --rmw
  cyclonedds` talker on native_sim publishes. The listener, service and
  action roles were not run, so item 3 is open for them.

  Measured later the same day on the same fix: all six roles boot past
  declaration, and the three pairs pass on native_sim (talker->listener
  `I heard: [Hello World: 2]`; service `Result of add_two_ints: 5`; action
  `Result received: [0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]`). Item 3 does not
  reproduce.

Left (assigned to nros 133; phase-535 W6 skips these leaves): the six
`zephyr_<role>_cyclonedds` images (after issue 2365's PR B, #3345, since
their `prj-cyclonedds.conf` sets stack sizes), and retiring the listener,
service-server, service-client and action-client leaves. The action-server
leaf STAYS for its zenoh row (RFC-0122: `zephyr_rust_zenoh_action_goal_completion_rate`
reads its `reply-slot: refusals=` heartbeat, which the workspace action
server does not print); only its Cyclone row retires. Follow-up: give the
workspace action server that heartbeat, then retire the leaf.

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

## Also on threadx-riscv64 (phase-535 W5)

Not Zephyr-only. The threadx-riscv64 Rust behaviour bringups got
`riscv_threadx_<role>_cyclonedds` images, and they BUILD on the cargo road,
but the listener boots to:

```
[ERROR] nros: cannot select an RMW backend — no RMW backend is registered
Executor::open failed: Transport(InvalidConfig)
```

The leaves link Cyclone through the Corrosion/cmake seam
(`build_threadx_cmake_rmw`), which registers the backend; the generated cargo
entry does not. So the five threadx-riscv64 Rust listener, service and action
leaves keep their Cyclone rows, as the Zephyr ones do, and their retirement
waits on this issue.

## Zephyr: done (2026-11-09)

The six images exist and the Rust Cyclone pairs run on them:
`zephyr_{talker,listener,service_server,service_client,action_server,action_client}_cyclonedds`
in `examples/workspaces/rust/src/{pubsub,service,action}_bringup`, Cyclone
domains 22/25/28 (`alloc::domain_of`, Rust's port index 0), each with its
leaf's own `prj-cyclonedds.conf`. These are the leaf's file and not the C
bringup's: the Rust service leaf's pthread counts (256) differ from the C
service bringup's (2048/1024). Six `[[workspace_fixture]]` rows,
`workspace-rust-zephyr-<role>-cyclonedds`. `tests/zephyr.rs`'s
`resolve_example` lost its Rust-Cyclone leaf special case.

Measured: `cargo nextest run -p nros-tests --test zephyr -E
'test(/cyclonedds_rust/)'` passes 5/5 on the images (talker and listener boot
smokes; pubsub, service and action e2e).

Retired (RFC-0122, no lost coverage; each cyclonedds row is an image row now):
`examples/zephyr/rust/{listener,service-server,service-client,action-client}`,
with their `Cargo.toml` excludes, `zephyr/module.yml` sample, nightly
`build-one` matrix entries, `nros_tests::zephyr` aliases and bringup-baseline
rows. `examples/zephyr/rust/action-server` STAYS a leaf for its zenoh row
(`zephyr_rust_zenoh_action_goal_completion_rate` reads its `reply-slot:
refusals=` heartbeat); only its cyclonedds row moved. Recorded in phase-535 W6.

Left open here:
- Follow-up: give the workspace action server that heartbeat, then retire the
  action-server leaf.
- The threadx-riscv64 half above (`riscv_threadx_<role>_cyclonedds` boots to
  "no RMW backend is registered") is not addressed.
