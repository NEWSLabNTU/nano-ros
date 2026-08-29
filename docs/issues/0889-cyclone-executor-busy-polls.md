---
id: 889
title: "The Cyclone RMW installs no wake callback, so the executor polls on a
  timer and mostly misses"
status: open
type: bug
area: rmw
related: [issue-0836, issue-0780, phase-385]
---

## Symptom

`nros-rmw-cyclonedds/src/vtable.cpp` leaves the slot empty:

```c
/*set_wake_callback*/         nullptr,
```

so `Executor::has_async_wake` stays `false` and `spin_once` never uses its wake
primitive. The wait becomes a deadline-bound sleep and the runtime polls every
reader on a timer, whether or not anything arrived.

Measured on the ASI an536 lane (`rhc` tracing, ~5 minute run, five
subscriptions, 5 ms poll interval):

```
take_w_qminv(reader)          5069 per reader
update_conditions_locked      42   per reader   (deliveries)
```

**0.8% of takes find data.** The other 99.2% are the executor asking five
readers, 200 times a second, whether anything happened.

## Why it matters beyond the wasted cycles

The executor's own comment already names the cost, from a different backend:

> Poll-only backends (XRCE-DDS-Client, current Cyclone/dust-DDS shims) leave
> this `false`; the wait then becomes a no-op sleep that starves reliable
> retransmission (Phase 127.C.4 root cause: … a blind `wait_ms(100)` sleeps
> with zero session activity, so the agent's ACK arrives into a stalled session
> and reliable redelivery never fires).

Cyclone is less exposed than XRCE — it runs its own `recv`/`tev` threads, so
protocol progress does not depend on the executor polling. But on an emulated
single-core target those wasted polls compete with exactly those threads, and
this is the same lane where issue 0836 loses samples with every layer reporting
zero drops. **Not proven connected**; worth stating as a hypothesis rather than
leaving the coincidence unmentioned.

## Attempted fix, and why it is not in this commit

The natural implementation is a participant-level `data_available` listener:
one listener covers every reader (DDS propagates an unhandled event to the
parent), it fires on Cyclone's delivery thread, and the runtime callback is
documented as safe from a foreign thread — it does a flag write plus a condvar
signal and nothing else. That is precisely the distinction from the STATUS
listeners `subscriber.cpp` declines, which would need a buffer, a lock and a
delivery context.

Implemented and building:

* `SessionState` gains `wake_cb` / `wake_ctx` / `listener`
* `session_set_wake_callback()` installs `dds_lset_data_available_arg(...,
  reset_on_invoke = false)` on the participant, clears on `cb == nullptr`, and
  is torn down from `session_destroy` before the ctx can go away
* the vtable slot points at it

**It is not landed because it could not be verified.** One run with it showed
zero deliveries — but so did a run with the change reverted. The an536 lane's
run-to-run variance is large enough to swamp the signal (documented in 0836:
3,200 frames in one run against 84,524 in another of the same length), so
neither run proves anything about the patch.

There is also a real mechanism for harm that a stable rig must rule out:
invoking a listener consumes the communication status, and this backend's
`take`/`has_data` path polls that status. `reset_on_invoke = false` is meant to
prevent exactly that, and needs to be confirmed rather than assumed.

## What would settle it

A rig where delivery is repeatable — a native or QEMU lane with a fixed
publisher, not the full Autoware stack — running the poll/delivery counters
above with and without the listener. If the hit rate rises and delivery is
unchanged, land it; if `has_data` goes quiet, the status-consumption concern is
real and the wake path needs a waitset on its own thread instead.
