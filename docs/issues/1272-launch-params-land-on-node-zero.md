---
id: 1272
title: "Launch parameters for every node of a multi-node entry are declared on
  the executor's primary node, with their types guessed from strings"
status: open
type: bug
area: codegen, core
severity: medium
related: [issue-0745, issue-1269]
---

## What happens

A launch file can set parameters per node. In a multi-node entry, all of them
end up on node 0:

- **C++ entries.** The entry emitter writes one
  `nros_cpp_declare_param(executor, "<name>", "<value>")` per node parameter
  (`nros-cli-core/src/codegen/entry/emit_c.rs`). The call carries no node, and
  `nros_cpp_declare_param` (`nros-cpp/src/params_shim.rs`) calls
  `executor.declare_parameter(name, value)`, which declares on the executor's
  PRIMARY node.
- **Rust entries.** `nros::main!` gathers every node's parameters into one
  flat list, and `apply_param_services` declares each with the same
  `declare_parameter` (`nros/src/main_macro.rs`, `nros/src/node_runtime.rs`).

Consequences:

- a parameter set for node B is held by node A, so `ros2 param get /B x`
  cannot find it (once node naming works at all, see issue 1269) and B reads
  its own default;
- two nodes that set the same name collide: the second declaration is
  refused.

## Types are guessed

The value reaches the store as a string and its type is inferred -- bool,
then i64, then f64, otherwise string (`infer_param_value`). The resolved model
has no array type: a YAML sequence becomes a string list, baked as a
comma-joined string, so a `double[]` parameter arrives as a string.

## Not observed on a running image

The downstream this was found beside (Autoware Safety Island) sets no launch
parameters, so it has not hit this; both paths were traced by reading.

## Fix shape

- Seed each parameter on its own node: `declare_parameter_on(<node key>, ...)`,
  and give the C++ shim the node index.
- Carry the parameter's type from the resolver (or from a declaration, if the
  contract grows one) instead of inferring it from the text.
