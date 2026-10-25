---
id: 2056
title: "The hosted C++ surface has no rclcpp::Parameter, rclcpp::ParameterValue or vector-shaped on-set-parameters callback"
status: open
type: limitation
area: [api, cpp, params]
severity: medium
found: 2026-10-25
related: [phase-488, 2024, 2035]
---

## What

nros-cpp's parameter API was built for freestanding code:

- typed `declare_parameter<T>` / `get_parameter<T>` / `set_parameter<T>`,
  with `std::vector<T>` overloads on the std surface;
- a `rclcpp::ParameterDescriptor` that borrows its text;
- an on-set-parameters callback that is a C function pointer plus context. It
  sees ONE write at a time as scalars and answers with a `bool`
  (`node_parameters.hpp`, "Neither type exists here").

Upstream's shapes, which ported code writes, are absent on the hosted surface
too:

- `rclcpp::ParameterValue`: a value of any parameter type, including the
  arrays (`rclcpp::ParameterValue(std::vector<double>{0.5, 0.0, 2.5})`);
- `rclcpp::Parameter`: `get_name()`, `get_type()`, `as_double()`,
  `as_bool()`, `as_string()`, `as_double_array()`, …;
- `declare_parameter(name, const ParameterValue&,
  const rcl_interfaces::msg::ParameterDescriptor&)`. Since phase-488,
  `rcl_interfaces` routes to codegen, so the descriptor message exists;
- `add_on_set_parameters_callback(f)`, where `f` is
  `rcl_interfaces::msg::SetParametersResult(const std::vector<rclcpp::Parameter>&)`.
  It returns an `OnSetParametersCallbackHandle::SharedPtr` whose destruction
  unregisters it;
- `node_interfaces::NodeParametersInterface` and
  `get_node_parameters_interface()`. Nav2's
  `nav2_util::declare_parameter_if_not_declared` takes one;
- `rclcpp::exceptions::ParameterAlreadyDeclaredException` and its siblings.
  These are refused by design (RFC-0018, no exceptions), so they stay a
  stated edit.

## Who uses them

Measured 2026-10-25 over the 2944 C++ sources (tests excluded) of an Autoware
0.45.1 workspace:

| pattern | files |
| --- | --- |
| `std::vector<rclcpp::Parameter>` | 183 |
| `rcl_interfaces::msg::SetParametersResult` | 121 |
| `add_on_set_parameters_callback` | 58 |
| `rcl_interfaces::msg::ParameterDescriptor` | 18 |
| `rclcpp::exceptions::` | 21 |
| `get_node_parameters_interface` | 9 |

Phase-488 node 2, Nav2 `nav2_velocity_smoother` @jazzy, stops here.
`velocity_smoother.cpp` declares 13 parameters through
`rclcpp::ParameterValue`, several of them `std::vector<double>`. Its
`dynamicParametersCallback(std::vector<rclcpp::Parameter>)` reads
`get_type()`, `as_double()` and `as_double_array()` and returns a
`SetParametersResult`. `nav2_util`'s `node_utils.hpp` and
`lifecycle_node.hpp` build `rcl_interfaces::msg::ParameterDescriptor`s and
call through `NodeParametersInterface`. Autoware's node 1
(`autoware_external_cmd_selector`) used none of this, which is why it did
not surface sooner.

## Sketch

A hosted layer over what exists, adding nothing to the freestanding surface:

- `ParameterValue` holds a type tag and one of the parameter types. The
  std surface already has `std::string` and `std::vector`, so it can own
  the value.
- `declare_parameter(name, ParameterValue[, descriptor])` dispatches on the
  tag to the typed `declare_parameter<T>`. The `rcl_interfaces` descriptor
  overload is a template over the generated message shape (the way
  `Time::to_msg` binds to `builtin_interfaces/msg/Time`). It lowers to
  `rclcpp::ParameterDescriptor`, borrowing the message's strings for the
  call.
- `Parameter` is a name plus a `ParameterValue`, with upstream's accessors.
- `add_on_set_parameters_callback(F)` stores `F` on the heap (hosted). It
  registers one C trampoline that wraps each write in a one-element
  `std::vector<Parameter>`, calls `F`, and answers with `result.successful`.
  It hands back a handle whose destruction unregisters. The callback still
  sees one write at a time, because the store validates writes one at a
  time; that is the ADOPT-BOUNDED note.
- `NodeParametersInterface` is a thin view over the node, for the
  `declare_parameter` / `has_parameter` / `get_parameter` calls helper
  libraries make through it.

## Done when

The parameter half of `nav2_velocity_smoother` and `nav2_util` compiles
unchanged on a hosted build. That means `ParameterValue` (vectors
included), `Parameter`, the `rcl_interfaces` descriptor overload, the vector
callback with `SetParametersResult`, and `get_node_parameters_interface()`.
A runtime test on the stub backend:

- declares through `ParameterValue`;
- registers a vector callback;
- sends a `set_parameters` request;
- asserts that the callback saw the write and that its refusal was honoured.
