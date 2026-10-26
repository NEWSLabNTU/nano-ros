---
id: 2057
title: "C++ has no get_parameter_overrides(): the executor applies -p / --params-file overrides but cannot list them"
status: resolved
type: limitation
area: [api, cpp, params]
severity: low
found: 2026-10-25
resolved: 2026-10-26
related: [2056, phase-488, phase-482, phase-518]
---

## What

Upstream's `NodeParametersInterface::get_parameter_overrides()` returns a
`std::map<std::string, rclcpp::ParameterValue>` of the overrides given to the
node (`--ros-args -p`, `--params-file`). Code uses it to ask whether a
parameter's value came from an override or from the code default.

nano-ros applies the same overrides (phase-482 W5,
`Executor::install_argv_params`). They are applied when a parameter is
DECLARED, typed by the declaration. Nothing lists them, so issue 2056's
`NodeParametersInterface` has no `get_parameter_overrides()`. That is the one
stated gap in its envelope.

## Who uses it

Nav2's `nav2_util::declare_or_get_parameter`
(`nav2_util/node_utils.hpp`). It warns, or with `strict_param_loading` throws,
when a parameter it declares has no override. Phase-488 node 2's port edits
that line to `const bool no_param_override = false;`, which never warns.

## Sketch

- A per-node FFI listing, `nros_cpp_node_param_overrides(node, ...)`, over
  `ArgvParamOverride`s whose `node` is unset or names this node. Each comes
  back as its name plus its TEXT, last one winning per name, as
  `apply_argv_param_override` resolves them.
- The hosted C++ side types each text the way `typed_param_override` types
  one against a `NotSet` declaration (bool, integer, double, string), into
  `ParameterValue`s, and returns the map.
- Array overrides take no override yet (phase-482 W5), so none would be
  listed.

## Done when

Nav2's `declare_or_get_parameter` compiles unchanged on a hosted build
(`rclcpp::exceptions` aside, which RFC-0018 refuses). A runtime test installs
`-p a:=1 -p other_node:b:=x` and lists `{a: 1}` for the first node only.

## Resolution

Built as sketched (phase-518 W1).

- `Executor::argv_param_overrides_on(node)` lists the overrides that apply to
  a node: the last override of a name wins and a `node:` override applies only
  to the node of that name, the same selection a declaration makes. Each text
  is typed as a declaration with no default types it (bool, integer, double,
  then string).
- `nros_cpp_node_list_param_overrides(node, callback, context, out_count)`
  hands each one to the callback as an `nros_cpp_param_write_t`, built by the
  same helper the on-set hook uses. An image without `alloc` has no argv
  overrides and lists none.
- `NodeParametersInterface::get_parameter_overrides()` (hosted) returns them as
  a `const std::map<std::string, rclcpp::ParameterValue>&`, one copy per
  node, as upstream's is. Nav2's `declare_or_get_parameter` calls it twice and
  compares one call's `find()` with the other's `end()`, so returning by value
  would compare iterators of two temporaries. The copy is re-read at each call
  and kept when nothing changed. The ledger row
  `cpp:Node::get_node_parameters_interface` states that bound.
- Tests: `argv_overrides_are_listed_per_node_last_winning` (nros-node) and the
  `get_parameter_overrides` case of `parameter_value_runtime.cpp`, which
  installs `-p a:=1 -p other_node:b:=x`, lists `{a: 1}` for the first node
  and `{a, b}` for `other_node`, and checks that two calls return one map.

Array overrides take no override yet (phase-482 W5), so none is listed.
