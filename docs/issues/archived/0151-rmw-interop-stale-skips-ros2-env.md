---
id: 151
title: "rmw_interop: stale 'nros talker doesn't support RELIABLE yet' skip-strings (wrong since #146) + ROS 2 action/latency lanes need env"
status: resolved
type: tech-debt
area: testing
related: [issue-0146, issue-0153]
resolved_in: 3e280622e
---

## Summary

Two distinct problems in `tests/rmw_interop.rs`:

1. **Stale capability skips.** The QoS matrix cases skip with
   `[SKIPPED] QoS reliable→best_effort/reliable: nros talker doesn't support
   RELIABLE yet` — factually wrong since #146 established that nros
   pubs/subs default RELIABLE and advertise it in the liveliness-token QoS.
   The skip predates that; the guarded cases should now RUN (and the fixture
   talker can request reliable via `--qos`/env if it doesn't already).
   Un-gate them and delete the stale skip strings.

2. **ROS 2 env lanes.** `test_action_ros2_server_nano_client` (needs a ROS 2
   action server → sourced ROS 2 + demo pkgs) and `test_latency_nano_to_ros2`
   panic on this machine — environment, not product. They should
   `skip!`-cleanly when `ros2` / the demo packages are absent rather than
   panic mid-test; verify their preconditions gate at the TOP.

## Repro

```
cargo nextest run -p nros-tests --test rmw_interop --no-capture
```

## Resolution (2026-07-08)

Verified on a machine with ROS 2 humble + `rmw_zenoh_cpp` + `action_tutorials_py`
+ `example_interfaces` present.

1. **Stale RELIABLE skips — FIXED + VERIFIED.** `test_qos_matrix` no longer
   assumes the publisher's reliability: it drives the `qos-override-pubsub`
   fixture (`NROS_QOS_ROLE=talker`, `NROS_QOS_OVERRIDE=reliability=best_effort`
   for the BE cells, default Reliable otherwise) against a matching-QoS ROS 2
   Int32 subscriber. The stale `"nros talker doesn't support RELIABLE yet"`
   `skip!` is gone. All four cells RUN green: BE→BE, R→R, R→BE deliver; BE→R
   over-delivers (logged INFO — zenoh reliability is looser than DDS RxO).

2. **Latency lane — FIXED + VERIFIED.** `test_latency_nano_to_ros2` had a 5 s /
   100×10 ms hand-rolled poll that (a) was shorter than rmw_zenoh's ~10 s
   zenoh-pico discovery (#146) and (b) could miss the `data:` line straddling two
   10 ms reads. Replaced with a single 25 s `wait_for_output` (the same robust
   pattern `test_nano_to_ros2` uses). Now green — first-message latency ≈ 10.7 s
   (dominated by discovery, as expected).

3. **Action-server → nano-client gate — ADDED.** `test_action_ros2_server_nano_
   client` now gates on `ros2_pkg_available("action_tutorials_py")` at the top, so
   an absent demo pkg is a clean `skip!` instead of a downstream "delivered
   nothing" panic. NOTE: on THIS dev machine the lane still fails with
   `Transport(Timeout)` on goal acceptance — but that is the ROS-2-server →
   nano-client delivery timeout tracked in **#150** (the untouched
   `test_service_ros2_server_nano_client` fails identically; `nano→ros2` lanes
   all pass), NOT this issue. #151's precondition-gating part is done; the
   residual green for that lane depends on #150.

Full `rmw_interop`: 23/26 pass, 1 skip; the 3 fails are all the
ros2-server→nano-client direction (#150).
