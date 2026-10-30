---
id: 2161
title: "The RTIC emit and `nros::node!` still carry the macro-only dispatch-slot registry, which nothing fills and nothing feeds"
status: open
type: tech-debt
area: [core, executor, macros, rtic]
severity: low
found: 2026-10-30
related: [2146, 2150]
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

Phase-520 W3 (deferred dispatch inside `nros::setup!`) currently says it will
"emit the `register_dispatch` slots". That is the registry this issue
describes as having no consumer. If W3 builds on the executor queue from 2146
(`set_deferred` / `run_deferred`) instead, the registry is left with no
producer either, and retiring it is this issue. W3 covers `setup!` and this
issue covers RTIC's `nros::main!` branch, so the two do not overlap. They
should, however, agree on which mechanism to use.
