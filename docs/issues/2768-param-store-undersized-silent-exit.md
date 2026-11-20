---
id: 2768
title: "An image whose code declares more parameters than its contract states keeps the default parameter store, and its configure fails with exit 250 and nothing on the console"
status: open
type: bug
severity: high
area: [cli, params, sizing, c-api]
related: [issue-1694, issue-2640, issue-2634, rfc-0100]
found: 2026-11-20
---

## What happens

`packages/testing/nros-tests/fixtures/probe_seeds_ws` (row
`workspace-probe-seeds`) launches two components with no `params:` in any
contract. Its C node declares 41 parameters (`ctrl_period_ms` and 40 gains).
The image opens its session, prints the parameter store's size, and exits with
status 250 and no other line:

```
[INFO] nros: ... session open (rmw=zenoh)
[INFO] nros: ... parameter store: 32 slots (275712 B) + state and services (28104 B) carved from the executor backing
$ echo $?
250
```

`rmw_coordinate_truth::a_rows_entry_registers_its_nodes_at_runtime` reports
it as "did not reach node registration". Measured on main `b3bc81ce58` (there
the store is 8 slots) and on a branch with 32; same exit.

## Why

Two defects, one silent and one undersized.

1. **The store is sized from the contract alone.** `ParamDeclarations::from_model`
   reads `contracts.node_params` and nothing else, so with no `params:` the
   descriptor's `[params]` carries no `max_parameters` and the store keeps its
   default. The metadata probe had recorded all 41 declarations in
   `src/seeds_c_pkg/metadata/seeds_c.json`; sizing never read them. Entity
   counts have been floored by those same sidecars since issue 1694; the
   parameter store was not.
2. **The refusal is silent.** The C node's `rclc_parameter_server_init_with_option`
   asks `max_params = 41`; the store holds 32, and the function returned a bare
   `RCLC_RET_FULL`. The component's configure passed it up, and every board's
   `run_components` returned a failed setup's code with no line of its own.

## Fix

- `ParamDeclarations::from_model_with(model, recorded)`: the contract's
  declarations, unioned by name with what each launched node's code declared
  (the probe sidecar's `parameters[]`, reads excluded). The contract's type
  stands where both name a parameter; a node the contract is silent on is
  stated by the record instead of refusing. The record is kept only when every
  launched node has exactly one fresh sidecar (`EntityInventory::recorded_params`),
  so a count over it is never a count over part of the image. Every model road
  that composes the inventory attaches the floored declarations: `nros build`'s
  resolve, `nros dev image sizing-descriptor`, and `nros dev image entity-inventory`
  (single image and shared runtime). The cargo leaf road
  (`sizing_descriptor::write_for_leaf`) reads its own `metadata/` sidecars for
  the same record (`leaf_entity_env::recorded_params_for_leaf`).
- The C parameter server's two init refusals log the node, both numbers and
  the knob. The class sweep found nine more bare `RCLC_RET_FULL`s of a
  descriptor-sized pool: every `rclc_executor_add_*`/`attach_*` and the two
  guard-condition adds at `max_handles` (at most `NROS_EXECUTOR_MAX_CBS`).
  They now go through one reporter, `executor::handles_full`.
- Every board's `run_components` logs a failed setup's code before it exits
  (`rclcpp::detail::report_setup_failed`), so a refusal nothing below reported
  is never silent again.

## Not covered

- A component that creates several nodes has all its recorded declarations
  attributed to the launched node. The store is image-wide, so the total is
  right; only the per-node seeded `use_sim_time` count can come in low by the
  extra nodes.
