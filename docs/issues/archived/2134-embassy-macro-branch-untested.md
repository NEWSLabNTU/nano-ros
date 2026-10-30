---
id: 2134
title: "`nros::main!`'s Embassy branch has no board and no CI cell, so nothing checks it — keep it as a Tier 2 seam or demote it to a recipe?"
status: resolved
resolved: 2026-10-30
type: tech-debt
area: [macros, boards, embedded]
severity: low
found: 2026-10-29
related: [0248, 0415, 2143, 2144, 2145, 2146, 2150, 2155]
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

## Worked comparison (2026-10-29)

The same one-listener app, written each way.

**Keep.** The app is `nros::main!();` plus `[image.embassy] board = "…"` in
`system.toml`, and gets component install, the run plan, the deploy overlay and
deferred-dispatch registration from the model. But a board crate must exist
first: an `EmbassyBoardEntry` impl (spawner, executor, a `NodeDispatchRuntime`
with its channel, all hardware and network bring-up), the
`[package.metadata.nros.board] framework = "embassy"` row and a `build.rs`
calling `nros_build::emit_board_framework()`. And what the branch emits today is
itself a poll loop:

```rust
loop {
    let _ = executor.spin_once(Duration::from_millis(1)); // blocks the Embassy thread up to 1 ms
    while let Some(env) = runtime.take() { executor.dispatch_callback(..) }
    Timer::after_millis(1).await;                         // fixed 1 ms tick
}
```

So it wakes every millisecond when idle and stalls every other Embassy task for
up to 1 ms a cycle (issue 2145's defect, emitted). Its comments also still call
`dispatch_callback` a "no-op stub", which has been false since the executor
grew real dispatch slots.

**Demote, today's API.** A hand-written `#[embassy_executor::main]` opens the
`Executor`, creates its nodes, and spawns a task that loops
`spin_some(Duration::ZERO)` then `Timer::after_millis(1).await`. Already better
than the emitted loop (never blocks inside a poll), still a 1 ms tick. It loses
the `system.toml` assembly, and callbacks run inline because deferred dispatch
needs the macro's node ABI (issue 2146).

**Demote, after 2143 + 2144.** The task waits on
`select(notified, Timer::at(next_deadline))` and sleeps until there is work.
This is the first shape that is correct, and it is independent of Embassy:
the same loop serves any async host.

**A setup-only macro (issue 2150).** Split what `nros::main!` emits into the
assembly every host wants (`nros_setup(&mut Executor)`, from `system.toml`) and
the loop that differs per framework (the host keeps it). That gives a recipe
the Keep option's assembly UX with no Embassy-specific code in nano-ros, and
it is where 2146 is easiest to solve.

| | Keep | Demote (today) | Demote + 2150 |
|---|---|---|---|
| App code | ~5 lines | ~40–80 + hand wiring | ~20 |
| `system.toml`-driven | yes | no | yes |
| Board crate required | yes | no | no |
| Idle behaviour | 1 ms poll, blocks | 1 ms poll | sleeps (after 2143/2144) |
| Deferred dispatch | yes | no (2146) | yes |
| Embassy-specific code kept | ~136 macro lines + trait | none | none |
| CI-checkable | needs a board | std-arch executor | std-arch executor |

Recommendation recorded for the ruling: demote now, the 2143/2144 loop next,
issue 2150 to restore the assembly for every host. Keep only if an Embassy
board arrives with a cell and an owner, and then rewrite its emitted loop.

## Owner

Maintainer ruling given 2026-10-30: demote (see Resolution).

## Resolution (2026-10-30)

**Demoted.** The maintainer ruled for the recommendation above.

- The `Framework::Embassy` arm of `nros::main!` and the ~~`EmbassyBoardEntry`~~
  trait are deleted (with `board/embassy_entry.rs`).
- `FRAMEWORKS` is `owned-spin`, `rtic`, `zephyr`, `esp32`.
  `nros_orchestration_ir::RETIRED_FRAMEWORKS` names `embassy` with a hint, so
  `nros::main!` refuses `framework = "embassy"` naming this issue and the
  `spin_some` shape, and `nros ws check` reports it as
  `retired-board-framework`.
- `book/src/user-guide/embassy-integration.md` now shows the hand-driven shape
  ("Demote, today's API" above) and lists what it lacks (2143, 2144, 2145, 2146,
  2150).
- RFC-0111 and RFC-0112 (D3, D4, D5) record the ruling.

Issue 2155 found one more reason the arm had to go: it called `runtime.take()`
and executor methods no trait bound declared, so it compiled only for a board
whose concrete types happened to carry them. It is closed as a duplicate.

Left for later, under their own issues: the notifier loop (2143 + 2144) and
the setup-only macro (2150). The Embassy recipe and its std-arch CI cell
(RFC-0112 D4) wait on those.
