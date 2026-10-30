---
id: 2155
title: "nros::main!'s Embassy arm calls methods no trait declares, and nothing in the tree expands it"
status: wontfix  # duplicate of issue 2134
resolved: 2026-10-30
type: bug
area: [api, macros, boards]
severity: low
found: 2026-10-30
related: [phase-337, phase-515]
---

## What

A board whose crate declares `[package.metadata.nros.board] framework =
"embassy"` gets the `Framework::Embassy` arm of `nros::main!`
(`packages/core/nros-macros/src/main_macro.rs`). That arm's
`__nros_run_task` calls, on the board's associated types:

- `executor.spin_once(Duration)` and `executor.dispatch_callback(cb_id, ctx)`
  — on `<Board as EmbassyBoardEntry>::Executor`, bounded only `'static`;
- `runtime.take()`, then `.into_inner()` on what it yields, then `.cb_id` /
  `.ctx_ptr` — on `<Board as EmbassyBoardEntry>::Runtime`, bounded
  `NodeDispatchRuntime + 'static`.

`NodeDispatchRuntime` (`packages/platform/nros-platform/src/board/runtime.rs`)
declares no `take`. So the expansion compiles only for a board whose CONCRETE
types happen to carry inherent methods of exactly those names and shapes — a
contract stated nowhere: not in the `EmbassyBoardEntry` trait, not in
`book/src/user-guide/embassy-integration.md`, not in
`book/src/porting/stm32f4-out-of-tree.md`, which is where that chapter sends a
reader to write the board.

The one board that met it, `nros-board-embassy-stm32f4`, left in phase-337
W7.a. Since then no in-tree crate selects `framework = "embassy"` and no test
expands the arm (the framework tests in `main_macro.rs` parse the name; none
compiles the emitted body), so a change that breaks it is invisible until an
out-of-tree user builds.

## Options

- State the contract in the trait: add `take` (and the envelope shape) to
  `EmbassyBoardEntry::Runtime`'s bound, and bound `Executor` by whatever trait
  carries `spin_once` / `dispatch_callback`; then add a compile-check fixture
  with a stub Embassy board that expands the arm.
- Or retire the arm and `EmbassyBoardEntry` with the board, and refuse
  `framework = "embassy"` with a hint, as phase-337 did for the STM32F4 keys.

## Acceptance

Either a build-stage fixture expands the Embassy arm against a stub board
whose types satisfy only the TRAIT bounds, or the arm is gone and the
framework name is refused.

## Resolution (2026-10-30)

Duplicate of issue 2134, which asked the same question (keep or demote the
Embassy branch) a day earlier. The maintainer ruled to demote: the arm and
~~`EmbassyBoardEntry`~~ are deleted and `framework = "embassy"` is refused with
a hint. This issue's finding (the arm's unstated `take` / `spin_once` /
`dispatch_callback` contract) is recorded in 2134's resolution.
