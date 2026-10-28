---
id: 2134
title: "`nros::main!`'s Embassy branch has no board and no CI cell, so nothing checks it — keep it as a Tier 2 seam or demote it to a recipe?"
status: open
type: tech-debt
area: [macros, boards, embedded]
severity: low
found: 2026-10-29
related: [0248, 0415]
---

## What

`nros::main!` has an Embassy entry branch (about 136 lines in
`packages/core/nros-macros/src/main_macro.rs`), backed by the
`EmbassyBoardEntry` trait (`packages/platform/nros-platform/src/board/embassy_entry.rs`).
Since phase-337 W7.a deleted `nros-board-embassy-stm32f4`, no in-tree board
reaches it. There is no fixture row and no runtime cell. The only tests are
unit tests on name resolution: the macro maps `"embassy"` to
`Framework::Embassy`, and the `nros ws check` dispatch lint reads a stub board
crate's metadata. Neither checks that the emitted code compiles against a real
Embassy executor, or that it boots and delivers.

The book says so (`book/src/user-guide/embassy-integration.md` has a status
banner), and calls what remains "the seam". An out-of-tree board can reach the
branch through `[package.metadata.nros.board] framework = "embassy"`
(issue 0415).

## Why it is a decision, not a fix

RFC-0112 D3 admits a framework to `nros::main!` codegen only with an in-tree
board, a CI cell that delivers, a need the host-driven contract cannot serve,
and a named owner. Embassy fails the first two, and arguably the third: an
async task can drive the executor once RFC-0112 D2.2's readiness notifier
replaces `spin_async()`'s self-waking loop. Two ways to resolve it:

- **Demote.** Delete the macro branch and `EmbassyBoardEntry`. Remove
  `embassy` from `FRAMEWORKS`, and make the out-of-tree metadata value a
  refusal that points at the recipe. Ship an Embassy recipe under RFC-0112 D4
  once the notifier exists. Its CI cell can run on a host executor
  (`embassy-executor`'s std arch), so it needs no board. This costs any
  out-of-tree user of the seam a migration. None is known.
- **Keep as a Tier 2 seam.** Leave the branch, and add one compile-only fixture
  that expands it against a stub board crate and a real `embassy-executor`, so
  at least "it still compiles" is checked. This keeps an unexercised runtime
  path, which RFC-0112 D3 says not to do. It would need a ruling that a seam
  with a compile cell is an acceptable exception.

## Owner

Maintainer ruling needed. Do not change code until it is given.
