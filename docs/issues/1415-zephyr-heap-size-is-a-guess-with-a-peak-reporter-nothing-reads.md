---
id: 1415
title: "CONFIG_NROS_ZEPHYR_HEAP_SIZE is set by hand; the high-water reporter
  is compiled on every Zephyr image and nothing reads it off a board or gates
  the knob against it"
status: open
type: enhancement
area: [zephyr, memory, sizing]
severity: medium
found: 2026-09-21
related: [issue-1036, issue-1145, issue-0900, phase-412, phase-460]
---

## What the island states

`src/zephyr_entry/boards/mr_canhubk3_s32k344.conf:266` on the Autoware Safety
Island: `CONFIG_NROS_ZEPHYR_HEAP_SIZE=94208`, annotated in the file as a
guess. The configure-time gate at `zephyr/cmake/nros_cargo_build.cmake:911-948`
checks `NROS_EXECUTOR_ARENA_SIZE + 24576 <= NROS_ZEPHYR_HEAP_SIZE`, where
24576 is "18352 measured, rounded up to 24 KiB" - a constant measured once on
one image. The heap is the largest RAM item the map attributes to a knob after
the service inbox table (95,928 B on the island).

## What exists, and what the brief got wrong (verified at 783cdfa14)

The brief that opened this worried the counter "may not be compiled and may
be cumulative rather than high-water". Both are refuted by the source:

* `nros_zephyr_heap_peak()` (`packages/platform/nros-platform/src/zephyr_heap.rs:103`)
  returns `HEAP.peak()`, a `fetch_max` of outstanding bytes
  (`packages/rmw/zenoh/zpico-alloc/src/lib.rs:264`), charged at the rlsf
  USABLE block size (`:321-330`, phase-412), so it is a true high-water mark
  of what the arena had handed out.
* The `stats` feature that compiles it is NOT optional on Zephyr:
  `packages/platform/nros-platform/Cargo.toml:113`,
  `platform-zephyr = [..., "zpico-alloc/stats"]`.
* The comment at `packages/platform/nros-platform-zephyr/src/platform.c:256`
  says the reporter "requires nros-platform's `heap-stats` feature"; no such
  feature exists (`alloc-stats` is the Rust-allocator counter, `zpico-alloc/stats`
  the arena's). Stale, and it is what the brief read.

What is true: `nros_zephyr_platform_heap_peak_bytes()` (`platform.c:273`) is
reachable, and on a board with no console (the MR-CANHUBK344 has no wired
console UART; the second UART carries the zenoh serial transport) nothing
reads it. The boot report (`CONFIG_NROS_BOOT_REPORT`, `zephyr/Kconfig:1325`,
a 60-byte RAM record for exactly this board class) carries stages and the
failed arena allocation, not the heap peak. And nothing, anywhere, compares
the knob to a measured peak; the 24576 constant is the only headroom
argument.

## What would fix it

phase-460 W5.

1. The boot report gains `heap_peak_bytes` and `heap_capacity_bytes`, written
   at every stage transition and on the exhaustion path.
2. `scripts/read-boot-report.py` prints them, and a recipe beside
   `mem-report` in `just/check/tools.just` reads a dump and refuses when
   `capacity - peak < 24576` or when the peak is 0 (never sampled).
3. A board `.conf` that sets `CONFIG_NROS_ZEPHYR_HEAP_SIZE` records the dump it
   was set from as a comment; the island's `.conf` gets that comment when the
   island runs the measurement.
4. `platform.c:256` names the real feature.

The board run is the island's; issue 1036 records why no nano-ros lane can
perform it.

## Acceptance

`read-boot-report.py` on a dump from a native_sim image with the report on
prints a non-zero peak below capacity; the recipe refuses a synthetic dump
with peak 0 and one with headroom below 24576.
