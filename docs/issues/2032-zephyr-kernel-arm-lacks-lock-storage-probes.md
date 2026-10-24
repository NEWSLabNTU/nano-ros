---
id: 2032
title: "Zephyr's non-POSIX platform arm defines no lock storage probes, so the native level runner drags the POSIX port into every Zephyr XRCE C++ image"
status: open
type: bug
area: [zephyr, platform, build, ci]
severity: medium
found: 2026-10-24
related: [phase-513, phase-466, phase-509, 1776, 1779, 0566, 2025]
---

**Home: [phase-513](../roadmap/phase-513-nightly-reds-with-no-home.md)** — R2.

## Measured

- **Runs:** nightly 40065220667 (schedule, 05:10 UTC, head `3c99fb35f`) and
  nightly 40073092367 (07:10 UTC, head `6750f3914`). Both fail in
  `tier 2 nightly (pairwise cover)`, step `just build tier2-nightly`, family
  `zephyr`. The previous night (39916704136, head `4ddf7c744`) built these
  images.
- **Images:** every Zephyr 3.7 `native_sim/native/64` C++ XRCE leaf —
  `build-cpp-{talker,listener,service-client,service-server,action-client,action-server}-xrce`.
  Every zenoh and Cyclone C++ leaf links. In these two nightlies the C XRCE
  leaves link as well. The scheduled `run-matrix` run 40070127626 (head
  `62b661ac1c`) shows the same collision in C XRCE leaves:
  `build-c-{listener,talker,service-client}-xrce` in tier 2 (job 120801569218)
  and `build-c-service-server-xrce` in tier 1 (job 120804235709). Both images
  link `libnros_cpp.a`, so the reach is "Zephyr native_sim XRCE images", not
  only C++. That run was filed separately as issue 2025, now retired as a
  duplicate of this one.
- **Failure:** 56 duplicate symbols at the final link, every non-weak function
  of the POSIX port's `platform.c`, for example:

```
nros-rust/x86_64-unknown-linux-gnu/nros-relwithdebinfo/libnros_cpp.a(10fc8707dfe1bebe-platform.o): in function `nros_platform_clock_ns':
packages/platform/nros-platform-posix/src/platform.c:41: multiple definition of `nros_platform_clock_ns'; modules/nros/libnros.a(platform.c.obj):packages/platform/nros-platform-zephyr/src/platform.c:55: first defined here
```

The logs are in that run's `tier2-nightly-junit` artifact under
`build/zephyr-fixtures/build-cpp-*-xrce.log`.

## Cause

Two facts that were each harmless meet here.

1. `nros-rmw-xrce-cffi/build.rs` compiles the POSIX port
   (`nros-platform-posix/src/{platform,net,timer,termination}.c`) into its own
   archive whenever `CARGO_CFG_TARGET_OS` is a POSIX host. A Zephyr
   `native_sim` build targets `x86_64-unknown-linux-gnu`, so the member is in
   `libnros_cpp.a`. Its comment says the member is demand-driven, so it is never
   pulled while the image's own port defines every symbol it carries.
2. The Zephyr port's **kernel arm** (`#ifndef CONFIG_POSIX_API`, the arm issue
   0566 moved onto `k_mutex` / `k_condvar` / `k_thread`) leaves eight functions
   of `<nros/platform.h>` undefined. Only the `CONFIG_POSIX_API` arm defines
   them:

   - `nros_platform_mutex_storage_size` / `_align`
   - `nros_platform_mutex_rec_storage_size` / `_align`
   - `nros_platform_condvar_storage_size` / `_align`
   - `nros_platform_condvar_signal_from_isr`
   - `nros_platform_task_attr_init`

   The XRCE confs (`examples/zephyr/*/*/prj-xrce.conf`) do not set
   `CONFIG_POSIX_API`; the zenoh and Cyclone confs do.

`97abd95450` (phase-509 P10, "the native level runner") added
`nros_board_native_run_levels_in` to `nros-cpp`, gated on
`rmw-cffi` + `env` + `unix`. A Zephyr `native_sim` C++ image has all three. The
runner calls `nros_platform_mutex_rec_storage_size`, the kernel arm does not
define it, so the linker pulls the POSIX `platform.o` from the XRCE archive to
satisfy it, and every other symbol in that member then collides with
`libnros.a`. This is issue 1776's shape (a POSIX member dragged in by one
symbol the image's port lacks), reached through a missing ABI function instead
of the termination guard.

## What it is NOT

- Not the runner's gate. Every sibling native runner in `nros-cpp`
  (`nros_board_native_run_tiers`, `_run_components*`) has the same gate and
  already sits in these archives; they reference only symbols the kernel arm
  defines. Narrowing this one gate would leave the kernel arm short of the ABI
  for the next caller.
- Not issue 2010 (the mixed entry's missing `#[panic_handler]`), which fails in
  the same family on the same nights.

## Fix

Define the eight functions in the kernel arm. The arm stores a pointer to a
heap-allocated kernel object in the caller's storage (`NROS_Z_HANDLE`), so the
storage probes answer `sizeof(void *)` / `_Alignof(void *)`;
`condvar_signal_from_isr` signals the handle's `k_condvar`; `task_attr_init`
fills the same defaults as the POSIX arm.

## What would close it

A tier-2 nightly in which the six `build-cpp-*-xrce` Zephyr leaves link, and a
scheduled `run-matrix` in which the `build-c-*-xrce` leaves of tier 1 and
tier 2 link.
