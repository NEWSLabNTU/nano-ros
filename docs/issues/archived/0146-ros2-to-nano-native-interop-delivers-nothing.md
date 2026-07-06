---
id: 146
title: "ROS 2 → nano native interop delivers nothing over rmw_zenoh — ros2 topic pub → native nros listener receives 0 (nano → ROS 2 works)"
status: resolved
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

## Resolution (2026-07-06) — test defect, not a product bug

Not a delivery failure: a stock ROS 2 (rmw_zenoh) publisher DOES reach a native
nros subscriber. The suite defect was a QoS mismatch the harness itself
introduced, compounded by too-tight timing.

Root cause (dominant): `Ros2Process::topic_pub` (ros2.rs) forced
`--qos-reliability best_effort`, but the native listener under test declares a
DEFAULT (reliable) subscription. A best_effort publisher is INCOMPATIBLE with a
reliable subscriber by ROS 2 QoS rules, so rmw_zenoh delivered nothing —
independent of direction plumbing. Proven at the shell: a reliable
`ros2 topic pub /chatter std_msgs/msg/String` delivers to the listener (recv=5),
the same command with `--qos-reliability best_effort` delivers 0. (The reverse
nano→ROS 2 direction always worked because it's reliable nano pub → `ros2 topic
echo`'s best_effort sensor_data sub — the compatible pairing.)

Two timing factors were also below the discovery cost and would have flaked even
with matched QoS: rmw_zenoh's publisher-side discovery of a zenoh-pico
subscriber measured ~10 s (pub-start → first sample), while `topic_pub` ran the
publisher for only `timeout 10` and the tests waited `from_secs(8)` for the
first receive.

A red herring surfaced first: the on-disk `native/rust/listener` fixture was a
STALE pre-W4 build declaring `Int32_` while the test publishes `String_` — a
type-segment keyexpr mismatch that also yields 0. Rebuilding the fixture cleared
that; the QoS/timeout defects remained underneath. (The absence of staleness
detection for plain-example fixtures — `require_prebuilt_binary` is a bare
existence check, no inputsig — is the recurring hazard behind this class;
tracked separately.)

## Fix (test-only)

- `topic_pub`: drop the `--qos-reliability best_effort` override → publish with
  the default reliable profile (matches the reliable subscriber); bump the
  publisher lifetime `timeout 10` → `timeout 45` to outlive ~10 s discovery.
- `rmw_interop.rs`: the two ROS 2→nros receive windows `from_secs(8)` →
  `from_secs(25)`.

`test_ros2_to_nano` PASSES (16.6 s); `test_communication_matrix::case_3`
(Ros2ToNano) and `test_qos_matrix::case_3` ride the same helper and clear too.
