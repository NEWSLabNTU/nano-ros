---
id: 2161
title: "The RTIC emit and `nros::node!` still carry the macro-only dispatch-slot registry, which nothing fills and nothing feeds"
status: resolved
resolved: 2026-10-31
type: tech-debt
area: [core, executor, macros, rtic]
severity: low
found: 2026-10-30
related: [2146, 2150, phase-520]
---

## What

Issue 2146 gave the executor a hand-callable deferred-dispatch queue
(`Executor::set_deferred` / `set_defer_notifier` / `run_deferred` /
`run_deferred_handle`). It asked for the RTIC emit to move onto it "so there is
one ring and not two". That part was not done, and reading the code for it
showed the older path is mostly dead:

- `Executor::register_dispatch_slot` / `Executor::dispatch_callback` /
  `DispatchSlot` / the carved `dispatch_slots` region
  (`packages/core/nros-node/src/executor/spin.rs`) are reached only by the
  `register_dispatch(executor)` fn `nros::node!` emits
  (`packages/core/nros-macros/src/lib.rs`) and by
  `nros-build::emit_run_plan_register_dispatch`. Nothing calls either
  (`git grep 'register_dispatch('` finds the definitions, a unit test and
  Cargo.toml comments in the eight `*-rtic` examples).
- The RTIC `__nros_run` task (`packages/core/nros-macros/src/main_macro.rs`,
  the `Framework::Rtic` branch) spins with `ExecutorNodeRuntime::spin_once`, so
  every callback runs inline in that task. It then drains the board's SPSC
  queue through `ExecutorNodeRuntime::dispatch_callback`, but the only producer
  of that queue is `RticRuntime::signal_callback`, which only the
  `e2e-synthetic-callback` feature calls
  (`packages/boards/nros-board-mps2-an385/src/rtic.rs`).

So on RTIC "Deferred" is a queue with no producer, and the executor carries a
`MAX_NODES` slot registry with no consumer.

## Direction

Either move the RTIC emit onto the executor queue (mark the node's handles
deferred, have the notifier pend an RTIC software task at the callback's
priority, drain with `run_deferred_handle` there), or, if RTIC keeps running
callbacks inline, retire the dead registry: `register_dispatch_slot`,
`dispatch_callback`, `DispatchSlot`, the `dispatch_slots` region and its
`nros-executor-layout` unit, the macro's `register_dispatch` and
`__nros_node_<pkg>_on_callback`, and `emit_run_plan_register_dispatch`.

Not done with 2146 because issue 2150 is rewriting the same part of
`main_macro.rs` in parallel, and because either change needs the RTIC QEMU
cell run, not just a build.

## Relation to phase-520 W3

**Decided 2026-10-31 (W3 landed):** `nros::setup!` builds on the executor
queue (`callback_mark` / `set_deferred_since` around the component's
`register`; `nros_run_deferred` for the host). So the slot registry below now
has no producer on that road either; what remains of it is RTIC's `nros::main!`
branch, this issue.

(Before W3 landed this section noted that W3 planned to "emit the
`register_dispatch` slots", the registry this issue describes as having no
consumer, and asked the two to agree on one mechanism. They did: the queue.)

## Resolution

**2026-10-31 — retired.** RTIC keeps running its callbacks inline in its one
`__nros_run` task, as it has since phase-289, so the registry is the
"retire" half of the Direction. Nothing behaves differently at runtime: nothing
ever called `register_dispatch` or `run_plan_register_dispatch`, and the RTIC
drain goes through `ExecutorNodeRuntime::dispatch_callback`, which scans the
component table, not this registry.

Removed:

- `nros-node`: `DispatchSlot`, `Executor::register_dispatch_slot`,
  `dispatch_slot_count`, `Executor::dispatch_callback`, the carved
  `dispatch_slots` region (`ExecutorStorage`, the backing's size and offset
  tables) and its `dispatch_registry_tests` module. Each Node slot in the
  backing is one `DispatchSlot` smaller.
- `nros-executor-layout`: the `dispatch_slot` unit and `dispatch_slots`
  offset.
- `nros-macros` (`nros::node!`): the `register_dispatch(executor)` fn and the
  `#[no_mangle] __nros_node_<pkg>_on_callback` trampoline.
- `nros`: `__macro::__private_node_state_into_raw` (its only caller was
  `register_dispatch`) and the link probe for the trampoline symbol.
- `nros-build`: `emit_run_plan_register_dispatch`, the second fn it appended
  to every generated `run_plan.rs`, and its two tests.

Comments that named the registry were corrected: the eight RTIC example
manifests and `rtic-run-plan-e2e`, the two `n_board_agnostic_run_plan`
entries, the RTIC branch of `main_macro.rs`, `deferred.rs`, RFC-0032's
framework table, RFC-0043 (two "stays name-dispatched" passages marked
superseded) and RFC-0112's Defer row.

**Phase-520 W3** landed on the issue 2146 executor queue (PR #2896), so it
needs nothing from this registry.

**Not done, and not filed:** running RTIC callbacks at their own RTIC priority
(mark the handles deferred, have the defer notifier pend a software task, drain
with `run_deferred_handle`). It is a feature nothing asks for today; RFC-0112's
Defer row says it would build on the 2146 queue.
