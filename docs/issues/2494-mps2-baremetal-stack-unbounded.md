---
id: 2494
title: "mps2-an385 bare metal: the main stack has no size and no guard, so an overflow corrupts RAM silently"
status: open
type: bug
area: boards, memory
related: [phase-553, phase-557, issue-2265, issue-2400]
found: 2026-11-13
---

## What is wrong

On the bare-metal `mps2-an385` board, the main stack has no stated size and nothing stops it from overflowing.

- cortex-m-rt starts the stack at the top of RAM (`0x2040_0000`) and grows it downward. Nothing bounds it.
- `packages/boards/nros-board-mps2-an385/mps2-an385.x` sets `_stack_size = 8K`, `_heap_size` and `_heap_*`, but nothing reads them. The file's own comment says so (issue 2265).
- The image has no `MSPLIM` (the Cortex-M3 has none), no flip-link layout, no painted guard region and no MPU guard.

So a stack that grows past what is free does not fault. It writes over whatever lies below it, and nothing reports it.

## Where it can collide

- **The cargo-rooted image (today).** The heap is a `.bss` static (`nros_platform_mps2_an385::memory::HEAP`, 128 KiB, `NROS_HEAP_SIZE`). The first-project image measures about 180 KB of static RAM in a 4 MiB SRAM (issue 2265), so the stack has about 3.8 MiB of headroom. A collision there needs runaway recursion, so it is unlikely. But if one happens, it corrupts `.bss` with no diagnostic.
- **The C-rooted image (phase-553 W1).** newlib (nano) owns libc on that road, so its `_sbrk` heap grows UP from `end` while the stack grows DOWN from the top of RAM. The two meet with no guard. newlib's `_sbrk` checks the stack only when the port's syscalls do so. That makes a deep stack or a large heap a silent overlap, not a refusal.

## Why the stack-floor check cannot cover it

Issue 2400 W5 (the cargo-leaf stack-floor CHECK) compares a measured floor against the board's STATED stack. This board states none. So W5 prints "not checked: the board states no stack" for this board. It does not pass the image. Until the board states a stack, no floor can be checked against it.

## What a fix needs

1. **A stated stack.** For example, `_stack_size` made real: a region the link script reserves, with `ASSERT`s that `.bss`, the end of the newlib heap, and the Rust heap all stay below it.
2. **A guard.** One of these:
   - a flip-link layout, where the stack sits below `.data`/`.bss` so an overflow faults at the RAM base;
   - a painted canary checked at a known point;
   - for the C-rooted road, an `_sbrk` that refuses to grow past the stack limit.
3. **A knob.** The stated size is something a user can raise. Issue 2400 W5b then CHECKs the walked floor against it.

phase-553 W1 is already deciding the heap layout ("newlib's `_sbrk` region and the Rust allocator's arena must not overlap"). The stack limit belongs in the same link script (`c-runtime.x` on that road).
