---
id: 141
title: "nros publisher → rmw_zenoh_cpp subscriber delivers no data (`ros2 topic echo` sees nothing) while graph discovery and the service path interop fine"
status: resolved
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

## Resolution (2026-07-04) — not reproducible; direction pinned with coverage

Re-investigated with the router at `RUST_LOG=zenoh=debug`: `ros2 topic echo`
declares its subscriber on the EXACT keyexpr nros publishes
(`0/qos_ok/std_msgs::msg::dds_::Int32_/TypeHashNotSupported` — no wildcard,
no real hash), so the suspected type-hash mismatch does not exist on this
humble rmw_zenoh_cpp build. A debug-logging `rclpy` subscriber received the
Zephyr image's `/qos_ok` samples immediately (GOT 2/3/4), and `ros2 topic
echo` now delivers with BOTH the default sensor_data profile and explicit
`--qos-reliability reliable` — against the very same (pre-#143) image build
the failures were observed on. The original zero-delivery observations were
environmental (the #139 tx-starvation era plus accumulated stale router/
session state during that debugging session), not a product defect.

The REAL residual gap — no green coverage of the nros-pub → ros2-sub
direction — is closed by the new `qos_zephyr_ros2_interop_e2e`
(`nros_zephyr_publisher_reaches_ros2_topic_echo`, 5.2 s): boots the W5 qos
image and asserts `ros2 topic echo --once /qos_ok` sees a sample; serialized
with the sink-based qos e2e via the `zephyr-qos-port` nextest group (shared
baked router port). A regression on this axis can no longer hide behind
#133-style soft-passes.
