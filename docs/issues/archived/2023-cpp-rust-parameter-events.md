---
id: 2023
title: "C++ and Rust nodes publish no /parameter_events; only rclc's C server did, and the publisher was in no sizing"
status: resolved
type: enhancement
severity: medium
area: [api, sizing]
found: 2026-10-24
resolved: 2026-10-24
related: [2024, 1587, 1270, phase-491, rfc-0105]
---

## What

rclcpp and rclrs nodes publish `rcl_interfaces/msg/ParameterEvent` on
`/parameter_events` (QoS `rmw_qos_profile_parameter_events`) on start-up and on
every declare, set and undeclare, with `node` set to the node's fully qualified
name. rqt_reconfigure, `rclcpp::ParameterEventHandler` and
`AsyncParametersClient::on_parameter_event` read it.

nano-ros published it from the C surface only (rclc's `notify_changed_over_dds`,
phase-491 W3). The emitter was already in the core (`Executor::enable_parameter_events_on`,
`flush_parameter_events`), but nothing on the C++ or Rust road called it. So a
C++ `Node::start_parameter_services()` or a Rust `[param_services]` image served
`ros2 param` and announced no change.

Two smaller defects came with it:

- **The publisher was in no sizing.** `/parameter_events` costs a publisher slot
  and a liveliness token, and neither the entity inventory nor the zenoh
  liveliness default counted it, even for the C images that already created it.
  The lifecycle family's `~/transition_event` had been counted since issue 1587;
  this one was missed.
- **A declaration made before events went on was never reported.** rclcpp's
  publisher exists before the first declaration. Ours comes up when the services
  do, which on the generated-entry road is after the nodes have declared, so
  their parameters were invisible to a subscriber until they next changed.

## Resolution

One core path for all three languages, in `nros-node`:

- `Executor::register_parameter_services` (the C++ shim's
  `nros_cpp_register_parameter_services`, Rust's `apply_param_services` and
  `Executor` users) now enables events on every node its reconcile serves
  (`start_parameter_events_for_served_node`). One VOLATILE publisher per
  executor, created at ONE site, `create_param_event_pub`, which also handles an
  executor with no registered node (the direct-API images) under the
  executor's own identity.
- rclc's `notify_changed_over_dds = false` is now an explicit per-node opt-out,
  `Executor::disable_parameter_events_on`. `true` still calls
  `enable_parameter_events_on`. No C-side copy exists.
- **Start-up event:** enabling events on a node queues one `New` entry per
  parameter it already holds, drained by `ParameterServer::pop_initial_parameter_event`
  before the change log, through a cursor into the fixed table (bounded, no
  heap). A node holding none publishes one event with empty lists
  (`ParameterEventKind::Announce`). Changes made while the start-up event is
  pending fold into it instead of doubling up. Under `sim-time` the start-up
  event names `use_sim_time`, as upstream's does.
- **Too big:** an event that does not fit `NROS_PARAM_SERVICE_BUFFER_SIZE` is
  refused whole, never truncated, and counted into the existing lost-events
  line, now logged as an ERROR.
- **Gate:** the same `param-services` cargo feature / `param_services`
  capability that compiles the six services. An image without it compiles none
  of this and creates no publisher.
- **Sizing:** `PARAM_EVENT_PUBLISHERS = 1` (`nros-node::parameter_services`),
  mirrored into the entity inventory's `InfraServices::publishers` (so
  `max_publishers` and `max_liveliness` grow by one when `param_services` is
  declared) and into `nros-zpico-build`'s liveliness default.
  `check-infra-queryable-counts` holds the constant to the `create_param_event_pub`
  creation site and every mirror to the constant. No queryable: the writer is
  VOLATILE.

No ABI change: `nros_cpp_register_parameter_services` keeps its signature and
gains the behaviour.

Tests: `nros-params` (start-up event content, folding, re-enable),
`nros-node` (`parameter_events_publish_each_change_in_its_slot` updated for the
start-up event, `registering_parameter_services_publishes_parameter_events_for_every_node`
for the C++/Rust road incl. the opt-out, `a_node_holding_no_parameter_announces_itself`),
the entity inventory and zpico-build sizing tests, a C++ runtime test on the stub
backend (`parameter_events_runtime.cpp`, `just check cpp`), an opt-out check in
`rclc_parameter_server.c` (`just check c`), and a live cell,
`ros2_topic_echo_sees_each_nodes_parameter_events` in
`params_per_node_interop.rs` (cell `native-params-per-node-rust-zenoh`).

## Still open

- Subscribing to another node's events (`rclcpp::ParameterEventHandler`,
  `AsyncParametersClient::on_parameter_event`): issue 2024.
- A per-node off switch from C++ (`NodeOptions::start_parameter_event_publisher(false)`)
  stays refused at compile time; only C reaches the opt-out.
- No live C++ cell against stock ROS 2: no C++ fixture in `interop::CELLS` serves
  parameters to `ros2 param` today, and the C++ path is the same core call the
  Rust cell exercises.
