---
id: 1272
title: "Launch parameters for every node of a multi-node entry are declared on
  the executor's primary node, with their types guessed from strings"
status: resolved
type: bug
area: codegen, core
severity: medium
resolved_in: "fix(#1272): each launch parameter is seeded on its own node"
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

## Resolution

Each launch parameter is now seeded on the node it was set for. The type is
still inferred from the string; carrying the declared type is phase-446 W2/W3's
second half and needs the contract `params:` section (W1) first.

- **C and C++ entries.** `nros_cpp_declare_param` takes the node:
  `nros_cpp_declare_param(executor, node, name, value)` (C ABI change). The
  seed runs BEFORE the node is built (issue 0745), so `node` is a prediction:
  the index the executor's `node_builder` will hand that node, which the entry
  emitter computes as the node's position among the nodes its setup function
  builds on that executor (per tier for a tiered entry, since each tier has its
  own executor). The shim declares with `declare_parameter_on(node, ...)` and
  REFUSES a seed whose index is not the executor's next node index, logging the
  parameter and both indices. That check is what keeps the prediction honest:
  a component that built two nodes, or none, would otherwise shift every later
  node's values onto a neighbour. The C pack now emits its seeds before
  `nros_cpp_node_create`, as the C++ pack already did, so both packs make the
  same prediction.
- **Rust entries.** `nros::main!` no longer flattens every node's parameters
  into `apply_param_services`, which now only registers the services and
  creates the store. Each node's launch values already reached its `register`
  call through `RuntimeCtx::params`; `ExecutorSink::create_node` seeds them on
  the `NodeId` `node_builder` just returned, so nothing is predicted. A
  component's own source-declared parameter (`EntityKind::Parameter`) is
  declared on its own node too, where it also went to the primary.

Tests: `launch_seeds_made_before_construction_land_on_the_node_built_later`
(nros-node) seeds two nodes with the same name before building them and checks
each reads its own value and that a later source default adopts the seed; the
codegen unit tests pin the per-node and per-tier indices and the seed-before-
create order in both packs.

Not changed: the executor-scoped C getters (`nros_cpp_get_param_integer` and
siblings) still read the primary node, as documented. A C component on a
second node that wants its own launch value reads through the node-scoped
`nros_cpp_node_get_param_*` family.
