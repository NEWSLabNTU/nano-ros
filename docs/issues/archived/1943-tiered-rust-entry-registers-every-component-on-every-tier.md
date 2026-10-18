---
id: 1943
title: "A multi-tier `nros::main!` entry registers every component on every tier, so a third tier fails every component with an opaque `NodeRegister`"
status: resolved
resolved_in: 2026-10-18
type: bug
area: macros, executor, tiers
severity: high
found: 2026-10-18
related: [phase-462, issue-1535]
---

## What happens

A Rust entry with three tiers boots, and one tier dies during setup:

```
nros: tier `mid` setup failed: NodeRegister("actuator_pkg") — 0 downstream tier(s) will NOT start
```

`actuator_pkg` is not a `mid` node: its only callback group is bound to `high`.
On FreeRTOS the failure stops the spawn chain, so every tier after it never
starts; on Linux the tiers race, so which tier fails, and on which component,
changes from run to run.

Measured on the `nano-ros-rt-eval` safety island (six nodes, tiers
`high`/`mid`/`low`), on `mps2-an385` FreeRTOS under QEMU and on native Linux,
against `main` at `6486216ab7`.

## Why

`nros::main!`'s tiered closure is ONE closure the board runs once per tier
executor, and it spliced every component's register call unconditionally. Each
call installs the component into its per-class store
(`ComponentSlotStorage`, `NROS_RUNTIME_MAX_CLASS_INSTANCES` slots, default 2).
So three tiers installed each component three times, and the third install
found the store full: `install_node_typed_with_launch_in` returned `-3`, which
the `nros::node!` wrapper collapses into `NodeRegister(<pkg>)` with no cause.

The per-tier group filter was never the problem: each tier executor's
`set_active_groups` admits exactly its own `(node, namespace, group)` members,
so the tier created none of a foreign node's ENTITIES. But it still installed
the component and created the node, which is both the slot leak and a duplicate
node on the graph.

The C/C++ packs never had this: each tier's setup is built from the nodes that
tier names (`lower.rs::lower_tiers`).

## Fix

`main_macro.rs::tier_guarded_register_calls`: in both tiered closures (the
OwnedSpin `run_tiers` arm and the Zephyr one), a node's register call runs only
when the tier executor admits one of the node's groups
(`Executor::group_active`). A node the tier table names no group for is
registered on every tier, as before, because its entities carry no group and
every tier's filter admits them.

The node's name comes from its launch IDENTITY: on the Rust road
`LoweredNode::name` / `namespace` are the C-ABI packs' fields and stay empty,
and a first version of the guard that read them matched nothing and guarded
nothing.

Test: `nros-macros` `a_tier_registers_only_the_nodes_whose_groups_it_runs`.
End to end, the eval island's six nodes all tick on all three tiers on
FreeRTOS (`[ctrl]`, `[gate]`, `[mrm]`, `[steer]`, `[vel]`, `[wd]`).

## Not fixed here

- The `-3` is still reported as `NodeRegister(<pkg>)`. It is the per-class
  store being full, which has a knob to name
  (`NROS_RUNTIME_MAX_CLASS_INSTANCES`), the way `-2` already names
  `NROS_EXECUTOR_MAX_CBS`.
