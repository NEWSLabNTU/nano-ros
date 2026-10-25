---
id: 2056
title: "The hosted C++ surface has no rclcpp::Parameter, rclcpp::ParameterValue or vector-shaped on-set-parameters callback"
status: resolved
type: limitation
area: [api, cpp, params]
severity: medium
found: 2026-10-25
resolved: 2026-10-25
related: [phase-488, 2024, 2035, 2057]
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

## Resolution

Built as sketched, on the hosted (`NROS_CPP_STD`) surface, forwarding to the
executor's one store.

- `rclcpp::ParameterValue` and `rclcpp::Parameter`
  (`packages/api/nros-cpp/include/rclcpp/parameter_value.hpp`). They exist in
  every arm with one layout: a type tag, the three scalar slots, and one
  pointer to a heap block only a hosted build allocates.
  `check-cpp-capability-layout` measures them in all three arms. Freestanding
  keeps the scalar constructors and accessors. A string literal is refused
  there rather than silently becoming a `bool`.
- `Node::declare_parameter(name, ParameterValue[, descriptor])` dispatches on
  the tag to the typed `declare_parameter<T>`. The descriptor may be
  nano-ros's or `rcl_interfaces::msg::ParameterDescriptor`, which is a
  template over the generated shape, as are its text, `read_only` and the
  ranges. The typed `declare_parameter<T>(name, v, descriptor message)` form
  is included too.
- `get_parameter(name)`, `get_parameter(name, Parameter&)`,
  `get_parameters`, `set_parameter(const Parameter&)` and `set_parameters`.
  The setters return `successful` and `reason`, and are not `[[nodiscard]]`.
- `add_on_set_parameters_callback(f)` over `std::vector<rclcpp::Parameter>`.
  It returns `node_interfaces::OnSetParametersCallbackHandle::SharedPtr`,
  whose destruction unregisters, and
  `remove_on_set_parameters_callback(handle.get())` unregisters it early.
- `get_node_parameters_interface()`, a view in issue 2015's node-interface
  handle shape.
- The FFI half: `nros_cpp_param_write_t` gained `byte_array`, `bool_array`,
  `integer_array`, `double_array` and `array_len`. The hook therefore sees a
  proposed array's ELEMENTS. Before this, an array arrived as its type with
  every field zeroed, so Nav2's `as_double_array().size() != 3` check would
  have refused every write. `set_parameters_atomically` now accepts those
  arrays, and `ParameterValue::from_byte_array` joins the other three
  constructors.
- When the user's build generated `rcl_interfaces`, the header includes the
  `rcl_interfaces` messages upstream's `rclcpp.hpp` reaches transitively
  (`ParameterType`, `ParameterValue`, `Parameter`, `ParameterDescriptor`,
  `SetParametersResult`, `ListParametersResult`). Nav2 names them without
  including them.
- Found on the way: `declare_parameter<std::vector<bool>>` did not compile,
  because `std::vector<bool>` has no `data()`. It now crosses through a
  contiguous copy.

**Bounds.**

- The callback still sees one write per call, and its `reason` is dropped,
  because the store's hook answers `bool`.
- A byte or string array can be HELD but not declared, since the store FFI
  carries neither.
- A wrong-type `as_*` answers the zero value, not an exception (RFC-0018).
- `get_parameter_overrides()` is absent. That is issue 2057.
- `rclcpp::exceptions` stays refused by design.

**Test.** `packages/api/nros-cpp/tests/compile/parameter_value_runtime.cpp`,
in `just check cpp`, on the `param-services` archive:

- it declares through `ParameterValue`, vectors and a descriptor message
  included;
- it registers a `std::bind` callback;
- it votes on a local write and on a `~/set_parameters` request staged on the
  stub backend, and asserts both that the refusal is the remote caller's
  answer and that the store is unchanged.

Negative controls, measured:

- the trampoline's `double_array` left NULL gives 6 failures;
- a no-op handle destructor makes the next write call a freed handle (SIGBUS).

**Done-when, measured on the Nav2 port** (phase-488 node 2):

- every parameter-API error is gone from `nav2_velocity_smoother` and
  `nav2_util`;
- `velocity_smoother.cpp`'s 13 `ParameterValue` declarations and its
  `dynamicParametersCallback` compile unchanged;
- `node_utils.hpp` keeps two stated edits: `rclcpp::exceptions` becomes
  `std::invalid_argument`, and the `get_parameter_overrides()` test becomes
  issue 2057's edit.

The port's remaining errors are not parameter API. They are node 2's next
findings.
