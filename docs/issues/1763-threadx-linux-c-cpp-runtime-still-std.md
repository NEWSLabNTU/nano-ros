---
id: 1763
title: "threadx-linux C/C++ images still build their nros-c/nros-cpp runtime staticlib with `std`, so `PANIC halt` stays refused there"
status: open
type: tech-debt
area: [boards, threadx, cmake]
severity: low
found: 2026-10-09
related: [issue-1759, issue-1742, rfc-0077, phase-370]
---

## What was measured

Issue 1759 dropped libstd from every threadx-linux **Rust** image: the entry is
`#![no_std]` + `#![no_main]`, `nros::main!(panic = "halt")` links, and
`panic = "platform"` reaches `nros_platform_panic`.

The C, C++ and mixed images on the same board are a different link. Their Rust
half is the `nros-c` / `nros-cpp` (or `nros_ws_runtime`) staticlib, and
`nros_feature_set` in `cmake/NanoRosFeatureSet.cmake` still gives it
`std platform-threadx` for `threadx_linux` (and for a host `threadx`). So on
those images:

- `std` still supplies the `#[panic_handler]`. `panic-platform`'s handler is
  `cfg(not(feature = "std"))`, so it stands down and never reaches
  `nros_platform_panic`.
- `nros_apply_panic_policy` still refuses `PANIC halt` at configure (issue
  1742), correctly: `panic-halt` next to `std` is rustc E0152. The refusal is
  keyed on `std` in the staticlib's `CORROSION_FEATURES`, not on the board, so
  it no longer applies to any Rust image on this board and still applies to these.
- Rust allocations in the staticlib still go to glibc `malloc`, not the ThreadX
  byte pool.

Measured on 2026-10-09 with 1759's tree: `rtos_e2e` threadx-linux C and C++
pubsub/service/action all pass, and so do the C/C++/mixed workspace entry
builds. Nothing is broken; the staticlib is simply still the `std` tier.

## Direction

Give the threadx-linux staticlib the 1759 treatment: `alloc platform-threadx`
instead of `std platform-threadx`, plus `global-allocator` and a
`rust_eh_personality` provider. That last one is required: phase-370 found that
a host target with `alloc` and no `std` fails to link on `rust_eh_personality`,
and 1759 found the same symbol on the Rust images. The board crate's
`image-runtime` feature is the provider those images use.

**Acceptance:** `PANIC halt` configures and links for a threadx-linux C carrier,
`PANIC platform` reaches `nros_platform_panic`, and the C, C++ and mixed
`rtos_e2e` / `entry_e2e` cells still deliver.

`freertos-posix` is in the same position, in both its Rust and its C/C++
forms (see issue 1759's follow-up).
