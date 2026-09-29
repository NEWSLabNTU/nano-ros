---
id: 1571
title: "The Rust tier road still takes every non-boot tier's executor backing from the heap (Box::leak) — the one executor path issue 1568 did not unify"
status: open
type: tech-debt
area: boards, memory
severity: medium
found: 2026-09-29
related: [issue-1568, issue-1551, issue-1145, issue-1171, phase-392]
---

# Rust tier executors are heap, where every other executor is `.bss`

Issue 1568 made the C and C++ executor storage ONE method on every RTOS board:
the entry emits a `.bss` static at the build's size, the runner takes it
(`_in`), and the linked library refuses a short block. The Rust single
executor was already `.bss` (`nros_node::executor::backing::EXECUTOR_BACKING`,
phase-392 W6). The Rust TIER road is the one path left on the heap.

## Where (surveyed on 1568's branch)

Every Rust `run_tiers` opens the boot tier with `Executor::open` — which takes
`EXECUTOR_BACKING` — and each spawned tier with
`Executor::open_with_session_handle` / `open_with_session`
(`nros-board-{zephyr,freertos,threadx}` `entry*.rs:…open_with_session_handle`,
`nros-board-{linux,nuttx}` `…open_with_session`). Those reach
`from_session_ptr` → `default_backing(ExecutorSizing::DEFAULT)`
(`packages/core/nros-node/src/executor/spin.rs:187`), which tries the one
static and, once the boot tier has taken it, falls through to
`Box::leak(Box::new_uninit_slice(words))`. `backing.rs` says so: "the tiered
boot paths open a second per tier, and those correctly fall through to the
heap".

So a two-tier Rust image has one executor in `.bss` (named by `mem-report`)
and one on the heap (invisible to it, unpriced at link) — the shape 1551 and
1568 removed from C/C++. The size is not a guess (it is `ExecutorSizing`,
type- and const-derived, so nothing is short), but it is the DEFAULT sizing,
not the entry's derived one, and it is not where the other road puts it.

## Fix direction

The sized API already exists: `open_with_session_handle_in(handle, backing,
sizing)`. `nros::main!` knows the tier count and the entry's sizing, so it can
emit `static mut TIER_BACKING: [[MaybeUninit<u64>; W]; N-1]` beside the boot
backing and hand each board's `run_tiers` a slice per tier, exactly as the C
pack hands `__nros_tier_executor_storage`. Touches the proc macro and five
board crates' Rust `run_tiers`; PR #1431 (the Rust derived-tier test lane) is
the lane that would prove it and sits in the same files.

## Acceptance

`mem-report` on a Rust tiered FreeRTOS or Zephyr image names every tier's
executor backing, and the boot heap peak drops by the moved amount.
