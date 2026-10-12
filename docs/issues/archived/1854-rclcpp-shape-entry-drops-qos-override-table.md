---
id: 1854
title: "The C++ entry bakes no QoS override table for an `rclcpp`-shape component (or a `rust` one), so a contract or launch QoS that differs from the code's is silently not applied"
status: resolved
resolved_in: "issue 1854 PR (cpp node_body.jinja rclcpp arm, NodeHandle table, rust-arm refusal)"
type: bug
area: codegen, api
related: [0298, 0303, 0308, 1456, rfc-0044, rfc-0100]
---

## Problem

`qos_overrides.<topic>.<role>.<policy>` parameters reach a node from a launch
`<param>`, a params file, `system.toml`, or -- since play_launch phase 74 --
from the launch CONTRACT: the resolver derives a contract's `deadline`,
`liveliness` and `lease_duration` into those parameters on the node.
`plan_from_model` lowers them (`nros_orchestration_ir::qos_override`) onto
`PlanNode.qos_overrides`, and the entry is supposed to bake them.

The C++ pack's `node_body.jinja` rendered the table only in the
`configure`-shape arm (`set_qos_overrides` between `create_node` and
`configure`, issue 0298). The `rclcpp` arm -- a component that constructs its
own node and its entities in its constructor -- rendered nothing: no table,
no call, and `NodeHandle` had no field to carry one. Archived issue 0298 noted
it as a LIMITATION ("would need the override passed into the ctor"); no open
issue tracked it. The `rust` arm (a Rust component installed from a C++
entry through `__nros_component_<pkg>_install`) dropped the table too.

Measured downstream (Autoware Safety Island, phase-9 C5): all three island
nodes are `SHAPE rclcpp`. While the contract still requested
`liveliness: manual_by_topic` + a 500 ms lease on mrm_handler's
`/system/operation_mode/availability` subscription, the resolved model carried
the two derived `qos_overrides.*` parameters on `/mrm_handler`, and the
generated `zephyr_entry_nros_main_generated.cpp` had no
`nros_cpp_qos_override_t` table and no `set_qos_overrides` anywhere: the image
never requested the lease. The C pack (`c/node_body.jinja`) and the
`nros::main!` macro path were not affected.

## Fix

- `rclcpp::NodeHandle` (nros-cpp `node.hpp`) carries `qos_overrides` +
  `qos_override_count`, with a 5-argument constructor. `Node`'s handle
  constructor installs the table (`nros_cpp_node_set_qos_overrides`) right
  after the node opens -- before the derived constructor body creates any
  entity -- and latches `set_qos_overrides` as the node's error on failure.
- `cpp/node_body.jinja`'s `rclcpp` arm emits the same `static const
  ::nros_cpp_qos_override_t __nros_qos_<i>[]` table the `configure` arm does
  and passes it on the handle. Without overrides it still writes the
  3-argument handle, so every existing entry is byte-identical.
- A `rust` component carrying overrides in a C/C++ entry is REFUSED at
  `emit::admit_and_lower`, naming the node (its install seam has no table;
  issue 0303's rule: a QoS producer never drops silently).

## Tests

- `emit/tests_cpp.rs`: the rclcpp arm bakes the table and hands it to the
  handle (table before handle before construction); no overrides -> no
  table; a rust node with overrides is refused, without them it renders.
- `testdata/entry/cpp_native_shapes.cpp.golden`: `rcl_two` now carries a
  table (both directions of the arm in one golden).
- `tests/contract_qos_reaches_the_entry.rs`: one launch file, two contracts
  differing ONLY in a subscriber's `liveliness:` line, resolved by the pinned
  resolver; the entry differs by exactly the handler's baked table.
  Negative control: the old template -> the contract test fails.
- `nros-cpp/tests/compile/node_qos_overrides_runtime.cpp` (`just check cpp`):
  on the stub backend, the component's constructor body sees the handle's
  table on its node; the 3-argument handle leaves none. Negative control,
  measured: the install removed -> 2 failures.

## Not covered here

The duration policies' UNIT disagree between the producer (play_launch
writes nanoseconds, as rclcpp does) and the lowering (reads milliseconds):
issue 1855.
