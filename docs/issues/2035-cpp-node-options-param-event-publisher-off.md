---
id: 2035
title: "C++ cannot turn off one node's /parameter_events publisher: NodeOptions::start_parameter_event_publisher(false) is refused at compile time, and only C reaches the opt-out"
status: open
type: enhancement
area: [api]
severity: low
found: 2026-10-25
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
