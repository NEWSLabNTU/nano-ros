---
id: 2024
title: "No surface offers rclcpp::ParameterEventHandler: a node cannot react to ANOTHER node's parameter changes"
status: open
type: enhancement
severity: low
area: [api]
found: 2026-10-24
related: [2023, phase-379, phase-515, phase-488]
---

**Home: [phase-515](../roadmap/phase-515-api-stock-shape-review.md)** — homed 2026-10-24; listed in that phase's table of open API issues.

## Problem

Every surface publishes `/parameter_events` since issue 2023. None offers the
subscribing half: rclcpp's `ParameterEventHandler` (`add_parameter_callback`,
`add_parameter_event_callback`, the owning `ParameterCallbackHandle` /
`ParameterEventCallbackHandle`) and `AsyncParametersClient::on_parameter_event`.
A user who wants to react to another node's parameter change subscribes to
`rcl_interfaces/msg/ParameterEvent` on `/parameter_events` by hand and filters
by `node`.

The ledger declined this on an allocator argument (`std::function` registry,
`shared_ptr` handle per registration). That holds for a freestanding build only.
A hosted C++ build has both, and a freestanding one could use a fixed-size
callback table. So the rows are now `gap`, not `declined`
(`cpp:ParameterEventHandler`, `cpp:ParameterEventCallbackHandle`).

## Shape of a fix

- A typed `rcl_interfaces/msg/ParameterEvent` subscription per handler, created
  on the node, with a bounded callback table (`NROS_PARAM_EVENT_CALLBACKS`).
- The hosted build takes `std::function` and hands back a `shared_ptr` handle
  whose destructor unregisters, as upstream does. The freestanding build takes a
  function pointer plus context and returns a slot id.
- Costs one subscriber slot and the receive buffer for one event, sized like the
  reply buffer the publisher encodes into.
