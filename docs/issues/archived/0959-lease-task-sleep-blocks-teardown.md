---
id: 959
title: "The lease task sleeps for a whole keep-alive interval, so stopping a session waits it out — 20 s per teardown after #0906"
status: resolved
type: bug
area: rmw-zenoh, testing
related: [issue-0906, issue-0854, issue-0835]
---

## What happens

`_zp_unicast_lease_task` sleeps for its whole computed interval:

    z_sleep_ms((size_t)interval);

`_zp_unicast_stop_lease_task` clears `_lease_task_running` and then JOINS, so a
task asleep at that moment makes teardown wait out whatever remains of the
interval. The interval is `lease / Z_TRANSPORT_LEASE_EXPIRE_FACTOR`.

[[issue-0906]] raised `Z_TRANSPORT_LEASE` from 10 s to 60 s so the client would
stop expiring against the ROS router's 30 s keep-alive cadence. That was correct
for the protocol, and it moved this latency from 3.3 s to **20 s per session
teardown**, on every platform.

## Measured

One native test, the only variable being the lease constant:

    Z_TRANSPORT_LEASE = 60_000   ->  test_pubsub_loopback  20.16 s
    Z_TRANSPORT_LEASE = 10_000   ->  test_pubsub_loopback   3.49 s

The 16.7 s difference is exactly `60000/3 - 10000/3`. The test's own body sleeps
3 s and the router's readiness probe is event-driven (a 100 ms poll that returns
on first connect), so the remainder was teardown and nothing else.

## What it cost

`target/nextest/default/junit.xml` from a ci-l1 run: 1028 tests, **129 s total,
of which six tests were 20.2 s each — 121 s, 94 % of the suite.** All six are the
`zenoh_integration` cases that open a session against a private router; the
tests in the same file that do not open one run in 0.003 s.

`ci-l1` runs before every push.

## Fix

Sleep in bounded chunks and re-read the running flag between them:

    size_t remaining = (size_t)interval;
    while (remaining > 0 && ztu->_common._lease_task_running) {
        size_t chunk = MIN(remaining, Z_TRANSPORT_LEASE_TASK_SLEEP_CHUNK_MS);
        z_sleep_ms(chunk);
        remaining -= chunk;
    }
    interval = interval - (int)remaining;   /* account only what was slept */

The loop already decremented `next_lease` / `next_keep_alive` by whatever it
slept, so this is local: **the keep-alive schedule is unchanged**, only the
granularity at which the stop flag is observed. `Z_TRANSPORT_LEASE_TASK_SLEEP_
CHUNK_MS` defaults to 1000 — one wakeup per second on an otherwise idle task,
against a twenty-second join.

Bounding teardown at ~1 s rather than at the lease also decouples the two: the
lease can now be sized for the peer's cadence, which is what #0906 needed, without
that choice showing up as test latency.

## Result

    zenoh_integration, whole binary:  ~121 s  ->  4.16 s   (stable across runs)

14 passed, 1 skipped. (The "1 failed" a bare `cargo nextest` prints is the
`nros_tests::skip!` panic for `ZPICO_MAX_SESSIONS=1`; only `just test-all`'s
junit rewrite renders it as a skip.)

## How it was found

Chasing [[issue-0854]] — "compare in-sweep against solo across the suite", the
measurement that issue says nobody has done. The junit gave the in-sweep half.
Running the slowest test solo returned 20.16 s, IDENTICAL to in-sweep, which
falsified the starvation hypothesis for these six immediately: a test that takes
the same time alone is not contending for anything.

Worth keeping as method. The cheap half of "in-sweep vs solo" is re-running one
slow test alone, and it separates "slow" from "starved" in a single measurement.

## Acceptance

* ~~Session teardown does not scale with the lease.~~ Met: bounded at the chunk.
* ~~The keep-alive schedule is unchanged.~~ Met: accounting decrements by time
  actually slept.
