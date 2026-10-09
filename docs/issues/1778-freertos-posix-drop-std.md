---
id: 1778
title: "freertos-posix images build their nros-c/nros-cpp runtime staticlib with `std`, so the port's panic ending and heap stand down and `PANIC halt` is refused"
status: open
type: tech-debt
area: [boards, freertos, cmake]
severity: low
found: 2026-10-10
related: [issue-1759, issue-1763, issue-1742, phase-370, rfc-0077]
---

## What was measured

This is the follow-up that issues 1759 and 1763 name. Read on 2026-10-10 at
`ced0b39912`.

- **The board has no Rust image.** `nros-board-freertos-posix` is a descriptor
  plus C (`c/freertos_posix_entry.c`, `c/freertos_posix_hooks.c`,
  `config/FreeRTOSConfig.h`). There is no `board_crate`, no `BOARD_PATHS` key,
  and no `examples/**` Rust leaf names it. Its images are the C and C++
  workspace entries (`workspace-{c,cpp}-freertos-posix`, Cyclone), driven by
  `tests/freertos_posix.rs`. The descriptor's `entry_kind = "hosted-main"` says
  the C `main` in `freertos_posix_entry.c` starts the scheduler. No Rust entry
  emitter reads it for this board.
- **What pulls `std`:** only `nros_feature_set` (`cmake/NanoRosFeatureSet.cmake`).
  The freertos arm emits `std platform-freertos` when `NOT _cross` (phase-370),
  so the `nros-c`/`nros-cpp` staticlib (and a workspace's `nros_ws_runtime`)
  links libstd. That gives the same three consequences 1763 measured on
  threadx-linux:
  - `nros-c`'s `panic-platform` handler is `cfg(not(feature = "std"))`, so it
    stands down. A Rust panic ends in std's handler and never reaches
    `nros_platform_panic`.
  - The allocator is NOT the issue here, unlike on threadx-linux.
    `nros-c/platform-freertos` turns on `nros-platform/global-allocator`, and
    `nros-platform` does not carry the `std` feature the staticlib does. So its
    `#[global_allocator]` (over `nros_platform_alloc`, i.e. `heap_3` and the
    host `malloc`) is already installed beside libstd. This was measured on
    2026-10-10: while issue 1763 was in progress, a strong
    `rust_eh_personality` placed in that allocator module collided with
    libstd's in this exact image.
  - `nros_apply_panic_policy` refuses `PANIC halt` at configure, because
    `panic-halt` next to `std` is rustc E0152 (issue 1742).
- **Why it was `std`:** phase-370's comment records that `alloc` without `std`
  on a HOST target failed to link on `rust_eh_personality`, which the prebuilt
  host `liballoc` references. Since issue 1763, `nros-platform-freertos`'s
  `platform.c` defines that symbol WEAK on `__linux__`. The weak definition
  yields to libstd's where libstd is linked, so the reason is gone.
- **The env rung is not the obstacle either.** Without `std`, the image would
  lose `$ROS_DOMAIN_ID` and friends (`nros::env` is `std::env`), and this lane
  depends on them: its C and C++ cells run in parallel on distinct domains.
  Measured while working on issue 1763, with the tier switched to `alloc` and
  no other change, the cpp cell failed in 4 of 6 parallel runs because the two
  images shared the baked domain. Issue 1763 added `nros::host_env` (the same
  rung, read with `getenv`), so the rung survives.
- `check-platform-provider-features` lists `platform-freertos` in
  `STD_TIER_KNOWN` against this issue. Resolving it removes that row.

## Direction

The same as 1763, with no Rust entry work:

1. In `nros_feature_set`, the freertos arm emits `alloc platform-freertos`
   whether the build is host or cross.
2. Remove `platform-freertos` from `STD_TIER_KNOWN`.
3. Print the heap peak at boot. `heap_3` keeps no high-water mark of its own
   (`xPortGetMinimumEverFreeHeapSize` is a `heap_4`/`heap_5` API). On this
   port `nros_platform_heap_used_bytes` is `mallinfo2().uordblks`, so the board
   samples it and reports a peak.

**Acceptance:**
- The freertos-posix C and C++ ELFs have no Rust `std::` symbols, checked
  against a positive control.
- `tests/freertos_posix.rs` (C and C++, Cyclone) still delivers.
- A C and a C++ image link with both `PANIC halt` and `PANIC platform`, and
  `platform` reaches `nros_platform_panic`.
- The boot prints the heap peak.
