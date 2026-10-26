---
id: 2080
title: "C++ has no `ParameterEventHandler::get_parameter_from_event` / `get_parameters_from_event`"
status: open
type: limitation
area: [api]
severity: low
found: 2026-10-26
related: [issue-2024, phase-515, phase-488]
---

**Home: [phase-515](../roadmap/phase-515-api-stock-shape-review.md)** (item P6) — the residue issue 2024 left open when it shipped the hosted `rclcpp::ParameterEventHandler`.

## What is missing

rclcpp's `ParameterEventHandler` carries two static helpers that read a received
`rcl_interfaces::msg::ParameterEvent`:

```cpp
static bool get_parameter_from_event(const rcl_interfaces::msg::ParameterEvent & event,
                                     rclcpp::Parameter & parameter,
                                     const std::string & parameter_name,
                                     const std::string & node_name = "");
static rclcpp::Parameter get_parameter_from_event(const rcl_interfaces::msg::ParameterEvent & event,
                                                  const std::string & parameter_name,
                                                  const std::string & node_name = "");
static std::vector<rclcpp::Parameter> get_parameters_from_event(
    const rcl_interfaces::msg::ParameterEvent & event);
```

Our `rclcpp::ParameterEventHandler` (issue 2024, hosted C++ only) ships
`add_parameter_callback` / `add_parameter_event_callback` and none of these.
Ledger rows `cpp:ParameterEventHandler::get_parameter_from_event` and
`cpp:ParameterEventHandler::get_parameters_from_event` are `gap`.

## Why it was left

The helpers take upstream's generated `rcl_interfaces::msg::ParameterEvent`, and
the nano-ros C++ surface cannot name that type: there is no runtime typesupport,
and message types are generated per user package rather than shipped with the
library. Nothing is lost for the common case — `add_parameter_callback` already
hands the callback a decoded `rclcpp::Parameter` for the one parameter it
watches. What a user cannot do is take a whole event (from
`add_parameter_event_callback`, or from their own `/parameter_events`
subscription) and pull parameters out of it with upstream's helper.

## Possible shape

A helper TEMPLATED over the caller's generated event type, so the library never
names `rcl_interfaces` itself:

```cpp
template <typename ParameterEventT>
static bool get_parameter_from_event(const ParameterEventT & event, rclcpp::Parameter & out,
                                     const std::string & name, const std::string & node = "");
```

It reads `event.node` and walks `new_parameters` / `changed_parameters`, decoding
each `rcl_interfaces::msg::Parameter`-shaped element into `rclcpp::Parameter` with
the same conversion the handler's per-parameter path already uses. Hosted only,
like the handler; freestanding keeps refusing. An upstream call site then compiles
unchanged once the user's generated `rcl_interfaces` header is included.

## Acceptance

Both ledger rows leave `gap` (to `same`/`arity-only`, or a `divergence` naming the
template), with a runtime test that feeds a generated `ParameterEvent` through the
helper.
