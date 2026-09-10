---
id: 1255
title: "The derived arena prices every subscription slot at ONE global message
  bound, not at each subscription's own type"
status: open
type: enhancement
area: executor, build
severity: medium
related: [issue-1227, issue-1179, issue-1190]
---

## What is true today

Per-type bounds exist and are generated. Codegen emits
`_TX_MAX_SERIALIZED_SIZE` / `_RX_MAX_SERIALIZED_SIZE` per message and a
per-package `nros_message_bounds.cmake` fragment (the chain `6852548c1`,
`afeab01d5`, `36622cadd`, `5c4eed203`). At RUNTIME, C++ `bind_subscription`
already allocates from the type's own bound (phase-392 W3a/W3b).

The BUILD-TIME arena does not use them. They are collapsed into two image-wide
maxima before `nros-node/build.rs` sees anything:

| knob | meaning | reference island |
| --- | --- | --- |
| `NROS_SUBSCRIBER_BUFFER_SIZE` | largest type the image SUBSCRIBES to | 880 |
| `NROS_SUBSCRIPTION_BUFFER_SIZE` | largest type the image LINKS | 1,496 |

and `subs_arena()` (`build.rs:52-83`, issue 1227) prices every declared
subscription as `buffered_region(depth, rx_recv_size) + entry_struct` with the
same `rx_recv_size`. The declared-depth triples are `type|topic=depth`, and the
parse keeps only the depth:

```rust
.filter_map(|t| t.rsplit_once('=').map(|(_, d)| d))
```

so the type is on the wire into the build script and is discarded there.

## Cost

On the reference island (11 subscriptions, all `depth: 1`, so each is a
`TripleBuffer` of three slots) every slot is billed at the 880-byte class.
Most of those subscriptions carry small control/status messages. The over-bill
is `3 x (880 - bound(type))` per subscription, which the arena model already
knows how to compute and does not. The image fits today (DTCM 74.8%), so this
is headroom, not a blocker; it becomes one the next time a subscription is
added to a part with 128 KB of DTCM.

## Why it is open, and who owns it

phase-392 W3c deferred it explicitly (phase-392 "W3 remainder"): a mem-report
delta "needs the BUILD-TIME arena derivation to learn the per-type bounds,
which is `nros-node/build.rs`'s `rx_recv_size` term and
`nros_derive_message_bound_knobs` -- phase-403 step 3's lane, not this one, and
deliberately deferred there". phase-403 step 3 then landed without it. No work
item in phase-394's ledger names it, so nothing currently owns it.

The design question phase-403 recorded still stands: the per-type size
reaches cmake under a BACKEND name (`ZPICO_SUBSCRIBER_BUFFER_SIZE`), which the
backend-agnostic crate must not read. Carrying the bound INSIDE the triple
(`type|topic=depth|bytes`) sidesteps that: one producer (the entity inventory,
which already resolves each type) and one consumer.

## Also stale

`build.rs:335-347` says the receive class is "NOT WIRED YET" and falls back to
the closure knob. It is wired: `nros_cargo_build.cmake:666` forwards
`NROS_SUBSCRIBER_BUFFER_SIZE`, and the reference island's build shows
`PUBSUB_REGION = 9768` (11 x 880 + 88). The comment should go with this fix.

## Acceptance

- `subs_arena()` sums `buffered_region(depth_i, bound(type_i)) + entry_struct`
  over declared subscriptions, and falls back to today's global bound for any
  subscription whose type bound is absent (absence is not zero).
- The host oracle (`cargo:arena_size`) and the island's linked
  `EXECUTOR_BACKING` agree, measured before and after.
