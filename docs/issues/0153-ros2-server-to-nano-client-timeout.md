---
id: 153
title: "ros2-server → nano-client service/action lanes time out on goal/response delivery"
status: open
type: bug
area: interop
related: [issue-0146, issue-0150, issue-0151]
---

## Summary

The three remaining `rmw_interop` failures after #151's fixes are all one
direction: a ROS 2 server (service or action) with a nano-ros CLIENT times
out (`Transport(Timeout)`) waiting for the response / goal acceptance —
`test_action_ros2_server_nano_client`, `test_service_ros2_server_nano_client`
(+ the latency variant of that direction). The `nano→ros2` direction all
passes, and the pub/sub interop lanes are green both ways.

Bookkeeping note: issue 0151's resolution attributed these to issue 0150,
but 0150's scope (XRCE session-key collision, bridge type flip, safety
resolver drift, qos-mixed stale objects) is resolved + archived and never
covered this direction — this issue is the live tracker.

## Starting points

- #146's lesson: rmw_zenoh applies publisher-side RxO. A nano client's
  REQUEST reaches the ROS 2 server (server logs receipt?) — verify with the
  server's own logs whether the request arrives and the RESPONSE is
  published; then check the nano client's reply-subscription QoS vs
  rmw_zenoh's response publisher.
- zenoh-pico query/reply path (`zpico_get_*`) vs rmw_zenoh's queryable
  semantics — the nano client uses zenoh queries for services; a ROS 2
  server's rmw_zenoh queryable may reply on a keyexpr/attachment shape the
  shim's reply matcher rejects (issue 0135-class silent drop).
- Discovery time: #151 measured ~10 s first-message latency on this
  machine; the service call timeout budget may simply be shorter than
  rmw_zenoh discovery. Try a pre-warm (sleep after liveliness match) or a
  larger `NROS_SERVICE_TIMEOUT_MS`.

## Repro

```
cargo nextest run -p nros-tests --test rmw_interop -E 'test(ros2_server_nano_client)'
```
