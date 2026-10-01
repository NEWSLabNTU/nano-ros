---
id: 1606
title: "`mem-report` summed RAM sections by NAME, so a Zephyr image's `.noinit.*` RAM was missing and the unattributed gap went NEGATIVE (-76.1 %)"
status: resolved
type: bug
area: tooling
related: [1147, 1180, 0815, phase-392]
resolved: 2026-10-01
---

## Problem

`scripts/nros-mem-report.py` computed "RAM by section" as the sum of sections
whose NAME starts with `.bss`, `.data`, `.sbss` or `.sdata` (from `size -A`).
A Zephyr image keeps much of its RAM elsewhere: one
`.noinit."<source file>".N` section per file (the kernel heap
`kheap__system_heap`, every `K_THREAD_STACK_DEFINE` — `nros_tier_stacks`,
`nros_thread_stacks`, `z_main_stack`), `*_area` iterable sections, and `.got`
on native_sim.

Measured on the derived-tiers C++ `native_sim/native/64` image
(`build-ws-cpp-derived-tiers-entry-zenoh/zephyr/zephyr.exe`):

```
RAM (.bss + .data), by section:  355,824 bytes
RAM attributed to symbols:       626,641 bytes
unattributed (padding, linker reservations, symbol-less data): -270,817 bytes (-76.1%)
```

A negative "unattributed" figure is impossible, so a reader rightly concludes
the tool is broken and stops trusting the rest of the report — on exactly the
platform the static-memory campaign is about.

Found while working issues 1147/1180 (the attribution half of the same report).

## Resolution

RAM sections are now selected by FLAG: every section `readelf -S -W` marks
ALLOCATED and WRITABLE (`A` + `W`), whatever it is called
(`ram_section_total`). Same image, after:

```
RAM (writable allocated sections): 650,740 bytes
RAM attributed to symbols:         626,641 bytes
unattributed (padding, linker reservations, symbol-less data): 24,099 bytes (3.7%)
```

Self-tested (`selftest_sections`) on a synthetic section table that includes a
`.noinit."x.c".0` NOBITS section, a read-only `.rodata`, and non-allocated
`.comment`/`.symtab`; replacing the flag rule with the old name filter fails it
(mutation run during the fix).

Not measured: an image whose linker script places RAM in a section the toolchain
marks without `W` (none seen on the three images measured).
