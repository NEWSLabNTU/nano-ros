---
id: 669
title: "`executor::handoff` is public API with no consumer, gated on `std`, and
  freeing it costs a dependency edge in a core crate"
status: resolved
type: design
area: api
related: [phase-359, issue-0594]
---

## What it is

`nros_node::executor::handoff::Handoff<M, N>` — a bounded FIFO between two
callbacks on different `SchedContext`s, added by phase-104.E.1. It is
`pub mod handoff;` and `#[cfg(feature = "std")]`.

## Two facts found while clearing phase-359 W10's `nros-node` sites

**1. Its recorded reason for being std-gated is not the one that holds.** The
module doc said:

> `std`-gated for now — the `alloc`-only path needs a lock-free SPSC queue
> (heapless::spsc requires a `.split()` call that doesn't compose with
> Arc-sharing across callbacks).

That describes a design the module does not use. What is actually there is a
`heapless::Vec` behind a mutex, and the only hosted thing in the file is
`std::sync::Mutex`. The doc has been corrected in place.

**2. It has no consumer.** `grep -rn 'Handoff' packages/ examples/` returns
nothing outside the module itself. Not one call site, in-tree, since phase 104.

## Why it was not simply ported

A portable mutex exists in the workspace — `nros_rmw::sync::Mutex`, backed by
`spin` or `critical-section`. But `nros-node` takes `nros-rmw` with
`default-features = false` and enables neither sync feature, so reaching it
means adding `nros-rmw/sync-spin`, i.e. a new `spin` dependency edge on EVERY
build of a core crate — for a type nothing calls.

Hand-rolling a lock over `portable_atomic::AtomicBool` would avoid the edge and
is about fifteen lines, which is precisely why it is the wrong answer: an
un-poisoned, unfair, hand-written mutex in the crate every image links, to serve
zero callers.

## The decision this wants

1. **Delete it.** No in-tree consumer, and out-of-tree users of a phase-104 API
   can be told. Removes a `std`-only corner from a crate the campaign is making
   `no_std`.
2. **Port it and pay the edge** — `nros-rmw/sync-spin`, available everywhere.
   Justified if the "two callbacks, different SchedContexts" hand-off is a
   pattern embedded users are expected to reach for, which is what it was
   designed for.
3. **Leave it std-gated**, and accept that `nros-node` keeps one hosted module.

This is a question about whether an unused public API justifies a dependency in
a core crate, which belongs to whoever owns the API rather than to the flavour
campaign.

## Resolved 2026-08-19 — option 1, deleted

The measurement that decided it: **`spin` is in no board's dependency graph
today.** `cargo tree -e normal` on `nros-board-{freertos,mps2-an385,threadx,
esp32-qemu,nuttx,linux}` finds it nowhere — its only appearance in the tree is
under `nros-rmw-cyclonedds-sys`, which none of those take. So option 2 was not
"flip a feature", it was "put a new crate in every image", for a type with no
caller.

Option 3 (leave it) was the status quo and costs `nros-node` a hosted module
forever, which is what phase-359 exists to remove.

A fourth shape came up and was rejected: gate the type on its own feature that
pulls `nros-rmw/sync-spin`, so nobody pays unless they ask. That keeps the API
and costs nothing at build time — but it recreates issue 0652's class, a target
in no lane: a feature nothing enables is code nothing compiles, and this one has
had no consumer for four months. Preserving an API that way is preserving the
appearance of one.

### The pattern it wrapped, for anyone who needs it

`Handoff` was six lines of sugar. Written out, with the same semantics
(non-blocking push, drop on overflow, drain from a lower-priority timer):

```rust
use std::sync::{Arc, Mutex};
use heapless::Vec;

let q: Arc<Mutex<Vec<MyMsg, 32>>> = Arc::new(Mutex::new(Vec::new()));

let q_in = Arc::clone(&q);
executor.register_subscription::<MyMsg, _>(topic, move |msg: &MyMsg| {
    if let Ok(mut g) = q_in.lock() {
        let _ = g.push(msg.clone());   // full -> drop; the high-pri side never stalls
    }
})?;

let q_out = Arc::clone(&q);
executor.register_timer(period, move || {
    let drained: Vec<MyMsg, 32> = match q_out.lock() {
        Ok(mut g) => core::mem::take(&mut g),
        Err(_) => return,
    };
    for msg in drained {
        let _ = pub_out.publish(&msg);
    }
})?;
```

Draining into a local before publishing is one improvement over the deleted
type, which held its lock across `pop()` per message while the low-priority side
published.

### Its sibling went the other way, and the contrast is the rule

`nros::metadata_mode` had the same shape — one `std::sync::Mutex` around a
process-global — and was PORTED in the same commit, onto `nros_rmw::sync::Mutex`
behind `metadata-mode = ["nros-rmw/sync-spin"]`. The difference is not taste:

* there the lock was the ONLY `std` thing, so porting it moved a whole
  capability from `std` to `alloc`; here it was the only `std` thing in a module
  with no callers, so porting it moved nothing;
* there the `spin` edge rides an OPTIONAL feature that no firmware image
  enables; here it would have been unconditional.

Ask what the edge BUYS and who PAYS for it. Same primitive, opposite answers.

## Reproduce

```
grep -rn 'Handoff' packages/ examples/ | grep -v handoff.rs   # empty
grep -n 'sync-spin\|^spin' packages/core/nros-node/Cargo.toml # absent
```
