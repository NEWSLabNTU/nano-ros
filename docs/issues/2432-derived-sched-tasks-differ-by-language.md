---
id: 2432
title: "The same workspace and contract get a derived rate-monotonic schedule in C and C++ and none in Rust, silently, so the Rust image's 100 Hz loop misses its periods"
status: open
type: bug
severity: medium
area: [scheduling, codegen, cli, macros]
related: [rfc-0079, 2269]
found: 2026-11-11
---

Found while building the two-tier lab of issue 2269 on `mps2-an385-freertos`.
**This is a parity defect, and it is silent.** No build or boot line says
that the Rust image took a different schedule from its C and C++ twins.

## Measured

2026-11-11, `main` after #3508 (cb799bc93b).

One workspace shape in all three languages:
- `nros new control_ws --board mps2-an385-freertos --template talker --lang <L>`.
- `nros new telemetry_node --template talker --lang <L> --to control_node_bringup`.
- `control_node` runs a 10 ms timer in callback group `ctrl` and logs, per 100
  ticks, the longest gap between two ticks against a 50 ms budget. After
  1000 ticks it prints `10 windows, <n> late`.
- `telemetry_node` runs a 100 ms timer in group `telem` whose callback
  busy-waits 60 ms.
- The bringup's contract (`system.contract.yaml`) states `params: {}` and one
  timer path per node: 100 Hz and 10 Hz.
- `system.toml` authors **no** `[tasks]` and no `group_tasks`.

| Language | Schedule the image got | `nros run` result |
|---|---|---|
| C | the entry codegen derives two sched tasks from the contract's rates: `derived-control_node` at FreeRTOS priority 3 and `derived-telemetry_node` at 2. Boot: `scheduler core=0 levels=2`, `multi-level run — 2 level(s) over one executor` | `10 windows, 0 late` (longest gaps 15–19 ms) |
| C++ | the same derivation | `10 windows, 0 late` (13–18 ms) |
| Rust | nothing derived: no `derived-*` line at build, no multi-level line at boot. Both nodes run on one task | `10 windows, 10 late` (longest gaps 146–153 ms) |

The C build prints the plan:

```text
codegen entry: sched task `derived-telemetry_node` spin period 100000 us, derived from the sched task's own period
codegen entry: priority plan (freertos, bigger-is-urgent): transport [4, 4], pool.app [1, 3] - the sched task key's projection
codegen entry:   sched task derived-control_node: priority 3 (derived)
codegen entry:   sched task derived-telemetry_node: priority 2 (derived)
codegen entry:   level 0: priority 3, threshold rank 0, members /control_node (rank 0)
codegen entry:   level 1: priority 2, threshold rank 1, members /telemetry_node (rank 1)
```

The Rust build and boot print none of it.

With `[tasks]` AUTHORED, the three languages agree. With both groups on one
task `main` (priority 2), each prints `10 windows, 10 late`. With `ctrl` on
`[tasks.high]` (priority 3), each prints `10 windows, 0 late`.

## Where the derivation lives

All three roads reach the same shared core,
`nros_orchestration_ir::derive`. Each asks `placement_is_unauthored` and then
calls `derive_sched_tasks_from_contracts` / `derive_sched_tasks_in_plan`:

- C/C++ entry: `packages/cli/nros-cli-core/src/codegen/entry/mod.rs` (the
  `placement_is_unauthored` guard, then the derive call). This is the road
  that derived above.
- Rust entry: `nros::main!`, `packages/core/nros-macros/src/main_macro.rs`
  (the same guard and call, "so a pure-cargo Rust entry gets an identical
  table"). This is the road that derived nothing above.
- `packages/cli/nros-cli-core/src/cmd/codegen_system.rs` (the `codegen-system`
  verb).

So the Rust side calls the derivation and yet the image ran on one task. Why
it produced no table is not determined here. Two inputs differ between the
roads and are candidates:
- the contract the macro's model folds in: this workspace's contract is the
  leaf `system.contract.yaml`, not a launch-file contract;
- the callback groups it sees (`node_groups`).

## What is not decided here

Which side is right is a design call, and this issue does not make it:

- **Derive everywhere.** The Rust road gets the C/C++ table: an image whose
  contract states rates is scheduled rate-monotonic unless `[tasks]` says
  otherwise.
- **Derive nowhere** on a leaf contract. C/C++ stop deriving, and an
  unauthored image runs on one task in every language.

Either way, the result should not depend on the language. A build that
derives, or declines to, should say which it did.

## Next

Rule the design, then make the three roads agree. Add a cell that builds one
contract in all three languages and compares the sched-task tables. The
two-tier lab authors `[tasks]` explicitly, so its lesson does not depend on
this.
