---
id: 2060
title: "A wake-attributed subscription's status events are never polled without an arrival: its matched count does not fall when the last publisher leaves"
status: resolved
resolved: 2026-10-26
type: bug
area: [api, rmw]
severity: medium
found: 2026-10-25
related: [2039, 1972, 1993, phase-488]
---

## What

Found by issue 2039's interop cell, `native-qos-event-c-zenoh-bidir`
(`tests/qos_event_interop.rs::c_matched_and_liveliness_callbacks_follow_stock_ros2_peers`).

The cell runs a C probe against stock ROS 2 peers. A stock `ros2 topic pub`
starts, and the C subscription's matched callback reports `current=1
change=1`. When that publisher is killed, no `current=0` ever arrives. The run
waited 40 s, and the result was the same on three runs. The publisher side does
fall: `PUB_MATCHED current=0 change=-1` arrives within seconds of the stock
`ros2 topic echo` leaving.

## Why (from the code, consistent with the measurement)

The two sides have different poll points.

- **The zenoh shim** polls a subscription's statuses inside `has_data`
  (`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs`, `has_data`):
  `check_deadline_and_fire`, `check_liveliness_and_fire` and the matched
  `MatchWatch::poll` all run there.
- **The executor** skips `has_data` for a subscription whose wake is
  ATTRIBUTED (it registered an entity waker) when nothing arrived
  (`packages/core/nros-node/src/executor/spin.rs`, the readiness scan:
  `pushed && recheck & bit == 0` gives `false`, so it does not probe). The
  first spin after the last sample still probes, through `entity_recheck`;
  after that, nothing does.

So once the last publisher has gone, nothing ever asks. That covers the
matched count falling, the liveliness `alive_count` dropping, and a
`RequestedDeadlineMissed` for a publisher that went silent. The last one is the
event whose whole point is "no sample arrived".

The publisher side is unaffected because `rcl_publish` is its poll point.

The same skip applies to every language: the executor is shared. The cell
measured it through C.

## Done when

- A wake-attributed subscription's status events are polled even with no
  arrival. Two possible routes:
  - the executor probes a slot that has event callbacks registered at a bounded
    cadence;
  - the backend wakes the entity when the graph changes, for example from its
    liveliness subscriber.
- `qos_event_interop.rs` asserts the subscription fall: today it prints
  `subscription matched fall … not reported (issue 2060)` and does not assert.

## Resolution

Fixed 2026-10-26, by the first route: the executor asks the slot.

- **The executor.** `Executor::entity_status_polled` is a bit per slot. While
  a slot's bit is set, the readiness scan calls that slot's `has_data` on every
  pass, as if it were a recheck that never drains (`spin.rs`, the scan's
  `recheck` fold). The bit is set by `Executor::subscription_handle_mut`, which
  is the only road a status-event callback has to an arena subscription's
  backend handle (the C setters use it). It is cleared when the entry is
  released. A subscription with no status callback never gets the bit, so an
  idle executor still makes no `has_data` call for it (the phase-397 W3
  property) and no wake is added. The cadence is the spin's own, one probe per
  pass, and each backend rate-limits its own walk inside `has_data`.
- **Why not the backend-wake route.** It cannot cover
  `RequestedDeadlineMissed`. That event is about time passing with nothing on
  the wire, so no graph change could ever wake it. A wake would also land
  through the arrival bit, and the arrival bit does not call `has_data`.
- **The class.** Every status kind is polled there: zenoh's deadline,
  liveliness and matched/incompatible-type walk, and Cyclone's
  `subscription_take_event` drain through the cffi adapter's `has_data`. The
  executor fix covers all of them on both backends. XRCE and uORB carry no
  subscription status events and do not attribute wakes, so their slots were
  always probed. The zenoh shim's three polls now live in one
  `poll_status_events()`. `take_serialized` calls it as well as `has_data`; it
  used to run only the deadline check, so a poll-road caller that only takes
  never saw a matched or liveliness change.

**Tests.**

- `packages/api/nros-c/tests/run/subscription_status_poll.c`, in
  `just check c`, runs on the stub backend. The stub gained a `has_data` call
  counter and a count event that fires from inside the next `has_data`. The
  test makes three checks:
  - with no status callback, 5 idle passes make 0 `has_data` calls (the
    control);
  - with a deadline callback, an event staged with no arrival is delivered on
    the next pass, and each of 5 idle passes asks;
  - after remove and re-add, the slot is unprobed again.
- Mutations, measured:
  - dropping the fold fails 2 checks (0 deliveries, 0 probes);
  - not clearing the bit on release fails 1 check (5 probes after re-add).
- **Live.** The interop cell `native-qos-event-c-zenoh-bidir` now asserts the
  subscription fall. It needs subscription matched current 0 with a negative
  change, and a `LivelinessChanged` with `alive_change < 0`, after the stock
  publisher leaves.
  - It passed in the `ros2` box (humble, `rmw_zenoh_cpp`, `rmw_zenohd`) in
    18.5 s and in 4.7 s.
  - With the fold removed and the fixture rebuilt, it failed after the 40 s
    wait. The transcript ended on `SUB_MATCHED current=1` and
    `LIVELINESS alive_change=1`.
- `entity_wake` (10 cases, including
  `a_pushed_spin_dispatches_without_probing_readiness` and
  `an_idle_member_parks_through_kilohertz_arrivals_for_another`) is unchanged
  and green.

## Still open

- Cyclone DDS's `has_data` always answers "maybe". On Cyclone, a subscription
  with a status callback is therefore dispatched on every pass, and when
  nothing is waiting the take finds nothing. That is the pre-phase-397 cost,
  paid only by subscriptions with a status callback. It was not measured live
  on Cyclone.
- The bit is set when the handle is taken, before the backend has accepted
  the registration. A refused registration costs one probe per pass. The C
  setters check `supports_event` first, so a refusal is rare.
- A poll-road (C++ `PollSubscription`) caller observes events only when it
  calls take. That is the poll road's contract, not this bug.
