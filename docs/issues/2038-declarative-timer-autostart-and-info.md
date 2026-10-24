---
id: 2038
title: "The declarative nros::Node timer has no autostart flag and no call info; only the imperative node and rclcpp layers got issue 1974's surface"
status: open
type: enhancement
area: [api]
severity: low
found: 2026-10-25
related: [1974, phase-488]
---

## What

Issue 1974 added Jazzy's timer call info (`expected_call_time` /
`actual_call_time`) and the `autostart` flag to three surfaces:

- the executor (`Executor::register_timer_with_info`);
- the imperative Rust node (`Node::create_timer_with_info(period, autostart, cb)`,
  `packages/core/nros-node/src/executor/node.rs`);
- C++ (`create_wall_timer(period, cb, group, autostart)`, `rclcpp::create_timer`,
  `TimerInfo`).

The DECLARATIVE surface a component uses has neither. That is the
`nros::DeclarativeNode` extension trait on `nros::Node`
(`create_timer_for_callback_name`, `..._on_clock`, `packages/api/nros/src/node.rs`).
A component timer always starts armed, and its callback is not told when it
was due or how late it ran.

Ledger: `rust:DeclarativeNode::create_timer_for_callback_name` and its `_on_clock`
sibling (divergence: entities are declared) do not mention either facility;
`rust:Node::create_timer_with_info` covers the imperative node only.

## Done when

- A declared timer can be declared stopped (an autostart flag in its
  declaration, carried into the model).
- A declared timer's callback can receive the call info.
- A unit test covers each.
- The two declarative rows name the result.
