---
id: 669
title: "`executor::handoff` is public API with no consumer, gated on `std`, and
  freeing it costs a dependency edge in a core crate"
status: open
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
campaign. Left std-gated for now, deliberately, with the corrected rationale in
the module.

## Reproduce

```
grep -rn 'Handoff' packages/ examples/ | grep -v handoff.rs   # empty
grep -n 'sync-spin\|^spin' packages/core/nros-node/Cargo.toml # absent
```
