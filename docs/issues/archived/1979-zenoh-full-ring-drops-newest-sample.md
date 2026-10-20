---
id: 1979
title: "zenoh: a full subscriber ring drops the NEWEST sample, silently, which is the opposite of KEEP_LAST"
status: resolved
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

## Resolution (2026-10-21)

**Why the fix direction above was not enough.** The consumer reads a ring
slot IN PLACE: an in-place subscription callback runs on the ring's bytes,
for as long as the callback takes. So the producer cannot simply overwrite
the oldest slot when the ring is full, because that slot may be the one being
read. A seqlock cannot help either, since an in-place callback has already
run on the bytes by the time a torn read could be detected.

**Protocol.** The consumer now claims the head slot before reading it
(`zpico_ring_claim_head`) and consumes it after (`zpico_ring_consume_head`).
Both are in `zpico.c` and take the session's `slot_mutex`, which every
producer of the ring already holds (issue 1956). The consumer holds the lock
only for the claim itself, never while it reads, and an empty ring needs no
claim. The ring descriptor gained three fields: `claimed`, `full_drops` and
`lock_session`.

**Policy on a full ring** (`_zpico_ring_reserve`), under the lock:

- Head not claimed: evict the oldest sample and write the new one at tail.
  This is KEEP_LAST.
- Head claimed and depth >= 2: the consumer is reading the oldest, so replace
  the newest queued sample in place. The burst's final value still arrives;
  one older sample is lost instead of the oldest.
- Head claimed and depth 1: the incoming sample is dropped.

Each case increments the ring's `full_drops`. The process-wide sum is
`nros_rmw_zenoh::ring_full_drops_total()`, separate from
`overflow_drops_total()`, which still counts oversized payloads only.

The Rust test producer (`simulate_subscription_callback`) used to mirror the
C policy. It now calls the C producer through `zpico_ring_push_bytes`, so
the unit tests exercise the policy that ships rather than a copy of it.

**Tests.**

- Unit: `sub_buf_full_ring_keeps_the_newest` and
  `sub_buf_full_ring_never_writes_the_claimed_slot` replace
  `sub_buf_ring_full_drops_excess`, which asserted the old behaviour.
  Restoring drop-newest fails both. Ignoring the claim fails the second.
- End to end: `a_burst_into_a_slow_subscription_ends_on_its_newest_sample`
  in `nros-tests/tests/entity_wake.rs` runs the measured scenario (25
  samples at 500 µs into a 2 ms callback) for both dispatch shapes. It also
  asserts that `ring_full_drops_total()` moved, as the control that the ring
  filled. Against the old policy it failed 3 of 3 runs in both shapes. With
  the fix, the whole `entity_wake` suite (8 tests) passed 6 of 6 runs.

W5's race test still paces its publisher, now so that it stays a test of
dispatch rather than of eviction.
