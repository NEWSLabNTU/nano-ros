---
id: 2858
title: "The C and C++ lifecycle change-state, get-state and register shims find no machine once an executor holds two managed nodes"
status: resolved
type: bug
severity: medium
area: [c-api, cpp-api, lifecycle]
related: [phase-554, issue-2429, issue-2626, issue-2857]
found: 2026-11-23
---

**Home: [phase-554](../../roadmap/phase-554-per-node-lifecycle.md) W9** (and the
per-node machine of W1). Found while landing W6 (#4634); batched with W9,
whose two-managed-nodes case is what proves the fix.

## What happens

Since W1 every managed node has its own machine, keyed by its `NodeId`. The C
and C++ shims still ask for the EXECUTOR-scoped one:

- C (`packages/api/nros-c/src/lifecycle.rs`):
  `rclc_executor_lifecycle_get_current_state`,
  `rclc_executor_lifecycle_change_state` and every
  `rclc_executor_lifecycle_register_on_*` call
  `exec.lifecycle_state_machine{,_mut}()`.
- C++ (`packages/api/nros-cpp/src/lifecycle_shim.rs`):
  `nros_cpp_lifecycle_change_state`, `nros_cpp_lifecycle_get_current_state`
  and the `nros_cpp_lifecycle_register_on_*` family do the same, although
  `nros_cpp_lifecycle_register_node` registers the machine ON the node
  (`register_managed_node_on`).

`Executor::lifecycle_entry(None)` answers the machine with no node, or the
ONLY machine when there is exactly one. With two or more per-node machines
it answers nothing, so these calls return `RCL_RET_NOT_INIT` /
`NROS_CPP_RET_NOT_INIT` (a callback registration is lost the same way). One
managed node works by the fallback, which is why nothing has failed yet.

The service path is not affected: `ros2 lifecycle set|get` reaches each
node's own machine through its own services.

## Not measured yet

Read from the code. nros-node's
`two_managed_nodes_have_independent_state_machines` asserts the lookup half
(`lifecycle_state_machine().is_none()` with two machines); no C or C++ test
drives two managed nodes.

## Fix direction

The shims take the node they act on: the C++ ones are called from a
`LifecycleNode`, which knows its node; the C ones need a node argument (a new
`rclc_node_lifecycle_*` family beside the executor-scoped one, which stays for
the single-machine image). Then the lookup is `lifecycle_state_machine_on(id)`.

Acceptance: W9's "two managed nodes with independent states" cell, in C and
C++.

## Resolution (2026-11-23)

- **C:** a node-scoped family beside the executor-scoped one, which stays for
  the single-machine image: `rclc_node_register_lifecycle_services(node)`
  (services under the node's own name, with a machine of its own),
  `rclc_node_lifecycle_get_current_state`, `rclc_node_lifecycle_change_state`
  and `rclc_node_lifecycle_register_on_*`. Each takes a node bound with
  `rclc_executor_node_init` and acts on `lifecycle_state_machine_on(id)`.
- **C++:** `nros_cpp_lifecycle_node_{change_state,get_current_state,bind}`
  take the node's handle. `rclcpp_lifecycle::LifecycleNode` uses them whenever
  it is bound to a node, which every constructor does; the executor-scoped
  shims remain for an unbound engine.

Measured on W9's cells (`lifecycle_pair_interop`, C and C++): two managed
nodes in one image, each driven by `ros2 lifecycle` to its own state, each
running its own callbacks. Negative control (C++): with the callbacks bound
through the executor-scoped calls, the second node's `on_configure` never
runs, because those calls find no single machine once the first node has one.
