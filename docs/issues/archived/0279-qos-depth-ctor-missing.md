---
id: 279
title: "nros::QoS lacked the rclcpp depth constructor — QoS(5) spelling failed to compile"
status: resolved
type: friction
severity: low
area: nros-cpp
---

## Finding (autoware-safety-island-example ports, 2026-07-24 — porting-notes 10)

Upstream spells `rclcpp::QoS(5)` / `rclcpp::QoS{1}.transient_local()`;
native `nros::QoS` had only the default ctor.

## Resolution (same-day, 2026-07-24)

`explicit QoS(int depth)` added; ported code keeps the upstream spelling.
`transient_local()` chaining already existed (latched VelocityLimit
verified against a late-joining `ros2 topic echo`). Filed retroactively for
the record trail.
