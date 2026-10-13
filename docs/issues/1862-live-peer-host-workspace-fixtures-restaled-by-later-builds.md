---
id: 1862
title: "live-peer host job: the workspace fixtures it builds first read STALE at run time, because the row builds after them re-stale them"
status: open
type: bug
area: [ci, testing]
severity: medium
found: 2026-10-13
related: [0835, 0828, 1353, 1846]
---

## Measured

Live-peer regression run
[38321018350](https://github.com/NEWSLabNTU/nano-ros/actions/runs/38321018350)
(`workflow_dispatch`, main at `05a52e7cd`), job 115048869242,
"rows whose board IS this runner". This is the first host run in this window
that got past the fixture build. The earlier ones died on runner disk
(1353/1846). Step "Run the cells with a recorded PASS" failed four cells, every
one at `fixtures/require.rs:78` with a STALE verdict, not a runtime result:

```
FAIL nros-tests::params test_ros2_param_set_reconfigures_live_read
  BuildFailed("Workspace fixture workspace-features-rust-params is stale: …/examples/workspaces/features/target-fixtures/.nros-workspace-fixture.workspace-features-rust-params.inputsig
FAIL nros-tests::params_per_node_interop xrce_set_parameters_naming_25_declared_lands
  … workspace-features-rust-params-xrce is stale …
FAIL nros-tests::rust_multi_node_per_node_graph rust_multi_node_entry_per_node_graph_nodes_cyclonedds
  … workspace-rust-native-cyclonedds is stale …
FAIL nros-tests::rust_multi_node_per_node_graph rust_multi_node_entry_per_node_graph_nodes
  … workspace-rust-native is stale …
```

All four fixtures were built by the SAME job a few minutes earlier, in the
fixture-build step, log lines ~3886–4938:
`workspace-fixtures-build.sh linux --id <row>`, each with a `row-time:`. In the
same step, after them, came:

1. `fixtures-build.sh` for six derived triples (`linux c cyclonedds`,
   `c zenoh`, `cpp zenoh`, `rust cyclonedds`, `rust xrce`, `rust zenoh`).
2. `compile-check-fixtures.sh` for the derived compile-check rows.

Then the cells ran and found the workspace rows' input signatures no longer
matching.

## Lead (not measured)

The step's own comment states the rule: "Order is load-bearing: the CLI first,
then anything that runs `nros sync` — it rewrites `generated/` and re-stales
every row fixture built before it — then the rows." The workspace loop
(`--host-workspace-fixtures`, `.github/workflows/live-peer.yml` ~line 294) runs
BEFORE the triple loop (~312) and the compile-check loop (~326). Both reach
codegen or `nros sync` (`fixtures-build.sh` runs `nros_require_sync` and
`nros generate-rust`), so one of them plausibly rewrites an input the workspace
`.inputsig` hashes. Archived issue 0835 is the same class: fixture families
whose builds re-stale each other.

Not yet known: WHICH input moved. Read the `probe:` / NOT RUN lines, or diff the
`.inputsig` before and after the triple builds, before choosing the fix.

## What it is not

- It is not 1353/1846 (disk). This run had space and reached the cells.
- It is not a runtime failure of params or per-node graph. None of the four
  launched.
- It is not 1758 (a fixture nothing built). These were built, then re-staled.

## What would close it

1. Name the input that moves (`.inputsig` before and after the triple and
   compile-check builds).
2. Then either build the workspace rows LAST in that step, or stop the later
   builds rewriting the shared input. The second fixes the class (0835).
3. Acceptance: a live-peer dispatch where these four cells produce a verdict
   rather than STALE.
