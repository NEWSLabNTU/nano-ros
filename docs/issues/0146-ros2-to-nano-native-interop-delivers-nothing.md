---
id: 146
title: "ROS 2 → nano native interop delivers nothing over rmw_zenoh — ros2 topic pub → native nros listener receives 0 (nano → ROS 2 works)"
status: open
type: bug
area: rmw-zenoh
related: [133]
---

## Summary

Surfaced by the #133 fail-loud hardening (2026-07-06): the **ROS 2 → nros**
direction of the native rmw_zenoh interop suite delivers nothing. A stock
`ros2 topic pub /chatter std_msgs/msg/String …` reaches the native nros
listener as **0 samples**, while the reverse direction (**nano → ROS 2**, a
native nros talker → `ros2 topic echo`) works. The asymmetry is consistent, not
a timing flake.

Two independent tests in `packages/testing/nros-tests/tests/rmw_interop.rs`
reproduce it identically (~8.6 s each, well past the discovery window):

- `test_ros2_to_nano` (`rmw_interop.rs:200`) — "ROS 2 → nros delivered nothing:
  the nano listener received 0 samples from the ROS 2 publisher over rmw_zenoh."
- `test_communication_matrix::case_3` (`Ros2ToNano`, `rmw_interop.rs:270`).

Both fire the `assert!` only *after* the ROS 2 publisher launches successfully
(a launch failure `skip!`s), so the router + ROS 2 CLI are up — the message
just never reaches the nros subscriber.

## Why it was invisible until now

The tests soft-passed on 0 received (`[INFO] … may be timing issue`, no
assert) — the #133 defect. Converting them to hard asserts (2026-07-06) exposed
this. Because nano → ROS 2 passes, the zenoh router + the ros2↔zenoh bridge and
keyexpr mapping work in at least one direction; the gap is specific to
ROS 2 publisher → nros subscriber.

## Suspected area

- The nros subscriber's liveliness / keyexpr may not be discovered by the ROS 2
  (rmw_zenoh) publisher, so the publisher never routes to it — mirror of the
  keyexpr/liveliness work in the resolved #141 (nros pub → rmw_zenoh_cpp sub, the
  opposite direction). Compare the declared sub keyexpr against what rmw_zenoh_cpp
  expects on the publish side.
- Confirm whether this reproduces in a cleanly provisioned ROS 2 + rmw_zenoh CI
  lane vs a local dev environment quirk before treating it as a hard product bug.

## Next steps

1. Capture the nros listener's declared subscription keyexpr and the ROS 2
   publisher's target keyexpr (`z_scout` / router admin space) and diff them.
2. Re-run `test_ros2_to_nano` where ROS 2 + rmw_zenoh are provisioned to confirm
   product-bug vs environment.
3. Fix the discovery/keyexpr mismatch; the two tests above become the gate.
