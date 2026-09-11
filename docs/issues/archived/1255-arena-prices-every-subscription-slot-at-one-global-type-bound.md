---
id: 1255
title: "The derived arena prices every subscription slot at ONE global message
  bound, not at each subscription's own type"
status: resolved
resolved_in: phase-448 W7
type: enhancement
area: executor, build
severity: medium
related: [issue-1227, issue-1179, issue-1190, issue-1319, phase-448]
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

## Resolution (phase-448 W7)

`subs_arena()` now sums `buffered_region(depth_i, bound(type_i)) + entry_struct`
over the declared subscriptions. Three pieces:

* **The producer.** `_nros_bounds_join_subscribed` in
  `cmake/NanoRosMessageBounds.cmake` already visited each subscribed type with
  its `_RX` in hand, to compute the three payload CLASS knobs. It now also
  hands back the table those classes are the maximum over, published as
  `NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS` and written into
  `message_bound_knobs.cmake`. `subscribed` basis only -- on the `closure`
  basis there is no subscribed set, so there is no per-subscription table to
  publish. A REFUSED join publishes none either: a partial table would price
  the subscriptions it happened to cover and silently leave the rest.
* **The wire.** `zephyr/cmake/nros_cargo_build.cmake` resolves it beside the
  declared depths, on the derivable ladder with the `-1` sentinel passed
  literally (there is no Kconfig symbol -- a per-type table is not a number a
  person states). It has to be `_nros_resolve_derivable_knob` and not
  `_nros_resolve_knob`: the resolver road is HARVESTED from those call sites,
  and a fact delivered by any other spelling is invisible to
  `check-declared-fact-carriers`.
* **The consumer.** The parse keeps the type it used to throw away
  (`.filter_map(|t| t.rsplit_once('='))` then splitting the head on `|`), and
  looks each one up. **Absence is not zero at either level**: a type the table
  does not price keeps `rx_recv_size`, today's image-wide bound, so a missing
  table reproduces the old number byte for byte and a partial one is exact
  where it can be and unchanged where it cannot. A bound that parses as `0` is
  discarded rather than believed.

### Measured

`cargo check -p nros-node` on the island's SHAPE (11 subscriptions, every one
KEEP_LAST(1), `NROS_SUBSCRIBER_BUFFER_SIZE=880`), with the model read off the
arena oracle's own refusal rather than restated:

| bound table | modelled arena |
| --- | ---: |
| absent (the pre-W7 number) | 48,752 B |
| complete -- ten types at 16 B, one at 880 B | **22,832 B** |
| half absent -- five of the ten named | 35,792 B |

The 25,920-byte drop is exactly `10 x 3 x (880 - 16)`, and the partial run is
exactly 22,832 + `5 x 3 x 864` -- which is what "absence keeps the image-wide
bound" means arithmetically.

The island itself is an out-of-tree consumer and was not built here; what was
reproduced is its shape, on the host, with its numbers.

### The oracle, kept honest

`just check node-std-tests` runs the three builds above and compares them, and
it reads every number out of the refusal message rather than carrying a copy of
the model. Mutation-tested both ways on 2026-09-11:

* a lookup that always misses -> "the per-type bound table changed nothing --
  the model is 48752 B with it and 48752 B without";
* absence priced at zero -> caught by the same comparison (the fallback run
  drops to 19,712 B, below the per-type run).

`cmake-message-bounds-tests.sh` covers the producer (the join hands back
`std_msgs/msg/Int32=12`; a refused join hands back nothing; the OUTPUT FILE
carries it), mutation-tested by deleting the `list(APPEND _type_bounds ...)`,
which reds 2 of 103 assertions.

### The stale comment

`build.rs`'s "NOT WIRED YET" paragraph about `NROS_SUBSCRIBER_BUFFER_SIZE` is
gone, as this issue's "Also stale" section asked. It is wired, under the
backend-agnostic name, and the island's `PUBSUB_REGION = 9768` (11 x 880 + 88)
is the evidence.

### Not fixed here

Issue 1319: the model prices a subscription at or below the SUBSCRIBED class
while two registration paths -- a Rust typed subscription on a backend with no
type descriptors, and a C/C++ raw one with no hint -- allocate the CLOSURE
buffer. That gap predates this change and is not widened by it (the per-type
bound is at or above `min(framed(bound), RX_BUF)`), but it is the next thing to
answer for an image that declares.
