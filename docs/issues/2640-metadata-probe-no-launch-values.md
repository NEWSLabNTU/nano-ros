---
id: 2640
title: "The C/C++ metadata probe cannot construct a node that declares parameters without defaults, or more of them than a default store holds"
status: open
type: bug
area: [cli, params, cmake]
severity: medium
found: 2026-11-16
related: [phase-478]
---

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md).**
Reported by the Autoware Safety Island (phase-15 P3) on nano-ros 5b9e8844a4.

## What happens

The metadata probe (`orchestration/metadata_probe_cmake.rs`, run from
`metadata_refresh.rs`) builds a one-node host plan for each C/C++ component
(`probe_plan`) with `params: Vec::new()` and `param_services: false`, and
links it against a store at the default knobs. An rclcpp-shaped Autoware
node therefore runs its constructor with no launch values and a store
smaller than its declarations:

- a `declare_parameter<T>(name)` with NO default reads `T()`: the island's
  controller divides by `ctrl_period = 0` and sizes a deque from the NaN;
- past the store's capacity every further declaration silently returns its
  compiled default (GitHub #4018).

So the node cannot be probed, and the island duplicates every declaration's
default in C++ to keep the probe alive.

## Direction

Undecided: the probe area has an owner, who is asked before anything
changes. The proposal on the table: the probe seeds the launch values of
the component's instance from the launch RECORD (not the SystemModel, which
reads the probe's sidecar), and its host store is sized so that no
declaration can find it full.
