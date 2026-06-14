---
id: 63
title: native-rust cyclonedds binaries drop the posix platform C port — undefined nros_platform_wake_*
status: resolved
type: bug
area: build
related: [issue-0062, phase-241, phase-248, phase-249]
resolved_in: de85cadc2
---

## Symptom

Native **Rust** cyclonedds examples (`examples/native/rust/*` built with
`--features rmw-cyclonedds`) fail to link:

```
rust-lld: error: undefined symbol: nros_platform_wake_storage_size
rust-lld: error: undefined symbol: nros_platform_wake_storage_align
rust-lld: error: undefined symbol: nros_platform_wake_init
rust-lld: error: undefined symbol: nros_platform_wake_drop
rust-lld: error: undefined symbol: nros_platform_wake_wait_ms
rust-lld: error: undefined symbol: nros_platform_wake_signal
  >>> nros_node::executor::spin::nros_rmw_runtime_wake_cb in libnros_node-*.rlib
```

The zenoh and xrce native-Rust paths link fine; only cyclonedds was affected.

## Root cause

`nros_node` references `nros_platform_wake_*`, which are provided by
`libnros_platform_posix.a` (compiled by `nros-platform-cffi[posix-c-port]`'s
build script). That archive is dragged into the link only because
`nros-platform`'s `__FORCE_LINK_CFFI` `#[used]` static references the cffi
crate's force-link symbol. But a `#[used]` static buried in a **dependency
rlib** is dead-code-eliminated from a binary root unless something on the root
re-anchors it (single-runtime / phase-241 W-series behaviour).

`nros-rmw-zenoh` re-anchors it (`src/lib.rs::__FORCE_LINK_PLATFORM_CFFI`, gated
on its `platform-posix` feature). The cyclonedds register path
(`nros-rmw-cyclonedds-sys` → `nros-rmw-cyclonedds`) has **no `nros-platform`
dep at all**, so nothing re-anchored the cffi rlib → the posix C port was
DCE'd → the wake symbols went undefined. Pre-existing on `main`; not introduced
by the phase-249 work it surfaced during.

## Fix

Mirror the zenoh anchor:

- `nros-rmw-cyclonedds-sys` gains a native-only `platform-posix` feature
  (`dep:nros-platform` + `nros-platform/platform-posix`) and a
  `#[used] __FORCE_LINK_PLATFORM_CFFI` static gated on it.
- The 13 `examples/native/rust/*` cyclone examples enable that feature on
  their `nros-rmw-cyclonedds-sys` dep (same shape as the zenoh examples, which
  already pass `platform-posix`).

Embedded cyclone consumers (Zephyr/ThreadX) keep the feature OFF and source the
wake symbols from their own platform port.

## Resolution

Resolved 2026-06-14. Native cyclone Rust talker links clean
(`cargo build --no-default-features --features rmw-cyclonedds`).
