---
id: 2060
title: "A wake-attributed subscription's status events are never polled without an arrival: its matched count does not fall when the last publisher leaves"
status: open
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
