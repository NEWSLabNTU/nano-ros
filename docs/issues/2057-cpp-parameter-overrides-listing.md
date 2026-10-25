---
id: 2057
title: "C++ has no get_parameter_overrides(): the executor applies -p / --params-file overrides but cannot list them"
status: open
type: limitation
area: [api, cpp, params]
severity: low
found: 2026-10-25
related: [2056, phase-488, phase-482]
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
