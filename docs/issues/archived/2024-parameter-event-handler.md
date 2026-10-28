---
id: 2024
title: "No surface offers rclcpp::ParameterEventHandler: a node cannot react to ANOTHER node's parameter changes"
status: resolved
type: enhancement
severity: low
area: [api]
found: 2026-10-24
resolved: 2026-10-25
related: [2023, phase-379, phase-515, phase-488]
---

**Home: [phase-515](../../roadmap/archived/phase-515-api-stock-shape-review.md)** — homed 2026-10-24; listed in that phase's table of open API issues.

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

## Resolution

Resolved 2026-10-25 for hosted C++ (`packages/api/nros-cpp/include/rclcpp/parameter_event_handler.hpp`).

- `rclcpp::ParameterEventHandler(node[, qos])` creates ONE `/parameter_events`
  subscription (`rcl_interfaces/msg/ParameterEvent`) through
  `Node::create_subscription`. The entity inventory and receive-buffer sizing
  count it like any other subscription. `NROS_PARAM_EVENT_RX_BYTES` (default
  4096) bounds the frame, and a larger frame is dropped.
- `add_parameter_callback(name, cb, node_name = "")` hands `cb` an
  `rclcpp::Parameter` for each new or changed value of that parameter of that
  node. An empty node name means this node, and a relative name resolves
  against this node's namespace. Deletions are not reported.
- `add_parameter_event_callback(cb)` hands `cb` every event, decoded into the
  type `cb` takes (deduced): the image's generated
  `rcl_interfaces::msg::ParameterEvent`.
- Both return owning `shared_ptr` handles that the handler holds only weakly,
  so dropping a handle unregisters it. `remove_*` and destroying the handler
  also stop the callbacks.
- A hosted `rclcpp::Parameter` (name plus typed value, upstream's `as_*`
  readers) is new. Reading the wrong type returns the zero value, because
  there are no exceptions here.
- The frame is decoded in the header (little-endian CDR). A truncated frame is
  dropped, never misread.
- A freestanding build refuses the handler at compile time
  (`NROS_RCLCPP_REFUSE_PARAMETER_EVENT_HANDLER`). There is no fixed-table form:
  the per-parameter value needs owned storage. Freestanding code subscribes to
  the topic itself.

Held by `parameter_event_handler_runtime.cpp` (stub backend, frames staged as
a ROS 2 peer writes them) and `parameter_event_handler_freestanding_probe.cpp`
in `just check cpp`. Negative controls, measured: removing the node-name match
fails the "not another node" check, and removing the event-callback call fails
both event checks.

Ledger: `cpp:ParameterEventHandler` and `cpp:ParameterEventCallbackHandle` are
now divergence / adopt-bounded, and `cpp:Parameter` moved from declined to
divergence / adopt-bounded. New rows cover the theirs-only members.

## Still open

- `ParameterEventHandler::get_parameter_from_event` /
  `get_parameters_from_event` (ledger rows `gap`). They read the generated
  event type's fields, which this header cannot name.
- `AsyncParametersClient::on_parameter_event` is not provided.
- No Rust or C handler. A plain `/parameter_events` subscription works there.
- The per-parameter handle is `ParameterEventHandler::ParameterCallbackHandle`,
  because `rclcpp::ParameterCallbackHandle` is the on-set token
  (`cpp:ParameterCallbackHandle`). A member declared
  `std::shared_ptr<rclcpp::ParameterCallbackHandle>` does not compile.
