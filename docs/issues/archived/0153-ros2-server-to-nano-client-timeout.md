---
id: 153
title: "ros2-server → nano-client service/action lanes time out on goal/response delivery"
status: resolved
type: bug
area: interop
related: [issue-0146, issue-0150, issue-0151]
resolved_in: (this commit) — three stacked defects
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

## Resolution (2026-07-08) — three stacked defects

1. **Missing rmw attachment on queries** (product bug, the deep one). The
   zpico shim's `z_get` carried NO attachment; rmw_zenoh_cpp's
   `service_take_request` REQUIRES the (sequence_number, source_timestamp,
   gid) attachment and errors the whole take without it — the request
   reached the ROS 2 server and died inside rcl (`service failed to take
   request`, observed in the server's own traceback), so the client only
   ever saw `Transport(Timeout)`. nano↔nano services tolerate a missing
   attachment, which kept this invisible in-tree. Fix:
   `zpico_get_start_with_attachment` (C shim + sys bindings +
   `Context::get_start_with_attachment`) and the zenoh service client now
   builds the same 33-byte attachment as the publisher path (per-client
   gid + request sequence counter). Actions ride the same client machinery.

2. **Liveliness-vs-queryable gossip gap** (demo-client robustness). The
   server's liveliness token (what `wait_for_service` /
   `wait_for_action_server` observe) gossips ahead of its queryable route;
   a `z_get` fired in that window matches no queryable and completes
   instantly with no reply, so the demo clients' three tight retries all
   burned out inside the same gap. Fix: 1 s backoff between attempts in the
   service-client and action-client demos; the action client also clears
   the send-goal in-flight flag on a timed-out acceptance
   (`reset_send_goal_in_flight`) or the retry dies on `RequestInFlight`.

3. **Action test type mismatch** (test defect, 233.6 class). The zenoh
   `action_server_fibonacci` helper ran the stock `action_tutorials_py`
   server, which serves `action_tutorials_interfaces/action/Fibonacci` — a
   DIFFERENT type from the `example_interfaces` Fibonacci the nano client
   speaks, so the send_goal keyexpr never matched. The DDS variant
   (`action_server_fibonacci_with_domain`) had already fixed this with an
   inline `example_interfaces` rclpy server; the zenoh variant now uses the
   same script.

Verified: both #153 lanes pass; full regression across rmw_interop,
services, actions, c_xrce_api, both bridges, mixed_qos, safety-integrity =
**48/48 green** after a fixture rebuild.

## Repro

```
cargo nextest run -p nros-tests --test rmw_interop -E 'test(ros2_server_nano_client)'
```
