---
id: 1344
title: "The zenoh QoS report-once latch uses `core::sync::atomic`, which has no
  `swap` on riscv32imc — so no esp32 image compiles, and the lane that would
  have said so runs nightly"
status: resolved
type: bug
area: [rmw, embedded, build]
severity: high
related: [1343, phase-448]
---

## What happens

Every esp32 fixture fails to compile:

```
error[E0599]: no method named `swap` found for reference `&Atomic<bool>` in the current scope
   --> packages/rmw/zenoh/nros-rmw-zenoh/src/shim/qos.rs:110:11
    |
110 |     !flag.swap(true, core::sync::atomic::Ordering::Relaxed)
    |           ^^^^ method not found in `&Atomic<bool>`

error: could not compile `nros-rmw-zenoh` (lib) due to 1 previous error
error: recipe `build-qemu` failed with exit code 101
```

Reproduced 2026-09-12 with `just esp32 build-fixtures` on a fully provisioned
host (`just doctor esp32` all green: espflash, esp32-qemu, the
`riscv32imc-unknown-none-elf` target).

## Why

`riscv32imc` has no A extension, so `core::sync::atomic::AtomicBool` on that
target is the load/store-only shape: `load` and `store` exist, every read-modify-
write — `swap` included — does not. That is why this tree spells no_std atomics
`portable_atomic::*` everywhere else, including seven lines up the same module
(`shim/mod.rs:49` `use portable_atomic::Ordering`, `:196` `AtomicPtr`) and in
`nros_node::executor::backing`. `portable-atomic` is ALREADY a dependency of
this crate (`Cargo.toml:149`), so nothing had to be added — the two new statics
simply reached for `core::` instead.

Introduced by `b0ea5a04b fix(rmw/zenoh): grant the QoS the shim can serve,
refuse the rest` (2026-09-11), which added `DEPTH_CLAMP_REPORTED` and
`RELIABILITY_GRANT_REPORTED` as report-once latches.

## Why nothing caught it

No merge-gating lane builds an esp32 image. `check-compile-smoke` and
`check-workspace-all` are host builds; the esp32 fixtures are built by
`just esp32 build-fixtures`, which only the nightly reaches. So a CAS-less
target's compile errors land on `main` and are found by whoever next runs that
lane — here, phase-448 W5 trying to re-measure the esp32 heap pairing.

This is the same shape as issue 1226: the gate exists (the compiler), and
nothing between the commit and its merge asks it.

## Fix

`use portable_atomic::{AtomicBool, Ordering};` for the two latches, the spelling
the rest of the crate already uses. `portable-atomic` resolves a CAS-less target
through the `critical-section` / `unsafe-assume-single-core` feature that the
consuming board already enables (`nros-board-esp32-qemu`'s
`portable-atomic = { features = ["unsafe-assume-single-core"] }`).

Verified by building: `just esp32 build-fixtures` compiles
`nros-rmw-zenoh` for `riscv32imc-unknown-none-elf` after the change.

## Not fixed here

The nightly esp32 lane is the only thing that builds this target, and that is
the durable defect. A `cargo check` of `nros-rmw-zenoh` for one CAS-less triple
is seconds, not minutes, and would belong on the push lane — but sizing and
placing that gate is its own piece of work, not this one-line fix's.
