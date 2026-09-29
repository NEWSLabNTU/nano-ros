---
id: 1571
title: "The Rust tier road still takes every non-boot tier's executor backing from the heap (Box::leak) — the one executor path issue 1568 did not unify"
status: resolved
type: tech-debt
area: boards, memory
severity: medium
found: 2026-09-29
resolved: 2026-09-29
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

## Resolution

Fixed as the fix direction said, with the C/C++ METHOD and the boot
reservation's own size constant (no second spelling):

- **nros-node** (`executor::backing`): `TierExecutorBackingSlot` =
  `[MaybeUninit<u64>; EXECUTOR_BACKING_DEFAULT_U64S]` (exactly what an
  `ExecutorSizing::DEFAULT` executor carves — exact by type);
  `TierExecutorBacking<N>` (N slots + a once-only latch, `const fn new`,
  `take(&'static self)`; a second take is EMPTY, never an alias);
  `check_tier_executor_backing` / `TierBackingShort` (the one refusal);
  `Executor::open_with_session{,_handle}_slot` (slot + `DEFAULT` sizing paired
  in one place). Re-exported from `nros`.
- **`nros::main!`**: both multi-tier arms emit
  `static __NROS_TIER_EXECUTOR_BACKING: TierExecutorBacking<{tiers-1}>` and pass
  `.take()` to `run_tiers` — the entry owns and sizes it, as the C pack owns
  `__nros_tier_executor_storage`.
- **Every Rust `run_tiers`** (zephyr, freertos, threadx, nuttx, linux, and the
  forwarding wrappers mps2-an385-freertos / threadx-linux /
  threadx-qemu-riscv64 / nuttx-qemu) takes the slots, refuses a short block
  before anything opens, and opens each spawned tier over its slot. No board
  calls the heap twins any more
  (`git grep -n 'open_with_session(\|open_with_session_handle(' -- packages/boards`
  is empty).

**Where the heap arm still lives:** `default_backing`'s `Box::leak` is reached
only by an ad-hoc second `Executor::open` and an entry sized past the
reservation. The hosted C/C++ runner `nros_board_native_run_tiers{,_ns}`
(nros-cpp) is a separate road, not touched here (hosted: no fixed budget).
The FreeRTOS heap default still carries `DEFAULT_HEAP_SPARE_EXECUTOR_BYTES`
(2 x 131,072) for spare executors; with both tier roads in `.bss` its measured
basis is stale — re-measure before lowering it (its doc now says so).

### Measured

FreeRTOS mps2-an385, `orch_tiers_freertos` (2 tiers), release, before = the
parent commit:

| | before (heap) | after (`.bss`) |
| --- | ---: | ---: |
| `mem-report` `demo_entry::__nros_entry_run::__NROS_TIER_EXECUTOR_BACKING` | absent | 21,472 (slot 21,464 + 8 latch) |
| RAM `.bss + .data` | 1,130,612 | 1,152,084 (+21,472) |
| boot `nros: heap peak` (heap_4, 2 runs each) | 253,552 | 232,104 (−21,448) |

Zephyr native_sim, `realtime-rust/src/derived_bringup` (the #1537 derived-tier
lane): `__NROS_TIER_EXECUTOR_BACKING` absent → **88,560**; RAM 545,592 →
634,184. gdb on `sys_heap_aligned_alloc` > 32 KiB during boot: before, one
**88,552**-byte `malloc` from picolibc `z_malloc_heap` whose backtrace is
`default_backing <- from_session_ptr <- open_with_session <-
open_with_session_handle`; after, none. A MOVE, not a saving.

Boots: both tiers dispatch on the FreeRTOS `orch_tiers_freertos` and
`realtime-rust` (`freertos_realtime`) images, the Zephyr derived image
(`DerivedTierBelowTransport` passes before and after), native
(`native_orchestration_tiers` 4/4, `realtime_tiers` native/rust) and NuttX-arm
(`realtime_tiers` nuttx-arm/rust delivers on both tiers; its 3x ratio assertion
is issue 0736).

