---
id: 2083
title: "On posix, a client never sees a service server that lives in the same process: service_is_ready stays false while ros2 lists the service"
status: open
type: bug
area: [rmw, zenoh, posix, graph]
severity: medium
found: 2026-10-26
related: [2055, 2043, phase-518, phase-488]
---

## What

ros2/demos `composition` (`manual_composition`, phase-488 node 3) runs a
talker, a listener, an `add_two_ints` server and a client of that server in one
posix process. The client never sees the server. Asked on every 2 s timer tick:

```
DBG ready ok=1 val=0 code=0 wait=-2
```

`service_is_ready()` answers `ok(false)` (not `Unsupported`, not
`WouldBlock`), and `wait_for_service(1s)` times out. In the same run, from a
stock ROS 2 Humble peer, `ros2 service list` shows `/add_two_ints`, and
`ros2 service call /add_two_ints` is answered. With the in-process server
replaced by a stock `add_two_ints_server`, the same client sees it on its first
tick and gets `Got result: [5]` every tick.

zenoh-pico's `Z_FEATURE_LOCAL_SUBSCRIBER` and `Z_FEATURE_LOCAL_QUERYABLE` are
both 1 in that build's `zenoh_generic_config.h`.

On FreeRTOS and Zephyr (`mps2-an385`) the same composition's in-process client
reached the in-process server (phase-518 W5: 17 results in one run; on Zephyr
only after `local_queryable = 1` was stated, issue 2043).

## Where to look

`ZenohServiceClient::service_is_ready` answers from the session's graph cache
(`graph_cache_for_each`), which the liveliness subscriber fills. So the
question is why the server's own `SS` token does not reach that cache on
posix: whether the posix image opens one session per node (then the token
travels through the router and should arrive), or one session whose
liveliness subscriber does not see its own session's tokens.

Phase 488 recorded node 3 as running on posix, but only checked it from the
ROS 2 side. Its client edit (issue 2055) polled `service_is_ready()` on each
tick, which this shows never turned true, so the in-process half was never
exercised there.

## Done when

The unmodified composition client, on posix, gets `Got result` from the
in-process server, and a test that runs a client and a server in one process
asserts it.
