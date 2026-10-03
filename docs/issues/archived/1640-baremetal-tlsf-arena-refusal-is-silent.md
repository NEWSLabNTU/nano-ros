---
id: 1640
title: "Three of the four TLSF arenas refuse silently: mps2-an385, stm32f4 and esp32-qemu return NULL with no verdict, no record and no hook — only Zephyr says FRAGMENTED or TOO SMALL"
status: resolved
type: bug
area: [memory, platform, boards]
severity: medium
found: 2026-10-02
related: [issue-1370, issue-1036, issue-1425, phase-391, rfc-0034]
---

## What happens

`zpico_alloc::FreeListHeap` (rlsf TLSF) is the platform arena on four ports.
Issue 1370 made the FRAGMENTED / TOO SMALL verdict the OPERATIVE guard for
external fragmentation — no numeric bound holds for arbitrary traffic, so the
exhaustion report has to say which of the two a refusal was — and issues
1425/1036 made an exhausted heap reach the boot record and the fatal hook.

All of that is on ONE port. `nros-platform-zephyr/src/platform.c`
`nros_platform_alloc` classifies, prints, writes
`nros_boot_report_note_heap_alloc_failed`, and halts through
`CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL`. The other three call the arena and
return what it returns:

- `packages/platform/nros-platform-mps2-an385/src/memory.rs` `alloc()` —
  `HEAP.alloc(size)`;
- `packages/platform/nros-platform-stm32f4/src/memory.rs` `alloc()` — same;
- `packages/platform/nros-platform-esp32-qemu/src/memory.rs` `alloc()` — same.

A NULL from them carries no size, no verdict and no record, so on the boards
where the console is the least likely to be wired the operative guard does not
exist.

## Sweep

`git grep -n 'FreeListHeap<' -- 'packages/platform/**/*.rs'` — four statics;
`git grep -n 'classify_refusal\|free_shape\|note_heap_alloc_failed' -- 'packages/platform'`
— Zephyr only.

## What would fix it

One shared failure path rather than three copies: a `FreeListHeap` method (or a
small helper beside it) that, on a refused request, computes the verdict and
hands `(size, verdict, free_total, largest_free)` to the port's report hook,
which on these boards is `nros_log` at ERROR (now counted into the boot record
by issue 1036's sink) plus `boot_report::note_heap_alloc_failed`. Then the
`*_EXHAUSTION_IS_FATAL` behaviour, which needs a per-board knob.

Not done in issue 1370's resolution: it measured and stated the bound question
and fixed the verdict itself; extending the report to three more boards is a
change to three platform crates none of which this host can run end-to-end in
the time it had (only mps2-an385 has a QEMU lane).

## Resolution

Fixed 2026-10-03 on `fix/1640-baremetal-arena-refusal-reports`.

**One refusal path, as the issue asked.** `nros-baremetal-common::heap`
(`alloc_or_report` / `realloc_or_report`), which all three ports already
depended on, now wraps the arena call. On a refused request it:

1. writes the boot record's `failed_alloc_size` FIRST, through
   `nros_boot_report_note_heap_alloc_failed` (exported by `nros-node` in both
   its enabled and disabled builds);
2. classifies the refusal with issue 1370's verdict (`Exhaustion::classify`
   over the arena's `free_shape`) and emits one line at ERROR through the
   port's own `PlatformLog`, in the Zephyr report's words:
   `HEAP EXHAUSTED (<verdict>): request N bytes, arena N bytes, free N bytes,
   largest free block N bytes -- <remedy>`, naming the knob (or saying a larger
   arena only postpones a FRAGMENTED refusal); rendered into a stack buffer, no
   allocator;
3. halts through the port's `PlatformPanic` when
   `NROS_HEAP_EXHAUSTION_IS_FATAL` is set — default ON exactly when
   `NROS_BOOT_REPORT` is, the rule `nros-node` applies to its arena twin and
   Zephyr's Kconfig to `CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL` (this crate's new
   `build.rs`).

`realloc` reports too: a grow that cannot be served is the same exhaustion.
STM32F4 had no `PlatformPanic`; it gets one in the MPS2 shape (defmt, `bkpt`,
`wfi`), as a trait impl only (no new `nros_platform_panic` export).

**Measured on QEMU mps2-an385** (`examples/mps2-an385-baremetal/rust/talker`,
`[image.mps2-an385-baremetal] env = { NROS_HEAP_SIZE = "8192" }` set for the
measurement and reverted, a host `zenohd` at the image's locator, 25 s):

| build | output |
| --- | --- |
| BEFORE (origin/main `memory.rs`) | `zpico Generic -> ConnectionFailed` / `node declaration failed — PublisherCreationFailed`, repeating; nothing names the heap |
| after | `[ERROR] nros: HEAP EXHAUSTED (TOO SMALL): request 59 bytes, arena 8192 bytes, free 0 bytes, largest free block 0 bytes -- raise NROS_HEAP_SIZE once you know what asked`, then the same downstream errors |
| after, `NROS_HEAP_EXHAUSTION_IS_FATAL = "1"` | the same line, then `nros: PANIC platform heap exhausted (see the HEAP EXHAUSTED line above, and the boot report's failed_alloc_size)` and the board stops |
| after, default heap (control) | reaches the spin loop; no `HEAP EXHAUSTED` line |

The BEFORE row is the issue's point exactly: the refusal surfaced two layers
up as a transport error that names neither the heap nor its size. Unit tests
(`nros-baremetal-common`): the line carries every number and the knob; a
FRAGMENTED refusal says a larger arena only postpones it and fits the 256-byte
buffer whole; the verdict is read off a real `FreeListHeap` (filled -> TOO
SMALL, every other block freed -> FRAGMENTED).

**Not measured:** STM32F4 and ESP32-QEMU on hardware/emulator — both
cross-compile (`thumbv7em-none-eabihf`, `riscv32imc-unknown-none-elf`) and go
through the same function MPS2 was measured through; the boot record's
contents after a halt were not read back from RAM (the write is the one Zephyr
already makes, through the same symbol). The FRAGMENTED arm was exercised on
the host arena, not on a board.

Sweep: `git grep -n 'FreeListHeap<' -- 'packages/platform/**/*.rs'` — three
statics now report through `heap::*_or_report`; the fourth,
`nros-platform/src/zephyr_heap.rs`, is reached through Zephyr's C funnel, which
reported already.
