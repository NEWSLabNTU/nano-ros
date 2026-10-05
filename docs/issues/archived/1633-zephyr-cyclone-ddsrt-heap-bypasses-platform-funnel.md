---
id: 1633
title: "A Zephyr Cyclone image's ddsrt heap is libc `malloc`, not the nano-ros
  platform funnel the native road routes it through"
status: resolved
type: tech-debt
area: [rmw, zephyr, memory]
severity: low
found: 2026-10-02
related: [832, 881, 1324, 1611]
---

## What

Issue 0832 routed Cyclone's whole `ddsrt_{malloc,calloc,realloc,free}` family
onto `nros_platform_{alloc,realloc,dealloc}` (fork commit `d97a71e2`, file
`src/ddsrt/src/heap/nros/heap.c`), behind `-DNROS_DDSRT_PLATFORM_FUNNEL`. That
define is set in exactly one place, `ProvideCycloneDDS.cmake`, on `ddsc`.

The Zephyr road does not use `ProvideCycloneDDS.cmake`. It compiles Cyclone
itself in `zephyr/cmake/nros_rmw_cyclonedds.cmake`, globbing
`src/ddsrt/src/*/posix/*.c` -- so it builds `heap/posix/heap.c` (libc `malloc`),
never `heap/nros/heap.c`, and never defines the funnel switch. A Zephyr
Cyclone image therefore allocates from picolibc's arena
(`CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`, 16 MiB by `zephyr/Kconfig`'s
`configdefault`), while every Rust and zenoh-pico allocation in the same image
goes to the nros heap (`CONFIG_NROS_ZEPHYR_HEAP_SIZE`, issue 1324). Two heaps,
two budgets, and only one of them is reported by the boot record's heap peak
(issue 1424).

## How it was found

By reading, while resolving issue 1611 (which needed to know whether the Rust
cyclonedds confs could drop their arena too -- they cannot, for this reason).
NOT measured: no Zephyr Cyclone image was built for it. Confirm with `nm` on a
Zephyr Cyclone ELF (`ddsrt_malloc` calling `malloc`, no reference to
`nros_platform_alloc` from ddsrt) before acting.

## What would fix it

Compile `heap/nros/heap.c` and define `NROS_DDSRT_PLATFORM_FUNNEL` on the
Zephyr Cyclone library the way `ProvideCycloneDDS.cmake` does for `ddsc` --
one switch, two producers, so a gate should name both (the 0881 lane covers
only the native one). Then size the nros heap for Cyclone's demand and drop
the 16 MiB libc arena from the Rust and C/C++ cyclonedds confs, measured.

## Resolution

Fixed 2026-10-05 on `fix/1633-zephyr-cyclone-ddsrt-funnel`, based on `main`
after PRs #1618 and #1628 (issue 1653: the board heap reaches the descriptor
and Cyclone's D11 check on the cmake and west roads) had both merged. Neither
touched `zephyr/cmake/nros_rmw_cyclonedds.cmake` or the heap TUs, so nothing
here duplicates them.

**Confirmed first, as the issue asked.** On a native_sim `c/talker` Cyclone
image built from `main`, `ddsrt_malloc_s` disassembled to `jmp malloc`, and
under gdb (counting breakpoints, read at the first `dds_create_writer`):
794 `ddsrt_malloc_s` -> 805 libc `malloc` calls (299,191 B requested), and
**0** `nros_platform_alloc`.

**Fix.**

* `zephyr/cmake/nros_rmw_cyclonedds.cmake` compiles `heap/nros/heap.c` and
  defines `NROS_DDSRT_PLATFORM_FUNNEL` on the two heap TUs only (the posix one
  compiles out; the switch is read nowhere else, so it does not go on the whole
  `nros` library).
* `zephyr/Kconfig`: `NROS_ZEPHYR_HEAP_SIZE` defaults to **1 MiB** with Cyclone
  (it was 64 KiB / 128 KiB Rust, which a Cyclone participant alone overruns);
  the Cyclone `configdefault` for `COMMON_LIBC_MALLOC_ARENA_SIZE` drops
  **16 MiB -> 256 KiB**. The 21 Cyclone confs that stated 16 MiB (PR #1578
  left them "pending this") state nothing now and say why; the book's snippet
  names the right knob.
* Gate `check-ddsrt-funnel-producers` (one switch, two producers, as the issue
  asked): `ProvideCycloneDDS.cmake` and the Zephyr cmake must both define the
  switch in CODE (a comment does not count) and the Zephyr road must compile the
  funnel TU; a producer that disappears fails. Self-test, 5 cases.

**Measured after** (same image, same gdb census):

| image | `ddsrt_malloc_s` | `nros_platform_alloc` | libc `malloc` | nros heap peak (first `dds_write`) |
| --- | --- | --- | --- | --- |
| c/talker, before | `jmp malloc`; 794 | 0 | 805 / 299,191 B | — |
| c/talker, after, old 64 KiB nros heap | `jmp nros_platform_alloc` | — | — | halts: `HEAP EXHAUSTED (TOO SMALL): request 65584 bytes, arena 66048` + the fatal hook |
| c/talker, after, new defaults (1 MiB / 256 KiB) | 836 | 840 / 283,991 B to the writer | 7 / 15,200 B | 153,072 B of 1,049,088 |
| cpp/talker, after, new defaults | `jmp nros_platform_alloc`; 836 | 840 | 7 / 15,200 B | 153,072 B |
| rust/talker, after, new defaults | `jmp nros_platform_alloc`; 836 | 840 | 9 / 15,256 B | 153,072 B |

The halted row is the proof the bytes moved: it is platform.c's exhaustion
path (issue 1370's verdict, the boot record, `CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL`)
reporting a Cyclone allocation, which it could never see before.

**Not measured:** steady-state heap use with discovered peers or traffic. Every
run stops at the first `dds_write` because a native_sim Cyclone image currently
spins on `os_sockWaitsetWait: select failed` and its simulated time stops there
(issue 1674, already filed) — that is also why the census is read at a
breakpoint rather than at `--stop_at`. 1 MiB is ~6.8x the measured peak for that
reason, and the Kconfig help says so. Real hardware (no NSOS) and the action /
service examples were not run; the D11 heap-budget boot check is unchanged (its
budget comes from the board descriptor, which issue 1653 wired).

Sweep: `git grep -n 'NROS_DDSRT_PLATFORM_FUNNEL' -- cmake zephyr packages/rmw`
and `git grep -n 'COMMON_LIBC_MALLOC_ARENA_SIZE=16777216'` (empty).
