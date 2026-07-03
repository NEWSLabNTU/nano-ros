---
id: 141
title: "nros publisher → rmw_zenoh_cpp subscriber delivers no data (`ros2 topic echo` sees nothing) while graph discovery and the service path interop fine"
status: open
type: bug
area: rmw-zenoh
related: [phase-276, issue-0133, issue-0135]
---

## Summary

The DATA plane from a nano-ros zenoh publisher to an `rmw_zenoh_cpp` subscriber
appears dead: during phase-276 W5, `ros2 topic echo --once <topic>
std_msgs/msg/Int32` (and String) against a Zephyr native_sim image received
**zero samples** over repeated 15 s windows from healthy 1 Hz publishers —
while on the SAME image + router:

- `ros2 topic list` shows the topics (graph/liveliness interop works),
- `ros2 lifecycle nodes` / `get` / `ros2 service list` work end-to-end
  (query/service interop works — the W3 e2e asserts through it),
- a **nano-ros native subscriber** (`int32-observer`, raw
  `create_subscription_raw`) receives the very same stream at full rate.

So the gap is specifically nros-pub → rmw_zenoh_cpp-sub sample delivery.

## Evidence (2026-07-03)

- `ws-qos-rust` Zephyr entry (post-#139 fix, publishing `/qos_chatter` +
  `/qos_ok` at 1 Hz, confirmed via gdb put-counting and via `int32-observer`
  receiving 13 samples in ~18 s): `ros2 topic echo --once` on either topic
  times out on every attempt; `!rclpy.ok()` after the 15 s `timeout` kill.
- The original W5 e2e used `ros2 topic echo` and never passed; rewritten to
  the `int32-observer` assertion it passes in 7 s. Same for W3's discovery —
  services answer, but no topic-echo assertion exists anywhere green.
- Grep shows NO currently-green test covers nros-pub → ros2-sub: the
  `ros2-string-interop` fixture tests the OPPOSITE direction
  (`demo_nodes_cpp talker` → nros sub), and the #133 soft-pass tests may have
  been hiding this axis (zero received reported as pass).

## Suspects

`rmw_zenoh_cpp`'s subscriber may filter on the keyexpr's type-hash segment
(nros publishes with `TypeHashNotSupported` in some raw paths) or on
attachment metadata (sequence/source info) it expects on each sample.
Compare a wireshark/zenohd-log capture of a `demo_nodes_cpp` publisher vs an
nros publisher keyexpr + attachment. Untested: whether a NATIVE nros
publisher shows the same gap (suspected yes — the shim is shared).

## Impact

- No ros2-CLI-visible data from embedded nodes (debugging UX: `ros2 topic
  echo` silently empty against nros publishers).
- Interop coverage hole: the pub direction of the ROS 2 interop matrix is
  unproven; #133's soft-pass cleanup will surface it as failures.
