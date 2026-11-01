---
id: 2206
title: "A Cortex-M0+ Zephyr image (rpi_pico) does not build: no thumbv6m arm, two CAS-only atomic sites, no ARMv6-M __atomic libcalls"
status: open
type: bug
severity: medium
area: [zephyr, build, boards]
related: [issue-2176, phase-524]
found: 2026-11-01
---

## Symptom

Phase-524 W1 spike 3, rerun with a Wokwi token
([log](../roadmap/data/phase-524-w1-wokwi-pico-rerun.log)), built
`examples/zephyr/c/talker` for `rpi_pico/rp2040/w` (RP2040, Cortex-M0+). The
image runs in Wokwi and reaches `ros2 topic echo`, but only with three local
patches, in the order the build hit them:

1. `zephyr/cmake/nros_cargo_build.cmake` `nros_detect_rust_target()` has no
   arm for `CONFIG_CPU_CORTEX_M0PLUS` (or `CONFIG_CPU_CORTEX_M0`). The Rust
   lane falls through to the host triple, and cc-rs hands
   `arm-zephyr-eabi-gcc` the flag `-m64`:
   `arm-zephyr-eabi-gcc: error: unrecognized command-line option '-m64'`.
   The spike added `thumbv6m-none-eabi`.
2. `thumbv6m-none-eabi` has no compare-and-swap (`target_has_atomic = "ptr"`
   is false), and two sites use core atomics read-modify-write:
   - `packages/platform/nros-platform-cffi/src/log.rs:122` calls
     `nros_log::log_compat::install_log_crate_bridge`, which is
     `cfg(target_has_atomic = "ptr")` (`error[E0425]`).
   - `packages/api/nros-cpp/src/subscription.rs:73`,
     `SAMPLES_DROPPED_TOO_SMALL.fetch_add` on a `core::sync::atomic::AtomicU32`
     (`error[E0599]`).
   With both patched, the whole Rust lane compiled for thumbv6m. Most of the
   tree already routes CAS through `portable-atomic`; these two do not.
3. The C side: GCC lowers C11 atomic read-modify-write on ARMv6-M to
   libcalls that neither libgcc nor Zephyr provides. Link errors:
   `undefined reference to '__atomic_fetch_add_4'` / `'__atomic_fetch_sub_4'` /
   `'__atomic_compare_exchange_4'`, from zenoh-pico
   `src/collections/atomic.c` and `zpico-sys/c/zpico/zpico.c:1011`. The spike
   linked a shim (`irq_lock` bodies, correct on the single-core build).

## Fix shape

Do all three together (fix the class, CLAUDE.md). Add the thumbv6m arm. Route
the two Rust sites through `portable-atomic`, as the rest of the tree does on
CAS-less targets. Provide the ARMv6-M `__atomic_*_4` libcalls from the Zephyr
module, or build zenoh-pico's atomics on Zephyr's `atomic_*` API. Also add
`thumbv6m-none-eabi` to `examples/zephyr/rust-toolchain.toml`. Acceptance: the
`rpi_pico/rp2040/w` serial talker builds from main with no local patch. Phase-524
W3 (the Pico W row) needs this first.
