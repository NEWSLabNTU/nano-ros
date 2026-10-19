---
id: 1979
title: "zenoh: a full subscriber ring drops the NEWEST sample, silently, which is the opposite of KEEP_LAST"
status: open
type: bug
severity: medium
area: [rmw, zenoh]
found: 2026-10-19
related: [phase-397]
---

## Measured

Found while writing phase-397 W5's race test
(`nros-tests/tests/entity_wake.rs`,
`an_arrival_racing_a_drain_is_never_left_undispatched`). The first draft
published bursts of 25 samples, one every 500 µs, to a subscription whose
callback took 2 ms. The last sample of the first burst was never dispatched:
`latest 23` (and `latest 22` on a second run), with the burst fully published.

It also failed with the executor's readiness scan restored (W3 disabled), so
the executor does not cause it. The sample never reaches the executor.

## Cause

`zpico.c`'s sample handler (the ring branch, "Ring full — drop") refuses a
sample when `tail - head >= slot_count`, i.e. it keeps the OLDEST samples and
drops the incoming one. The Rust test producer
(`simulate_subscription_callback` in `shim/subscriber.rs`) mirrors this.
The default ring depth is 4 (`SUBSCRIBER_RING_DEPTH_DEFAULT`).

Two things are wrong with that for a `KEEP_LAST` subscription:

- **It keeps the wrong end.** `KEEP_LAST(n)` keeps the newest `n` samples. A
  slow consumer of a burst therefore ends on stale data, and the final value
  of the burst never arrives at all. For a state topic (a pose, a setpoint),
  the newest sample is the only one that matters.
- **It is silent.** `overflow_drops_total()` counts only oversized payloads
  (`len > payload_stride`), not ring-full drops. The C comment says a dropped
  tail "is reported via msg-lost accounting on the Rust side from the
  sequence gap". Nothing in the executor or the test surface reports it.

## Fix direction

Overwrite the oldest slot on a full ring (advance `head` from the producer
side), which needs the SPSC protocol to tolerate a producer-side head move,
or a seqlock per slot. Alternatively, keep drop-newest for `KEEP_ALL` only.
Either way, count ring-full drops beside the oversize ones.

## Workaround

W5's test paces the publisher slower than the callback, so the ring never
fills, and says why in a comment.
