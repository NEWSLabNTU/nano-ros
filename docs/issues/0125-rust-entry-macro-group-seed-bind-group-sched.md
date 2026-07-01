---
id: 125
title: Rust nros::main! multi-tier path does not seed bind_group_sched from group_tiers
status: open
type: enhancement
area: codegen
related: [rfc-0047, phase-273]
resolved_in:
---

## Problem

Phase 273 W4 (RFC-0047) lifted the `NodeSpansTiers` constraint and proved the
sub-node pattern (ONE node, TWO callback groups, TWO tiers) via the C++ entry
path (`nros codegen entry --lang cpp`). The C++ codegen emits
`nros_cpp_bind_group_sched(__exec, node, ns, group, sched_ctx_id)` for each
`group_tiers` member, binding the (node, group) pair to the correct
`SchedContextId` before the component constructor runs.

The Rust `nros::main!` multi-tier path (`run_tiers`) does NOT emit equivalent
`executor.bind_group_sched(node, ns, group, sc_id)` calls. The `group_tiers`
resolver output (which groups each tier's members contains) is available in the
`ResolvedTierTable` at macro expansion time, but the `SchedContextId` values are
**runtime**-assigned by the board during `run_tiers` — they are not known at
proc-macro expansion time.

## Current Behaviour

For the Rust multi-tier path the routing still works correctly via the
`active_groups` filter (each tier's `Executor::set_active_groups` admits only
its assigned group names). The `bind_group_sched` table is NOT consulted for
`active_groups` dispatch — it is used for a finer-grained concern: assigning a
per-group `SchedContext` (priority/period) within a single executor. Since the
Rust `run_tiers` path spawns **one executor per tier**, the SchedContext IS
already tier-specific; the group → SchedContext binding is redundant for the
multi-tier Rust shape.

## When This Matters

The gap becomes observable if a future Rust node implementation uses
`create_callback_group` + relies on `apply_node_default_sched`'s
`group_sched_table` lookup for priority assignment beyond what the active_groups
filter provides. Today's Rust nodes (including examples/workspaces/ws-realtime-rust)
use the `active_groups` mechanism exclusively and do not invoke `bind_group_sched`.

## Direction

Option A (preferred): extend the `run_tiers` closure signature to accept a
`&mut dyn FnMut(node, ns, group, SchedContextId)` callback that the board invokes
once per tier member after creating each tier's SchedContext. The proc-macro emits
the closure; the board calls it with real runtime IDs.

Option B: add a `bind_group_sched` method to `RuntimeCtx` and call it from the
existing setup closure per tier, passing the tier's SchedContextId obtained via
`runtime.tier_sched_context_id()` (new board API).

Both options require a board-API extension (nros-platform + nros-board-posix +
nros-board-freertos + ...) and a matching proc-macro change. Defer to a follow-up
phase after phase-273 W4 lands.

## Evidence

- `packages/core/nros-macros/src/main_macro.rs` — `tier_specs_tokens` uses only
  group names; does not emit `bind_group_sched` calls.
- `packages/boards/nros-board-posix/src/lib.rs` — `run_tiers` creates SchedContexts
  internally; no external hook for the proc-macro to learn the IDs.
- C++ reference: `packages/cli/nros-cli-core/src/orchestration/codegen_entry.rs` —
  `emit_sched_context_wiring` already emits group-seed for the C++ path.
