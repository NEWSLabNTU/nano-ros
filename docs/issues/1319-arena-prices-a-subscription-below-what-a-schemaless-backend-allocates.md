---
id: 1319
title: "The arena prices a subscription at the SUBSCRIBED payload class, and a backend without type descriptors allocates the CLOSURE buffer — the model is below the allocation on zenoh and XRCE"
status: open
type: bug
area: executor, build
severity: medium
found: 2026-09-11
related: [issue-1255, issue-1190, phase-403, phase-448]
---

## What the model says and what the runtime does

`nros-node/build.rs` prices a subscription's receive region at `rx_recv_size`
(`NROS_SUBSCRIBER_BUFFER_SIZE`, the largest bound among the types the image
SUBSCRIBES to) and, since issue 1255, at each type's own bound where the bound
table reaches it. Both are at or below the CLOSURE buffer
`NROS_SUBSCRIPTION_BUFFER_SIZE` (`DEFAULT_RX_BUF_SIZE`, the const generic
`RX_BUF`). On the reference island that is 880 against 1,496.

What the runtime allocates depends on the registration path, and the four are
not the same:

| path | slot size |
| --- | --- |
| C/C++ typed, `rx_buffer_hint != 0` (`nros::rx_size_bound<M>`) | the type's own `_RX` — matches the model |
| Rust typed, backend WITH type descriptors (Cyclone) | `min(transport_framed(bound), RX_BUF)` — at or below the model |
| Rust typed, backend WITHOUT type descriptors (zenoh, XRCE) | **`RX_BUF`** |
| C/C++ raw with no hint | **`RX_BUF`** |

The last two rows are the gap. `default_subscription_rx_bytes::<M>` has two
arms, and the `#[cfg(not(rmw_needs_type_descriptors))]` one returns `None` for
every type — correctly, because `MessageForRmw` carries no schema there and no
bound is reachable at a type-erased site. `spin.rs`'s
`register_subscription_buffered_on` then takes `.unwrap_or(RX_BUF)`. So on a
zenoh or XRCE image every default-registered Rust subscription claims
`buffered_region(depth, RX_BUF)` while the arena budgeted
`buffered_region(depth, rx_recv_size)`.

On the island's numbers, depth 1, that is `3 x (1496 - 880)` = **1,848 bytes
per subscription** the model does not hold.

## This predates issue 1255 and is not caused by it

The term has read `NROS_SUBSCRIBER_BUFFER_SIZE` since phase-403 step 3, whose
own comment argues the case: "`register_subscription_buffered_*` sizes it from
the type's own bound, and the SUBSCRIBER payload class is the maximum over
subscribed types, so it is a tight upper bound for every subscription". That
sentence is true of the C/C++ typed path and of a descriptor-carrying backend,
and it is false of the other two rows. Issue 1255 narrowed the same term from
the class maximum to the per-type bound; it inherited this gap rather than
introducing it, and the per-type table is `min`-safe against the second row
(`min(framed(bound), RX_BUF) <= bound`), so the gap is exactly the two `RX_BUF`
rows and no wider than it was.

## Why it has not bitten

`arena_model::REQUIRED` is non-zero only where every `NROS_ENTITY_COUNT_*`
arrives, which today is the Zephyr resolver road alone; elsewhere the
derivation keeps the pre-step-3 worst case, which budgets every slot at the
action-client entry and is far above this. The images that DO declare are the
ones at risk, and they are the memory-tight ones.

## What a fix has to decide

Not "raise the term back to `RX_BUF`" — that gives up issue 1255's saving on
the path where the per-type bound IS what is allocated. The question is which
of these:

* make the schemaless arm's default match the model — i.e. let a Rust typed
  registration on zenoh/XRCE reach the type's bound the way the C++ side does
  through `rx_size_bound<M>`. The bound exists (codegen emits it); what is
  missing is a way to read it at a site whose `MessageForRmw` has no schema;
* have the build model the REGISTRATION PATH, which it cannot see today;
* state the shortfall as a per-image margin the derivation adds, which is the
  answer that goes stale next time a path changes.

## Reproduction

Not reproduced as a failing image — the analysis is from the source
(`rmw_type_registry.rs::default_subscription_rx_bytes`, `spin.rs`'s
`register_subscription_buffered_on` and `add_arena_subscription_c_callback`).
An image that declares its entities, subscribes from RUST on zenoh, and sets
`NROS_SUBSCRIBER_BUFFER_SIZE` below `NROS_SUBSCRIPTION_BUFFER_SIZE` is the
shape that would show it, as `NodeError::BufferTooSmall` at a registration the
arena oracle passed.
