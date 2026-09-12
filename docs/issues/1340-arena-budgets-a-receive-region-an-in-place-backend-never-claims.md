---
id: 1340
title: "The arena budgets a full receive region for every subscription, and a backend that dispatches IN PLACE claims none — 9,768 bytes per subscription on zenoh and XRCE"
status: open
type: tech-debt
area: executor, build
severity: medium
found: 2026-09-12
related: [issue-1319, issue-1255, issue-1190, phase-454, rfc-0100]
---

## What the model says and what the runtime does

`Executor::register_subscription_buffered_on` asks the backend whether it can
hand a sample to the callback out of its own ring:

```rust
if handle.supports_process_in_place() {
    let entry_offset = self.arena_alloc::<SubInplaceEntry<M, F>>()?;
    …
    return Ok(HandleId(slot));
}
```

That test runs **before** the slot size is computed, so the buffered region —
`buffered_region_size(depth, slot)` — is never allocated. Both schemaless
backends answer an unconditional `true`:

* `nros-rmw-zenoh`'s `Subscription::supports_process_in_place` is
  `fn(&self) -> bool { true }`;
* XRCE's `xrce_subscription_supports_in_place` writes `true` and its
  `process_raw_in_place` slot is non-NULL (the capability is the conjunction of
  the two, per `rmw_vtable.h`);
* Cyclone leaves both slots NULL, so its subscriptions do take the buffered
  path.

`nros-node/build.rs` budgets a full region for every subscription regardless.

## Measured

`packages/testing/nros-tests/bins/contract-monitor`, `contract-monitor-sub`: a
Rust node on zenoh subscribing `std_msgs/Header` at the default `KEEP_LAST(10)`.
A probe over `Executor::arena_used()` across the one registration:

```
arena used: open=0 node=0 sub=672
SUB CLAIM   = 672
```

Against the model for the same image (`NROS_SUBSCRIBER_BUFFER_SIZE=880`,
`NROS_SUBSCRIPTION_BUFFER_SIZE=1496`, one declared subscription):

```
ARENA_SIZE=12840  REQUIRED=12840  PUBSUB_REGION=9768
```

So the subscription is budgeted **9,768 + 1,024 = 10,792 bytes** and claims
**672**. `mem-report` prices the linked image's executor backing at 27,152 bytes
of `.bss`; an image whose subscriptions all dispatch in place could give most of
the per-subscription half back.

This is the OVER direction, so nothing fails — which is why it has sat.
`report_arena_headroom` is the advisory that would say so at run time.

## Why phase-454 W5 did not take the saving

W5 recorded the fact (`RegistrationPath::RustTypedInPlace`, RFC-0100 D1) and
left the PRICE where it was. Lowering it needs one thing the descriptor does not
carry:

**The Rust GENERIC registration path does not consult the capability.**
`supports_process_in_place` has exactly one call site in `nros-node`, in
`register_subscription_buffered_on`. `register_subscription_buffered_generic`
— what `node_mut(id).subscription(t).generic(ty, hash)` lowers to, and what a
bridge writes — allocates `buffered_region_size(depth, RX_BUF)` unconditionally
on the same backend. An `[[endpoint]]` row carries `(kind, type, topic)` and
says nothing about which of the two spellings the image's code uses, so pricing
the in-place row at zero would under-size every generic subscription on zenoh
and XRCE, which is the direction that ships `NodeError::BufferTooSmall`.

## What a fix has to decide

* whether a generic registration should ALSO take the in-place path when the
  backend offers it — one call site moves, and the saving then needs no new
  fact; or
* whether the descriptor should distinguish a typed endpoint from a generic one,
  which is a property of the CALL SITE rather than of the contract — the same
  limit phase-454 W10 addresses for the C half's `rx_size_bound<M>`.

The first is the smaller change and removes the reason for the second. Either
way acceptance is a measured `mem-report --baseline` on an image with a GENERIC
subscription, not only on a typed one.
