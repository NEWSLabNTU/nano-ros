---
id: 1611
title: "Six Zephyr Rust example confs still size a ~940 KiB picolibc arena for Rust
  allocations, and check-executor-backing-arena-pairing still pairs it, after
  issue 1324 moved Rust alloc to the nros platform heap"
status: resolved
type: tech-debt
area: [zephyr, memory, sizing]
severity: low
found: 2026-10-01
related: [1324, 1145, 1171, 1424]
---

## What

Issue 1324 made every Zephyr image's Rust `#[global_allocator]` the
`nros_platform_alloc` arena (`CONFIG_NROS_ZEPHYR_HEAP_SIZE`) and refused
zephyr-lang-rust's `CONFIG_RUST_ALLOC`. Before that, a Rust leaf's `alloc` went
to picolibc `malloc`, so the six `examples/zephyr/rust/*/prj-zenoh.conf` set

    # nros-arena-base: 1048576
    CONFIG_NROS_EXECUTOR_BACKING_U64S=11069
    CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=960024

and `check-executor-backing-arena-pairing` holds those three numbers together on
the premise that the libc arena is the one the Rust executor's heap demand used
to come out of (issues 1145/1171).

After 1324 nothing Rust allocates from that arena. What remains in it is
Zephyr's own libc `malloc` users (the NSOS offloaded-socket driver), which the
C talker runs at the 16 KiB native_sim default. So each of these confs reserves
roughly 940 KiB that no allocation is sized from, and the gate keeps the
reservation "correct" against a premise that no longer holds.

## Not changed in 1324, and why

Retiring the lines re-prices every Zephyr Rust image (native_sim, mps2-an385,
and the gate's arithmetic) in one move, and 1324's acceptance was about the
heap Rust DOES use. Measure the NSOS driver's libc demand on a Rust image (the
boot record does not see it -- it is not `nros_platform_alloc`), then drop the
lines to that floor and retire or re-premise the gate in the same change.

## Reproduce

    git grep -n 'COMMON_LIBC_MALLOC_ARENA_SIZE\|nros-arena-base' -- examples/zephyr/rust
    nm build/zephyr/zephyr.exe | grep ZephyrAllocator   # empty since 1324

## Resolution

The arena half of the pairing is retired on Zephyr; the confs reserve nothing
for Rust in picolibc's arena, and the gate refuses the marker that claimed
otherwise.

**Measured: nothing Rust-side uses the libc arena.**

- native_sim `rust/talker` (zenoh, Zephyr 3.7, against a live `rmw_zenohd`):
  gdb on Zephyr's common-libc `malloc`/`free` (`malloc_trace.py`-style Python
  breakpoints, a FinishBreakpoint per call for live/peak bytes) over a 6 s
  `--stop_at` run that published 3 messages: **0 `malloc` calls**. Positive
  control, the same mechanism on `k_malloc` (NSOS allocates per socket from the
  KERNEL heap, not this arena): 2 calls. The image's only static caller of
  `malloc` is `strdup` from NSOS `addrinfo_from_nsos_mid` (a canonical name,
  absent for the numeric locators these images use), freed by
  `nsos_freeaddrinfo`.
- mps2_an385 `rust/talker`: **no static caller** of `malloc`, `calloc`,
  `realloc`, `aligned_alloc`, `strdup` or `free` in the ELF at all (no NSOS on a
  real-network board).

**Before/after** (the six `prj-zenoh.conf`, which drop the marker, the stated
backing and the arena line):

| image | arena before | arena after | `.bss` before | `.bss` after | runs |
| --- | --- | --- | --- | --- | --- |
| native_sim rust/talker | 960,024 B | 16,384 B (Kconfig default) | 1,800,752 | 857,112 | publishes, 0 malloc |
| mps2_an385 rust/talker | 960,024 B static | `-1` (the RAM left after `.bss`, nothing reserved) | -- | 1,364,920 | publishes under QEMU |

`EXECUTOR_BACKING` is the derived default now: 88,552 B on native_sim (the same
as the stated 11069 words) and 87,512 B on mps2_an385 (1,040 B less than the
stated size it used to carry there).

The six `prj-cyclonedds.conf` drop the marker and stated backing too. Their
arena is Cyclone's own -- Zephyr compiles ddsrt's `heap/posix` (libc `malloc`),
not the nano-ros funnel (`NROS_DDSRT_PLATFORM_FUNNEL` is set only by
`ProvideCycloneDDS.cmake`, which the Zephyr road does not use) -- so it returns
to the 16 MiB its C/C++ siblings state and `zephyr/Kconfig`'s
`configdefault` already supplies.

**The gate** (`check-executor-backing-arena-pairing`): the `zephyr` port is
recorded `none` with the measurement as its reason; the one conf rule left is
that the retired `# nros-arena-base:` marker is refused (self-tested). The heap
Rust DOES use (`CONFIG_NROS_ZEPHYR_HEAP_SIZE`) is not paired by arithmetic
because the `.bss` backing never comes out of it; issue 1424's gate scores it
against a measured boot peak. ThreadX's rung pairing and the claim half (issue
1284) are unchanged; the remaining claims are the two ThreadX boards.

Docs moved with it: `zephyr/Kconfig` help for `NROS_EXECUTOR_BACKING_U64S`,
RFC-0002 §4.4b, `executor/backing.rs`, `nros-node/build.rs`, the claims test,
AGENTS.md (whose "Zephyr Rust allocator is picolibc `malloc`" line predated
both phase-391 W3 and 1324) and CLAUDE.md.

Sweep: `git grep -n 'nros-arena-base\|BACKING_U64S' -- '*.conf'` (empty).

Not measured: the cyclonedds images (not built; their arena demand is Cyclone's
and unchanged by this), and the action/service Rust zenoh images individually --
they share the talker's allocator path, and none calls libc `malloc` from Rust.
