---
id: 1611
title: "Six Zephyr Rust example confs still size a ~940 KiB picolibc arena for Rust
  allocations, and check-executor-backing-arena-pairing still pairs it, after
  issue 1324 moved Rust alloc to the nros platform heap"
status: open
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
