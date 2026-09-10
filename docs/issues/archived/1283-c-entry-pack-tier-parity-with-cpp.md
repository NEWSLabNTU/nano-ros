---
id: 1283
title: "The C entry pack drops per-group scheduling and disagrees with the C++ pack
  on what counts as tiered"
status: resolved
type: bug
area: cli, codegen
severity: medium
found: 2026-09-11
resolved: 2026-09-11
related: [issue-1172, issue-1285, issue-1286, phase-432, rfc-0091]
---

## What happens

The two entry packs lower the SAME plan into different programs in two ways.

**1. The C pack has no sched-context path.** For a single-executor plan whose
callback groups or nodes carry scheduling (a "group-split" plan), the C++ pack
emits `nros_cpp_create_sched_context_from_policy` plus
`nros_cpp_bind_node_name_sched` / `nros_cpp_bind_group_sched`. The C pack emits
none of it. The image builds and runs, and every group runs at the executor's
default scheduling. Nothing reports the loss. This holds on every board, not
just one RTOS.

All three calls are already C ABI (`nros_cpp_ffi.h`, around lines 1324–1383),
so nothing but the emitter stands between a C entry and the same wiring.

**2. The packs use different "is this tiered?" predicates.**

| pack | predicate | source |
| --- | --- | --- |
| C | `!tiers.is_empty()` | `emit_c.rs` |
| C++ | `!is_single_tier()` | `emit_cpp.rs` |

`resolve_tiers` synthesises ONE `default` tier whenever a node declares callback
groups or node overrides exist, even with no `[tiers]` table. For such a plan
the C pack calls `nros_board_<rtos>_run_tiers` with one tier, while the C++
pack calls `run_components` on the single executor. On FreeRTOS, Zephyr and
NuttX that is a quiet divergence. On ThreadX there is no `run_tiers` symbol at
all (issue 1286).

## How it was found

By reading, during the phase-432 W3.1 ThreadX probe (2026-09-11), not by a
failing test. No in-tree fixture is known to exercise either path in C, which
is why nothing caught it. Issue 1172 was the same shape one layer down: the two
packs derived one fact two ways, and C failed open.

## Fix

- ONE predicate, owned by the plan (for example `plan.is_multi_tier()`),
  consumed by both packs. No pack re-derives it.
- Port the sched-context emission into the C pack, reusing whatever the C++ pack
  derives its bindings from (the `tier_group_keys` pattern from issue 1172: one
  derivation, two spellings).

## Acceptance

- A unit test that runs the same plan through both packs and asserts they take
  the same branch (multi-tier / single executor with sched-contexts / plain
  single executor). It must fail against the old predicates, mutation-checked.
- Golden cases for C: a group-split plan (emits the three sched calls) and a
  callback-groups-no-`[tiers]` plan (calls `run_components`, not `run_tiers`).

## Resolution

Fixed in `5ecbb0ca0` ("fix(#1283): the C entry pack takes the plan's executor
branch, sched contexts included"). Both claims were confirmed by reading the
code before the change: `emit_c.rs` asked `!t.tiers.is_empty()` where
`emit_cpp.rs` asked `!t.is_single_tier()`, and the C template had no sched
block at all.

**One predicate, owned by the plan.** `packages/cli/nros-cli-core/src/codegen/entry/mod.rs`
now has `Plan::is_multi_tier()` and `Plan::executor_shape()`, which returns
`ExecutorShape::{Single, SchedContexts, Tiers}`:

- `Single`: no resolved tiers, or only the synthesised `default` tier.
- `Tiers`: multi-tier, no group split, and the board has `run_tiers`.
- `SchedContexts`: multi-tier, but a node's groups span tiers (RFC-0047), or the
  board has no `run_tiers` (ThreadX, issue 1286).

Both packs branch on it, and neither `emit_c.rs` nor `emit_cpp.rs` asks the
question itself any more. The board half moved with it as
`board_has_run_tiers`, which retired the C++ pack's four-helper board
expression and the two helpers that existed only for it
(`board_is_freertos_embedded`, `board_is_nuttx`). The only pack-side refinement
left is C++'s metadata probe, which maps `Tiers` to `SchedContexts` and has no
C counterpart.

**One sched derivation, two spellings.** `sched_view()` (plus `SchedView` /
`SchedContextView` / `NodeBindView` / `GroupBindView`) moved from `emit_cpp.rs`
into `mod.rs`, and both packs render it. The C spelling is in
`packs/entry/c/entry.c.jinja`. It passes the setup's own `executor` handle,
which is the same `nros_cpp_init` context C++ reaches through
`::nros::global_handle()`, uses `NULL` for unset strings, and seeds every
table before the first `nros_cpp_node_create`. It is C99:
`c_native_group_split.c.golden` compiles clean with
`cc -std=c99 -pedantic -Wall -Wextra -fsyntax-only` against the real
nros-c / nros-cpp headers.

**Tests.**

- `codegen::entry::golden::both_entry_packs_take_the_plans_executor_branch`
  renders four plans through BOTH packs on native, zephyr, nuttx and freertos
  (16 rows) and reads the branch from what each TU calls. The four plans are
  multi-tier, group split, callback groups with no `[tiers]` (resolved through
  the real `resolve_plan_sched`), and no tiers. ThreadX is excluded because the
  C pack refuses it (no C-ABI runner). Mutation-checked:
  - the old C predicate alone fails on `native / groups, no [tiers]`
    (C: `Tiers`, want `Single`);
  - dropping the C sched arm alone fails on `native / group split`
    (C: `Single`, want `SchedContexts`).
- New goldens: `testdata/entry/c_native_group_split.c.golden` (the three sched
  calls) and `testdata/entry/c_native_groups_default_tier.c.golden`
  (`run_components`, no tier table). No existing golden moved. That includes
  `cpp_native_group_split` and `cpp_threadx_tiers`, whose sched block is now fed
  from `mod.rs` and is byte-identical.

**Not changed, noted.** Both packs ignore the return code of
`nros_cpp_bind_node_name_sched` / `nros_cpp_bind_group_sched`. A full binding
table would still drop a group's scheduling silently. The C pack mirrors C++
here on purpose, so the two stay one program. Changing it means changing both
packs and the C++ goldens, and is not this issue's scope.
