---
id: 2035
title: "C++ cannot turn off one node's /parameter_events publisher: NodeOptions::start_parameter_event_publisher(false) is refused at compile time, and only C reaches the opt-out"
status: resolved
type: enhancement
area: [api]
severity: low
found: 2026-10-25
resolved: 2026-10-25
related: [2023, phase-488]
---

## What

Issue 2023 made C++ and Rust nodes publish `/parameter_events` whenever their
parameters are served, which is upstream's default. Upstream lets one node opt
out with `rclcpp::NodeOptions().start_parameter_event_publisher(false)`. Ours
refuses that call at compile time. `packages/api/nros-cpp/include/rclcpp/options.hpp`
has the setter and the getter as `template <typename T = void>` bodies whose
`static_assert(detail::refuse<T>::value, NROS_RCLCPP_REFUSE_NODE_OPTIONS)` fires
on use.

The only per-node off switch is C's `notify_changed_over_dds = false`, which
`Executor::disable_parameter_events_on` backs. A C++ node that ports this
option has to drop the line and always publish.

It is written down in archived issue 2023's "Still open" section and in the
ledger row `cpp:NodeOptions::start_parameter_event_publisher` (refuse-loud).
No open issue tracked it before this one.

## Why it is refused today

`NodeOptions` carries no runtime state (the `cpp:NodeOptions` row). The
refusal is per concept, shared with the rest of the option family. This one
member has a runtime backing already (`disable_parameter_events_on`), so it
could be honoured without giving `NodeOptions` general state. One way is a
single flag that `Node` creation reads and forwards.

## Done when

`NodeOptions().start_parameter_event_publisher(false)` compiles on the C++
surface and the node it creates publishes no `/parameter_events`, held by a
runtime test on the stub backend. Alternatively, the refusal is re-ruled as
permanent with a reason that meets RFC-0089's bar.

## Resolution

Resolved 2026-10-25. `NodeOptions::start_parameter_event_publisher(bool)` and
its getter are now a real field (default `true`, as upstream). The
`Node(name[, ns], options)` constructors read it, and so do
`make_shared<Node>(name, options)` and `LifecycleNode`. When it is `false`, the
constructor calls the new `nros_cpp_node_disable_parameter_events`, the C++
binding of `Executor::disable_parameter_events_on` (the switch C's
`notify_changed_over_dds = false` reaches). The opt-out sticks: serving the
node's parameters later publishes nothing for it. The field is unconditional,
so `sizeof(NodeOptions)` stays probe-independent.

Held by `packages/api/nros-cpp/tests/compile/node_options_param_events_runtime.cpp`
(param-services build, stub backend, in `just check cpp`). It runs two nodes on
one executor. The opted-out node's set publishes nothing, and the default
node's set is published under its own name. Negative control, measured:
removing the opt-out call in `Node::apply_options_` fails three checks.
`ros2_refuse_node_options_probe.cpp` no longer lists the option.

## Still open

A node built without options (`Executor::create_node`, the generated entry's
`NodeHandle` constructor) has nowhere to state the option. On a build without
parameter services no node publishes the topic, so there the option changes
nothing.
