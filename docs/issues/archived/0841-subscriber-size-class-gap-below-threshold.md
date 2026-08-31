---
id: 841
title: "A subscription whose hint lands between the small block size and the size
  threshold gets a block that cannot hold it — and the build error's own remedy
  puts it there"
status: resolved
type: bug
area: rmw
related: [phase-392, phase-403, issue-0757, issue-0776, rfc-0038]
---

## Problem

`alloc_payload_block` routes a subscription to a size class by comparing its
hint against one knob and then hands back a block sized by a *different* one:

```rust
// packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs
if rx_buffer_hint > SUBSCRIBER_SIZE_THRESHOLD {
    ... Some((base, SUBSCRIBER_LARGE_SIZE))
} else {
    ... Some((base, SUBSCRIBER_BUFFER_SIZE))
}
```

At the shipped defaults those two knobs disagree:

| knob | default |
| --- | ---: |
| `ZPICO_SUBSCRIBER_BUFFER_SIZE` (the small block) | 1,024 |
| `ZPICO_SUBSCRIBER_SIZE_THRESHOLD` (the routing test) | 2,048 |

So every hint in **1,025..=2,048** is routed to the small class and handed a
1,024-byte block. The subscription is under-provisioned by up to 2x, and nothing
says so — not at build time, not at configure time. Nothing constrains the two
knobs relative to each other, in `build.rs` or as a `const` assertion.

## Why this is reachable rather than theoretical

`create_subscription`'s build assertion fails a type whose bound exceeds
`NROS_SUBSCRIPTION_BUFFER_SIZE`, and tells the user:

> Raise the knob to at least the type's bound

Follow that advice for a ~1.5 KiB message type — set
`NROS_SUBSCRIPTION_BUFFER_SIZE=2048` — and:

1. the build assertion now passes;
2. the registration passes `rx_buffer_hint = RX_BUF = 2048`;
3. `2048 > 2048` is false, so the subscription takes the **small** class;
4. the block is 1,024 bytes and the 1.5 KiB sample does not fit.

The result is the failure mode the assertion exists to prevent, reached by
doing what the assertion said. `report_dropped_take` describes that class
exactly — *"13.4 KiB Autoware trajectories were silently dropped by every Zephyr
image for the whole life of the lane; small degenerate samples fit the 1 KiB
default, so every green marker stayed green."*

The runtime diagnostic already knows better than the build assertion does: it
names `NROS_SUBSCRIPTION_BUFFER_SIZE`, `ZPICO_SUBSCRIBER_BUFFER_SIZE` **and**
`ZPICO_SUBSCRIBER_LARGE_SIZE`. The build assertion names only the first, which
is the one that does not size the buffer the bytes actually land in.

## Why it stayed hidden

Almost nothing sets `rx_buffer_hint` (phase-392 W3a): the only setter in the
tree is a bench site, and `rust_adapter` passes a literal `0`. A hint of `0`
routes small, and the small block is the right size for it, so the defaults look
consistent until someone raises a knob.

The existing unit test encodes the threshold semantics without noticing the gap:

```rust
let (_b, stride) = alloc_payload_block(SUBSCRIBER_SIZE_THRESHOLD + 1).expect("large alloc");
```

It probes one byte *above* the threshold, so it never visits the window where
the two knobs disagree.

## Fix

Route on whether the hint **fits the small block**, which is the property that
cannot be wrong, rather than on a threshold that is free to exceed it:

```rust
if rx_buffer_hint > SUBSCRIBER_SIZE_THRESHOLD.min(SUBSCRIBER_BUFFER_SIZE)
```

`min` rather than replacing the threshold outright: a consumer may legitimately
set the threshold *below* the block size to push borderline topics into the
large class early, and that should keep working. What must not happen is a hint
routed small that the small block cannot hold.

Landed as a named `SMALL_CLASS_CEILING` const (a `const fn`-style `if`, because
`Ord::min` is not const-callable on stable) so the effective ceiling is written
down once and read at the routing site.

A `const` assertion was written first and deleted: the only thing it could say
was `min(a, b) <= b`, which is true for all inputs. A check that cannot fail
reads as coverage and is worse than none — the same objection this campaign
raised against the vacuous expected-failure compiles in `check-c`.

The test probes *inside* the window (`SUBSCRIBER_BUFFER_SIZE + 1`) and asserts
the property directly — `stride >= hint` — rather than re-deriving the
threshold arithmetic, so it stays meaningful whatever a consumer sets the knobs
to. Verified to fail against the old routing: *"hint 1025 was routed to a
1024-byte block"*.

## What the fix changes for an existing consumer

Someone who raised `NROS_SUBSCRIPTION_BUFFER_SIZE` into the window was, before
this, silently under-provisioned: routed small, handed a block that could not
hold the sample, dropped at the transport. After it, that subscription routes
**large** and works.

The large class has `ZPICO_MAX_LARGE_SUBSCRIBERS` slots (default 2), so a
consumer with more than two such subscriptions now gets
`TransportError::SubscriberCreationFailed` at create time — with the metadata
index rolled back — where they previously got silence. That is the better
failure: an error at creation names a limit and a knob, while a dropped sample
needs a consumer-side packet capture to attribute (which is exactly what the
13.4 KiB Autoware trajectory case cost).

Raising `ZPICO_MAX_LARGE_SUBSCRIBERS`, or `ZPICO_SUBSCRIBER_BUFFER_SIZE` so the
type fits the small class, are both valid answers; the inventory prices each.

## Resolution — fixed, and the top end closed with it

`alloc_payload_block` no longer routes on the threshold alone. It routes on
`SMALL_CLASS_CEILING`, the `min` of the two knobs:

```rust
const SMALL_CLASS_CEILING: usize = if SUBSCRIBER_SIZE_THRESHOLD < SUBSCRIBER_BUFFER_SIZE {
    SUBSCRIBER_SIZE_THRESHOLD
} else {
    SUBSCRIBER_BUFFER_SIZE
};
```

`min`, not a replacement: setting the threshold BELOW the block size stays a
legitimate way to push borderline topics into the large class early. What can no
longer happen is a hint small-classed into a block that cannot hold it, which
was the 1,025..=2,048 window this issue reported and the one the build error's
own remedy walked you into.

**Phase-403 W4 then closed the same defect one class up**, which this issue left
open: a hint above `SUBSCRIBER_LARGE_SIZE` used to route large into a block too
small for it and drop every sample identically. It is now refused at create time
via `LARGEST_PAYLOAD_CLASS`, so the image fails where the person who set the
knobs is standing. That is also what makes `MAX_LARGE_SUBSCRIBERS == 0` legal:
with no large class, a hint past the small block has nowhere legal to go.

Both ends carry tests in `shim/subscriber.rs` — every block handed out can hold
the hint that routed to it, and the top-end refusal.

Closed 2026-09-01 after verifying against the code rather than the issue text;
the fix had landed and only the issue was left open.
