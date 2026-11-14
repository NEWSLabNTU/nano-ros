---
id: 2569
title: "The C++ metadata probe assumes `<pkg>/<Class>.hpp`, and still logs an rclcpp-shape failure when `HEADER` is set"
status: open
type: bug
area: [cmake, cli]
severity: low
found: 2026-11-14
related: [phase-478]
---

GitHub: NEWSLabNTU/nano-ros#3861

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)**.
Seen on nano-ros 0.6.0-nros8.

## What happens

The C++ metadata probe assumes a node's header is `<pkg>/<Class>.hpp`. A
ported Autoware package with a different header path fails the probe. Setting
`HEADER` (on `nano_ros_node_register`) works around it, but the probe still
logs an rclcpp-shape failure.

## Direction

With `HEADER` set the probe uses it and logs nothing; without it, the error
names the `HEADER` option.
