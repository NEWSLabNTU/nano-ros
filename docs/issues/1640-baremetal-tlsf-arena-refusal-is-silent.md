---
id: 1640
title: "Three of the four TLSF arenas refuse silently: mps2-an385, stm32f4 and esp32-qemu return NULL with no verdict, no record and no hook — only Zephyr says FRAGMENTED or TOO SMALL"
status: open
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
