---
id: 106
title: "RMW backend `.init_array` self-register ctor is dead-stripped when an Entry deps but never references the backend crate → `Executor::open_multi` fails `Transport(InvalidArgument)`"
status: open
type: bug
area: rmw
related: [phase-267, rfc-0009]
---

## Summary

A native binary that **depends on** an RMW backend crate (`nros-rmw-zenoh`,
`nros-rmw-cyclonedds-sys`, …) but never **references** any of its symbols gets its
backend **dead-stripped** by the linker — including the `#[used]` `.init_array`
self-register ctor (`nros_rmw_register_backend!`, Phase 249 P4b). The backend is
then absent from the CFFI registry, so `CffiSession::open_named` resolves a null
vtable and returns `TransportError::InvalidArgument`
(`nros-rmw-cffi/src/lib.rs:open_named`, the `raw.is_null()` branch). Surfaces as
`Executor::open_multi(...)` → `Transport(InvalidArgument)`.

## Repro (phase-267 ws-bridge-rust)

`examples/workspaces/ws-bridge-rust/src/native_entry` deps `nros-rmw-zenoh` +
`nros-rmw-cyclonedds-sys` and runs `nros_bridge::run_from_config_str` (which calls
`Executor::open_multi`). The macro-emitted `fn main()` never references either
backend crate → both are dropped → `open_multi` → `OpenSession(Transport(
InvalidArgument))` at boot.

Confirmed by a diagnostic: adding explicit `nros_rmw_zenoh::register()` +
`nros_rmw_cyclonedds_sys::register()` (a symbol reference) makes `open_multi`
succeed. `extern crate nros_rmw_zenoh as _;` (force-link) likewise fixes it.

## Why it bites here and not elsewhere

Every prior native consumer **references** its backend: the board crate calls
`nros_rmw_<x>::register()` in its boot path (Phase 248 C5a), and the imperative
bridge bins call `register()` explicitly. The `run_from_config` path was written
to rely on the self-register ctor ALONE (config.rs comment: "backends
self-registered via their `.init_array` ctor before `main`; no runtime section
walk") — which only holds when the crate is force-linked.

## Fix direction

The self-register-ctor-only contract is fragile (link-order / dead-strip
dependent). Options:
1. **The bridge Entry force-links** the backends (`extern crate <backend> as _;`)
   — the phase-267 workaround in `ws-bridge-rust/native_entry`. Works, but is
   per-Entry boilerplate the macro could emit.
2. **`nros::main!` emits the register calls** for a bridge's RMWs (read from the
   `nros-bridge.toml` `[[node]]` rmws → `nros_rmw_<x>::register()`), mirroring the
   board boot path — robust, no Entry boilerplate, but re-adds the rmw→crate map
   in the macro.
3. **Strengthen the force-link** so `extern crate … as _` reliably retains the
   ctor across rust → rlib → final-binary (the `#[used]` keeps it in the object;
   verify the linker keeps the `.init_array` entry).

Recommend (2) for the bridge entry path (phase-267 C4) so a declarative bridge
Entry stays plain `nros::main!`.
