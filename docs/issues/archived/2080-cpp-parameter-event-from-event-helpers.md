---
id: 2080
title: "C++ has no `ParameterEventHandler::get_parameter_from_event` / `get_parameters_from_event`"
status: resolved
type: limitation
area: [api]
severity: low
found: 2026-10-26
resolved: 2026-10-26
related: [issue-2024, phase-515, phase-488]
---

**Home: [phase-515](../../roadmap/archived/phase-515-api-stock-shape-review.md)** (item P6) — the residue issue 2024 left open when it shipped the hosted `rclcpp::ParameterEventHandler`.

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

## Resolution (2026-10-26)

Landed as the "possible shape" above. `rclcpp::ParameterEventHandler` gains
three static TEMPLATES over the event type, deduced from the argument:

```cpp
template <typename ParameterEventT>
static bool get_parameter_from_event(const ParameterEventT& event, rclcpp::Parameter& parameter,
                                     const std::string& parameter_name,
                                     const std::string& node_name = "");
template <typename ParameterEventT>
static rclcpp::Parameter get_parameter_from_event(const ParameterEventT& event,
                                                  const std::string& parameter_name,
                                                  const std::string& node_name = "");
template <typename ParameterEventT>
static std::vector<rclcpp::Parameter> get_parameters_from_event(const ParameterEventT& event);
```

Upstream's call sites compile unchanged once the caller's generated
`rcl_interfaces` header is included. The element conversion
(`detail::parameter_from_msg`, upstream's `Parameter::from_parameter_msg`) reads
`name` and `value.{type, *_value, *_array_value}`, and the string fields go
through `c_str()` / `length()`, so the inline (`FixedString` / `FixedSequence`)
and heap (`HeapString`) codegen shapes both work, and so does `std::string`.

Same semantics as upstream: `event.node` is compared verbatim to `node_name`,
then new parameters, then changed ones; deletions are not reported. One
difference, because nothing here throws: on a node-name mismatch upstream's
value overload throws `std::runtime_error`, and ours returns
`Parameter(name)` (`PARAMETER_NOT_SET`), the same as a missing name. The `bool`
overload tells the two apart. Hosted only, like the handler.

Held by `parameter_event_from_event_runtime.cpp` in `just check cpp`: a
codegen-shaped `rcl_interfaces::msg::ParameterEvent` (inline fields) plus a
heap-string variant, through both overloads and `get_parameters_from_event`.
Negative control, measured: removing the node-name test makes 4 checks fail.

Ledger: both rows `gap` → `divergence` / `adopt-bounded`.

Not done: `cpp:Parameter::from_parameter_msg` stays `declined`. The template
helper (`detail::parameter_from_msg`) would serve it as a static member, but
that is a separate row and a separate decision.
