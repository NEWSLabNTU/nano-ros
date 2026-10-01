---
id: 1424
title: "CONFIG_NROS_ZEPHYR_HEAP_SIZE is set by hand; the high-water reporter
  is compiled on every Zephyr image and nothing reads it off a board or gates
  the knob against it"
status: resolved
type: enhancement
area: [zephyr, memory, sizing]
severity: medium
found: 2026-09-21
related: [issue-1324, issue-1425, issue-1036, issue-1145, issue-0900, phase-412, phase-460]
resolved_in: "branch fix/zephyr-heap-1424-1425-1498-1324"
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

## Resolution

The knob is now CHECKED against a measurement on every native_sim e2e run, and
the acceptance's native_sim dump is taken. Branch
`fix/zephyr-heap-1424-1425-1498-1324`.

**Why it is checked rather than derived.** The heap's demand is not a build-time
fact: zenoh-pico allocates per wire event (fragment reassembly, peer interest
and liveliness declarations, reply slots) and Rust `alloc` per application
code path, so the entity inventory can count the entities and still not price
what they will be asked to carry. A derived number would be a model of the
runtime that the first new peer falsifies, silently. What the build CAN state
is a size and a floor; what the image CAN state is how far it got. The gate
compares the two, loudly, on the measurement.

**What changed:**

* `CONFIG_NROS_BOOT_REPORT` defaults ON on native_sim (`default y if
  ARCH_POSIX`). That also turns `CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL` on there
  (issue 1425), so an exhausted heap stops a cell.
* `nros_tests::zephyr::ZephyrProcess::heap_headroom()` reads the record out of
  the LIVE image (`/proc/<pid>/mem` at the ELF's `NROS_BOOT_REPORT` symbol; the
  test is the image's parent, which Yama `ptrace_scope=1` allows) and hands the
  dump to `read-boot-report.py --heap-headroom` -- the same gate and the same
  24,576-byte floor a board dump goes through, so the rule keeps one spelling.
  `example_e2e` reads both images' verdicts after the workload succeeded and
  before the kill, and scores them after the workload's own assertion, so a
  delivery failure keeps its headline.
* `nros-node`'s build script read `NROS_BOOT_REPORT` with `env::var` alone,
  which a pure-Rust Zephyr image never receives (zephyr-lang-rust builds that
  cargo command and passes only `DOTCONFIG` -- issue 0460's class): on a Rust
  image `CONFIG_NROS_BOOT_REPORT=y` compiled the C writes against empty Rust
  bodies and the record did not exist. It now reads the knob ladder.

**The acceptance, measured** (`read-boot-report.py` on a dump read out of a
running native_sim image, Zephyr 3.7, 8 s against `rmw_zenohd`):

    c/talker zenoh:  stage 6 FirstSpin; platform heap PEAK 15152 bytes (22.9%)
                     capacity 66048; HEAP HEADROOM: ok -- 50896 bytes spare

The recipe half (`--heap-headroom` refuses peak 0 and headroom < 24,576) was
phase-460 W5's and still holds (`read-boot-report --self-test: OK`).

**The gate in the real test** (`cargo nextest ... example_e2e`, fixtures built
into `NROS_ZEPHYR_BUILD_ROOT`):

    zenoh_c_pubsub_e2e     PASS  listener 14,880 / talker 15,504 of 66,048
    zenoh_rust_pubsub_e2e  PASS  listener 42,672 / talker 43,760 of 131,584

and its negative control -- the same Rust listener rebuilt with
`CONFIG_NROS_ZEPHYR_HEAP_SIZE=65536` -- FAILS the cell (delivery had succeeded):

    [zenoh/rust/Pubsub] the platform heap gate refused -- ...
    listener: HEAP HEADROOM: REFUSED -- 23376 bytes, floor is 24576.
      peak 42672 of 66048 bytes. ...
      set CONFIG_NROS_ZEPHYR_HEAP_SIZE >= 67248

It found a real one on its first use: issue 1324's one-heap change put Rust
`alloc` into this arena, and the gate refused the 64 KiB default for every Rust
image measured (worst rust/action-server, 7,760 spare), which is why a RUST
image now defaults to 131072.

**Not done / not measured:** a real board (still the island's, issue 1036); the
workspace-entry and QEMU cells (the gate is native_sim-only -- an emulator's
record lives in guest RAM; `heap_headroom` says so rather than passing); cells
other than the two pubsub cells above were not run with the gate (the other 25
`example_e2e` cells and the four C/C++ cells share the same code path).
