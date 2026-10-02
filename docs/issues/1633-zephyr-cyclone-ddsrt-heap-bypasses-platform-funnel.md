---
id: 1633
title: "A Zephyr Cyclone image's ddsrt heap is libc `malloc`, not the nano-ros
  platform funnel the native road routes it through"
status: open
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
