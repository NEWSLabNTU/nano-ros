---
id: 1980
title: "The entity-wake race test reports a lost dispatch when its own publish failed, and the publish failed in a tier-1 run on main"
status: resolved
resolved: 2026-10-23
resolved_in: "PR #2492"
type: bug
area: [testing, runtime, zenoh]
severity: medium
found: 2026-10-19
related: [phase-397, 1979, 1956]
---

## Measured

- Run: host-tests 39194192517 (tier 1, schedule, 2026-10-19 03:20 UTC).
- Job: 117873262543, `tier-1 gates (just check)`. The failing gate is
  `required-features-tests` (`check-build (parallel): 1 of 26 gate(s) FAILED`).
- The one failing test:
  `nros-tests::entity_wake an_arrival_racing_a_drain_is_never_left_undispatched::case_1_drains_to_empty`,
  at 5.215 s. Its sibling `case_2_one_sample_per_dispatch` passed in the same run.
- Two panics, in this order:

```
panicked at packages/testing/nros-tests/tests/entity_wake.rs:507:13:
DrainsToEmpty round 0: sample 3, published while a callback ran (None), was never dispatched (latest 2); an arrival that raced a dispatch was lost

thread '<unnamed>' panicked at packages/testing/nros-tests/tests/entity_wake.rs:475:47:
publish: Transport(PublishFailed)
```

The test landed with #2408 (phase-397 W5), merged a few hours before this run.

### Second sighting, 2026-10-20

- Run: gate 39356789492 (schedule, 2026-10-20 02:03 UTC, main at
  `3ed9ed306`).
- Job: 118413362232, `check / build tier + no-std (nightly/manual only)`,
  step `just check build`. The failing gate is again
  `required-features-tests` (`check-build (parallel): 1 of 26 gate(s) FAILED`).
- This time BOTH cases failed, at 5.35 s and 15.33 s, and each one has the same
  pair of panics:

```
DrainsToEmpty round 8: sample 27, published while a callback ran (None), was never dispatched (latest 26); an arrival that raced a dispatch was lost
panicked at packages/testing/nros-tests/tests/entity_wake.rs:475:47:
publish: Transport(PublishFailed)

OneSamplePerDispatch round 11: sample 36, published while a callback ran (None), was never dispatched (latest 35); an arrival that raced a dispatch was lost
panicked at packages/testing/nros-tests/tests/entity_wake.rs:475:47:
publish: Transport(PublishFailed)
```

This weakens the "session not yet ready when round 0 begins" reading under
item 2 below. These failures came in rounds 8 and 11, after 26 and 35 samples
had already been published and dispatched. So the publish can fail well into
a session that was already working. It is still the publisher thread
panicking, with `(None)` in the verdict, so the item 1 test fix applies
unchanged. The red is also not limited to the tier-1 lane: the scheduled
gate's build tier runs the same gate.

### Third sighting, 2026-10-22

- Run: host-tests 39716043967 (tier 1, schedule, 2026-10-22 03:17 UTC, main
  at `350203f0b`).
- Job: 119618277087, `tier-1 gates (just check)`. The failing recipe is again
  `required-features-tests`, and the lane ends `ci tier1-gates FAILED at step 1
  of 1 (check::default)`.
- Only `case_1_drains_to_empty` failed, at 5.26 s; `case_2` passed (5.50 s).
  The nextest summary reads `31 tests run: 30 passed, 1 failed` and the junit
  rewrite reads `Real failures: 1 / 1`.
- The same pair of panics, now at lines 510 and 478 (the file grew by three
  lines):

```
DrainsToEmpty round 0: sample 3, published while a callback ran (None), was never dispatched (latest 2); an arrival that raced a dispatch was lost
panicked at packages/testing/nros-tests/tests/entity_wake.rs:478:47:
publish: Transport(PublishFailed)
```

Three sightings in four days, each with the publish failing first. This is a
recurring tier-1 red, not a one-off.

## Reading

The verdict blames the executor, but the evidence points at the publish. The
`<unnamed>` thread is the publisher. Line 475 is its
`publish_raw(...).expect("publish")`. It panicked before it could send on
`raced_tx`, which is why the main loop reports the race as `None`.

So in round 0, sample 3 (the burst's last sample) was most likely never
published. The main loop waited out `ARRIVAL_BUDGET` for a sample that did
not exist, then printed the "an arrival that raced a dispatch was lost"
message. The test then reports a lost dispatch for what was a failed publish.

The order of the two panic messages is not evidence by itself, because
stderr from two threads interleaves. But `(None)` is: `raced_tx` sends only
after `publish(last)` returns.

## What it is NOT

- Not shown to be an executor defect. Nothing here says an arrival was
  dropped.
- Not issue 1979 (a full ring drops the newest sample). That needs 4 samples
  queued, and this burst stopped at sample 3, with the publish itself failing.
- Not stale-fixture noise: tier 1 builds and tests in the same job.

## What would close it

1. **The test reports what happened.** The publisher thread's failure must
   reach the verdict. Join it, or send its `Result` back on the channel, and
   fail with "publish failed: …" rather than the lost-arrival message. That
   is a test fix, and it separates the two failures from now on.
2. **Then the publish failure gets its own answer.** Find why `publish_raw`
   returned `Transport(PublishFailed)` in a loaded tier-1 job. The same job
   also runs the parallel `check build` gates. It may be zenoh-pico's TX path
   refusing under load, or a session not yet ready when round 0 begins. The
   test publishes immediately after `create_publisher_raw`, with no wait for
   the session to settle.

## Resolution (2026-10-23)

Two test defects, both reproduced locally under CPU load (`2 x nproc` busy
loops), neither an executor defect.

1. **The publish failure is zenoh-pico's DROP congestion control.** A
   publisher declared without `ZPICO_TX_BATCH` keeps zenoh-pico's default
   `Z_CONGESTION_CONTROL_DROP`, and `_z_transport_tx_send_n_msg` then only
   TRY-locks the transport TX mutex. When another thread holds it (the lease
   task's keep-alive, descheduled mid-write on a loaded host), the put is
   dropped and `z_publisher_put` returns an error, which reaches the test as
   `Transport(PublishFailed)`. Measured: 1 of 25 loaded runs, with exactly this
   issue's signature. The test now retries a dropped publish for up to 500 ms,
   and a publish that stays dropped fails the test as "publish failed", not
   as a lost arrival (item 1 above).
2. **A second race hid behind it.** The arrival budget started at the round's
   start, but the publisher itself waits up to the same `ARRIVAL_BUDGET` for a
   callback to publish into. A publisher descheduled past the previous
   sample's 3 ms callback waited the full 5 s, and the main loop's budget ran
   out at the same moment. That produced the same "was never dispatched"
   message with no publish failure (1 of 25 loaded runs once item 1 was
   fixed). The budget now runs from the last sample's publish.

Whether a RELIABLE publisher should keep zenoh-pico's DROP, which drops on
mere TX-lock contention rather than on a full queue the way zenoh's DROP does,
is a product question this fix does not settle. Filed as
[issue 2001](../2001-zpico-reliable-publish-dropped-on-tx-lock-contention.md).
