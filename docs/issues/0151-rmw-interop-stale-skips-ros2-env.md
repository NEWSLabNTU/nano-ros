---
id: 151
title: "rmw_interop: stale 'nros talker doesn't support RELIABLE yet' skip-strings (wrong since #146) + ROS 2 action/latency lanes need env"
status: open
type: tech-debt
area: testing
related: [issue-0146]
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
