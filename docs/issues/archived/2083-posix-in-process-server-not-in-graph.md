---
id: 2083
title: "On posix, a client never sees a service server that lives in the same process: service_is_ready stays false while ros2 lists the service"
status: resolved
type: bug
area: [rmw, zenoh, posix, graph]
severity: medium
found: 2026-10-26
related: [2055, 2043, phase-518, phase-488]
resolved: 2026-10-27
resolved_in: "the PR that archives this file"
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

## Cause

zenoh-pico never delivers a session's own liveliness tokens to that session's
own liveliness subscriber. `z_liveliness_declare_token` sends the declaration
to the router and records the token in `_local_tokens`. The `history` replay
(`_z_liveliness_subscription_trigger_history`) walks only `_remote_tokens`,
and its source says `// TODO: add support for local tokens`. So the shim's
graph cache, the one standing liveliness subscriber, held every entity
except this session's own. `service_is_ready` answered `false` for a server
the client's own process hosted, while every other process saw it. Upstream
`rmw_zenoh_cpp` sees its own entities because zenoh-c delivers local tokens.

The RTOS images were not immune: they only looked fine. Phase 518 W5's
FreeRTOS composition got results because its client does not gate on the
graph.

## Fix (2026-10-27)

The shim records the session's own tokens into its own graph cache
(`zpico.c`):

- `zpico_declare_liveliness` keeps the token's keyexpr (heap, only with graph
  discovery compiled in) and applies a PUT to the session's cache;
- `zpico_undeclare_liveliness` applies the DELETE and frees the key;
- `zpico_graph_cache_start` seeds the cache with the tokens declared before
  it started, which the `history` replay never sends;
- the liveliness subscriber's samples and these local changes go through one
  apply, `graph_cache_apply`, which fires the graph-change edge for both.

A duplicate (a router that does echo a token back) is a no-op through the
cache's set semantics.

### Measured

- ros2/demos `composition` (`manual_composition`, unmodified upstream
  sources), posix: the in-process client gets `Got result: [5]` from the
  in-process server on every tick. Before the fix it printed `Service not
  available after waiting` on every tick.
- `nros-rmw-zenoh/tests/in_process_graph.rs`, a client and its server on one
  session, live router: `service_is_ready` turns true when the server is
  declared and false when it is dropped. With the two local applies removed
  it fails at the 5 s deadline. It runs in `just check required-features-tests`
  (the router lane beside `matched_events_live`).
