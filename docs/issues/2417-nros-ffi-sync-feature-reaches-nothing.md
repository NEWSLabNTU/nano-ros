---
id: 2417
title: "`nros/ffi-sync` reaches no code: it turns on `nros-node/ffi-sync`, an empty feature nothing reads, and never the zenoh backend's real `ffi-sync` guard"
status: open
type: bug
severity: low
area: [api, rmw, zenoh, features]
found: 2026-11-10
related: [phase-535]
---

**Home: [phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md)** W6.
Found while retiring the mps2-an385 `*-rtic-mixed` leaves.

## What

The umbrella documents `ffi-sync` as the FFI reentrancy guard (a critical
section around every RMW FFI call), for an image where a higher-priority task
can preempt a call into zenoh-pico. The chain stops one crate short:

- `packages/api/nros/Cargo.toml`: `ffi-sync = ["nros-node/ffi-sync"]`.
- `packages/core/nros-node/Cargo.toml`: `ffi-sync = []`. No `cfg(feature =
  "ffi-sync")` exists in `nros-node` or `nros` source, and the feature
  forwards nowhere.
- `packages/rmw/zenoh/nros-rmw-zenoh/Cargo.toml`: `ffi-sync =
  ["dep:critical-section"]`, read by `src/zpico.rs` (the guard itself). No
  manifest in the tree turns it on.

So an image that names `nros/ffi-sync` builds byte-for-byte like one that does
not. Its only user was `examples/mps2-an385-baremetal/rust/listener-rtic-mixed`,
whose "mixed-priority" e2e therefore never exercised the guard. Its two-priority
task split was already gone since phase-244 D1 (the declarative node has one
dispatch context), so the pair was the plain RTIC pair on another port.
Phase-535 W6 retired it into the `baremetal_*_rtic` images.

## Fix

Either forward the feature to the backend
(`ffi-sync = ["nros-node/ffi-sync", "nros-rmw-zenoh?/ffi-sync"]`, or through
whichever crate links the backend for an image), with a test that a build
naming it compiles `zpico.rs`'s guarded arm, or drop the umbrella feature and
its documentation. Today no in-tree image preempts an FFI call (the RTIC entry
runs every node on one priority-1 task), so nothing needs the guard yet.
