---
id: 504
title: "Node code has no portable way to read a monotonic clock — control loops cannot compute dt without a per-platform extern hack"
status: resolved
resolved_in: "same-day fix; nros::time module"
type: enhancement
area: api
related: [issue-0502, issue-0503]
---

## Resolution (2026-08-11)

New `nros::time` module (`packages/api/nros/src/time.rs`):

- `nros::time::now() -> core::time::Duration`, plus a `now_us()`
  convenience. Monotonic since an unspecified epoch; documented as
  explicitly NOT ROS time (no sim-time, no absolute meaning).
- Clock source mirrors the executor's timer accounting: `std` builds
  use `std::time::Instant` anchored at first use; `no_std` +
  `rmw-cffi` builds read the platform's `nros_platform_clock_us`
  export — the same linkage contract the executor and wake primitives
  already rely on, so no new requirement on any image. A `no_std`
  build without `rmw-cffi` has no clock source and the module is
  compiled out (`#[cfg]`-gated), keeping the error at compile time.
- Resolution inherits issue #502's fix on FreeRTOS Cortex-M
  (sub-tick); ThreadX stays tick-coarse as documented there.

Compile-verified on host (`cargo build -p nros`) and on
`thumbv7m-none-eabi` via a FreeRTOS application image.

## Original problem (condensed)

Node packages are platform-agnostic, so they could reach neither the
per-platform timing types nor the C ABI clock; control loops were
forced to assume the nominal period for `dt` (silently misintegrating
whenever a callback ran late), and users re-invented the platform ABI
one layer up with hand-rolled `extern "C"` clock cratelets per entry
binary.
