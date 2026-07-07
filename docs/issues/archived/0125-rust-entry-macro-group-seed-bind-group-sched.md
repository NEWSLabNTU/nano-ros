---
id: 125
title: Rust nros::main! multi-tier path does not seed bind_group_sched from group_tiers
status: wontfix
type: enhancement
area: codegen
related: [rfc-0047, phase-273, phase-274]
resolved_in: "wontfix (2026-07-07) — inert under phase-274 Model 1"
---

## Wontfix — inert under the phase-274 Model 1 architecture

`bind_group_sched(node, ns, group, sc_id)` does something *different* from the
`active_groups` filter ONLY when one executor hosts multiple callback groups
that need different `SchedContext`s. Phase-274 (RFC-0015 Model 1) converged
every language and platform onto **one executor per priority tier**, so on the
Rust `run_tiers` path:

- each tier = its own `Executor` = its own `SchedContext`, and
- seeding `bind_group_sched` would bind every group in a tier to *that tier's*
  sc-id — **identical routing to what `active_groups` + the separate per-tier
  executors already produce.**

So the missing Rust seed is not merely redundant (as this issue read pre-274) —
it is **fully inert**: emitting it would compute the same schedule the tier
model already gives. It could only matter if a future shape placed multiple
sched-differentiated groups inside ONE Rust executor, which the tier model
specifically does not do and no example exercises.

Implementing Option A/B (a board-API extension across nros-platform +
nros-board-posix + nros-board-freertos + … to expose per-tier sc-ids, plus a
proc-macro emit) would add code for zero observable behavior change — YAGNI.

The C++ codegen still emits `nros_cpp_bind_group_sched(…, __nros_sc_ids[ti])`
(`emit_cpp.rs`); it is harmless and kept for historical continuity. The Rust
`Executor::bind_group_sched` API + `group_sched_table` machinery remain in place
and tested (`nros-node/src/executor/tests.rs`), ready if a shared-executor
multi-group shape ever lands.

## Reopen if

A Rust node ever hosts >1 sched-differentiated callback group in a SINGLE
executor (i.e. an execution shape that is NOT one-executor-per-tier). That would
also be the point at which the planned **full cross-language / cross-platform
execution-model unification** revisits the codegen seam — reopen (or file a
fresh issue) then, wired to that unification work.
