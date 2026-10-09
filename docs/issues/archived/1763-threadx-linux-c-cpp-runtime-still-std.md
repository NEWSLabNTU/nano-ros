---
id: 1763
title: "threadx-linux C/C++ images still build their nros-c/nros-cpp runtime staticlib with `std`, so `PANIC halt` stays refused there"
status: resolved
type: tech-debt
area: [boards, threadx, cmake]
severity: low
found: 2026-10-09
related: [issue-1759, issue-1742, issue-1778, rfc-0077, phase-370]
resolved_in: "the ThreadX runtime staticlib is alloc on every board (issue 1763)"
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

## Resolution

- **One ThreadX tier.** `nros_feature_set` now emits `alloc platform-threadx`
  for every ThreadX platform token: `threadx`, `threadx_linux` and
  `threadx_riscv64`, host or cross. `platform-threadx` already brings
  `global-allocator` (the byte pool), and `nros-c`'s default `panic-platform`
  ends in `nros_platform_panic`.
- **`rust_eh_personality`** is now a WEAK, empty C definition in the host
  ports (`nros-platform-threadx` and `nros-platform-freertos` `platform.c`,
  `#if defined(__linux__)`). It replaces the strong Rust definition issue 1759
  put in the threadx-linux board crate, and the one this issue first put in
  `nros-c`.
  - **Why not a Rust `cfg`.** "Is libstd in this link" is a fact about the
    final link, and no crate `cfg` can see it. Measured: a strong definition
    in `nros-platform`'s allocator module, gated `not(feature = "std")`,
    collided with libstd's on freertos-posix (`duplicate symbol:
    rust_eh_personality`). There the staticlib has `std`, but `nros-platform`
    does not carry that feature.
  - **How weak fixes it.** A weak definition yields to libstd's strong one
    where libstd is linked, and satisfies the reference where it is not.
  - **Side benefits.** It also stays out of `nros_generated.h`
    (`cbindgen-headers`) and adds no `std` cfg site (`std-census`). The
    weak-symbol allowlist records it as `body:correct`. It is never called,
    because nothing unwinds.
- **The workspace runtime umbrella** (`nros_ws_runtime`) emits `#![no_std]`
  when its runtime has no `std`, not only when cross-compiling. Before this,
  `workspaces/mixed`'s `threadx` image hit E0152: the umbrella root linked
  libstd implicitly, and libstd's `panic_impl` collided with `nros-c`'s
  handler. This was the same stand-in, "cross means no std", one file over.
  Being cross still suffices on its own, so no existing image changes shape.
- **The boot-config env rung survives without `std`.** `$ROS_DOMAIN_ID`,
  `$NROS_LOCATOR`, `$NROS_SESSION_MODE`, `$NROS_NODE_NAME`,
  `$NROS_NODE_NAMESPACE` and `$NROS_RMW` were read through `std::env`
  (`nros::env`, the `env` capability that `std` implies). Dropping `std` from
  the staticlib would have silently stopped a threadx-linux C/C++ entry from
  honouring them. No test here notices, because every rtos_e2e cell bakes its
  locator. The freertos-posix lane (issue 1778) does notice: its C and C++
  cells run in parallel on distinct domains, and with the rung gone both fell
  back to the baked domain and heard each other. The cpp cell failed in 4 of
  6 runs with `the listener's first Received: precedes the talker's first
  Published:`.

  The fix is `nros::host_env`: the same variables, precedence and errors,
  read with the C library's `getenv` on `target_os = "linux"`. `nros-c` and
  `nros-cpp` `resolve_boot` use it when their own `env` is off. The decision
  stays at the edge (issue 0687), and the edge is now "a C library is
  linked" rather than "`std` is linked".
- **The halt refusal** in `nros_apply_panic_policy` is still keyed on `std` in
  the staticlib. It now fires only on the `std` tiers (posix, hosted
  FreeRTOS), where it is still right.
- **Gate.** `check-platform-provider-features` refuses a provider port (one
  whose `nros-c` row names `global-allocator`) that `nros_feature_set` emits
  with `std`, unless the port is listed in `STD_TIER_KNOWN` with its reason.
  The two listed are FreeRTOS (issue 1778) and NuttX (its own std port).

### Evidence (2026-10-10)

- **No Rust `std`.** `nm -C` over 27 threadx-linux images found 0 Rust-std
  symbols (`std::rt`, `lang_start`, `std::panicking`, `std::sys`,
  `std::io::stdio`, …). The 27 are: 12 C and 12 C++ role examples (zenoh and
  Cyclone), plus the workspace c, cpp and mixed `threadx_entry`. Every one has
  `rust_begin_unwind`, `nros_platform_panic`, and `rust_eh_personality` as the port's weak
  `W` (the threadx-linux Rust talker too; the posix std image has libstd's `T`). **Positive control:** a posix (std-tier) `examples/native/c/talker`
  built the same way has 112.
- **Delivery.** `rtos_e2e` with `test(ThreadxLinux)`: 9/9 passed (Rust, C and
  C++ × pubsub 70/70, service and action, against `rmw_zenohd`).
  `test_threadx_linux_c_image_ends_on_one_sigterm`: 2/2 (zenoh and Cyclone),
  so #1741's guard holds on the no_std C image.
- **The env rung on a no_std threadx-linux entry.** I ran
  `workspaces/c`'s `threadx_entry`, whose baked locator is
  `tcp/127.0.0.1:9130`, against an `rmw_zenohd` on 9555. With
  `NROS_LOCATOR=tcp/127.0.0.1:9555` it opened the session and published
  (`[talker_pkg] sent: 0..5`, and its listener `Received:`). Without the
  variable it could not open a session, and `app_main` returned.
- **halt and platform.** A threadx-linux C talker and a C++ talker were each
  built with `nano_ros_add_executable(… PANIC halt)` and with
  `PANIC platform`. All four configure and link, with the staticlib built with
  `panic-halt` or `panic-platform` respectively. In the `platform` images,
  `rust_begin_unwind` calls `core::fmt::write` and then `nros_platform_panic`.
  In the `halt` images it is a single `jmp` to itself (`panic_halt`).
- **Mutation.** I put `std` back on the ThreadX arm (`# MUTATION 1763`) and
  confirmed by grep that it was applied:

  | gate | on the mutated tree | on the clean tree |
  | --- | --- | --- |
  | `check-platform-provider-features` (new) | rc=1, names `platform-threadx` | rc=0 |
  | the same gate from `HEAD` | rc=0: it could not see the hole | — |
