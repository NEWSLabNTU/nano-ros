---
id: 146
title: "A BEST_EFFORT rmw_zenoh_cpp publisher does not reach an nros subscriber (RELIABLE works) — ros2→nano interop QoS gap"
status: open
type: bug
area: rmw-zenoh
related: [133, 141]
---

## Summary

Surfaced by the #133 fail-loud conversion; root-caused 2026-07-06. The
ros2→nano data path is NOT broadly dead — it fails **only when the ROS 2
publisher's reliability QoS is BEST_EFFORT**. A RELIABLE `rmw_zenoh_cpp`
publisher (which is what any default rclcpp node uses) reaches the nros
subscriber fine.

## Isolation (all against a shared `zenohd`, same nros typed listener)

| Publisher | QoS | → nros sub | Result |
| --- | --- | --- | --- |
| `demo_nodes_cpp talker` (real node) | RELIABLE (rclcpp default) | raw sub | PASS (`nros_subscriber_receives_stock_demo_nodes_cpp_talker`, 3.7 s) |
| `demo_nodes_cpp talker` | RELIABLE | **typed** example listener | PASS (scratch, 3.5 s) |
| `ros2 topic pub … --qos-reliability reliable` | RELIABLE | typed listener | PASS (2.7 s) |
| `ros2 topic pub … --qos-reliability best_effort` | BEST_EFFORT | typed listener | **FAIL — 0 received** (even at 2 Hz over a 25 s window) |

So:
- The nros **typed** subscription is fine (it receives from a real node).
- The gap is **QoS-reliability-specific**: BEST_EFFORT rmw_zenoh_cpp pub → nros
  (zenoh-pico) sub delivers nothing.
- `test_ros2_to_nano` / `test_communication_matrix::case_3` (rmw_interop.rs) fail
  because `Ros2Process::topic_pub` hardcodes `--qos-reliability best_effort` —
  they hit the real gap. (Once #133 stopped soft-passing 0-received, this became
  visible.)

## Why it matters / asymmetry

nano→ros2 BEST_EFFORT works (a best_effort nano publisher reaches a `ros2 topic
echo` — the qos_matrix delivery-expected cells pass). The gap is the OTHER
direction: full-zenoh best_effort publisher → zenoh-pico subscriber.

## Suspected area

The nros subscriber declares no explicit zenoh reliability (uses zenoh-pico
defaults). `rmw_zenoh_cpp`'s BEST_EFFORT publisher publishes with a different
zenoh mechanism than RELIABLE — likely `CongestionControl::Drop` and/or a
reliability/priority setting on the `z_put` that the zenoh-pico subscriber's
default declaration does not receive over the router. Compare against the
resolved #141 (opposite direction, keyexpr suspicion was a red herring — the
real issue there was environmental).

## Next steps

1. Capture `zenohd` at `RUST_LOG=zenoh=debug` for a `best_effort` vs `reliable`
   `ros2 topic pub` on `/chatter` + the nros sub; diff the declared publisher
   keyexpr / reliability and confirm whether the router forwards the best_effort
   samples to the zenoh-pico session at all (routing vs sub-side drop).
2. If it's a sub-side reliability declaration: have the nros zenoh-pico
   subscriber declare a reliability that receives best_effort samples (or match
   the requested QoS). If it's a zenoh↔zenoh-pico transport channel issue, file
   upstream / pin the zenoh-pico behaviour.
3. Regression gate: a `ros2 topic pub --qos-reliability best_effort` → nros sub
   test that must receive (currently the implicit failing axis).

## Reproduce

`cargo nextest run -p nros-tests -E "test(test_ros2_to_nano)"` (needs ROS 2 +
rmw_zenoh_cpp provisioned) — fails with 0 received; swapping the publisher to
`--qos-reliability reliable` makes it pass.
