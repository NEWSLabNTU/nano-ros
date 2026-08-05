---
id: 420
title: "The nros_log facade is a silent no-op on ThreadX and NuttX, and on FreeRTOS's Rust path"
status: open
type: bug
area: platform
related: [phase-338, rfc-0069]
---

## Symptom

`nros_log::init(nros_log::sinks::default())` followed by `nros_info!(...)`
produces **no output** on ThreadX (both `threadx-linux` and
`qemu-riscv64-threadx`) and on NuttX, and on FreeRTOS when the image is built
through the pure-Rust board entry rather than the C entry.

Nothing errors. Nothing warns. The record is constructed, dispatched, and
dropped.

Nothing has noticed because no shipped body uses the facade on those platforms:
the group-A example bodies all use `log::info!`, which those boards *do* bridge
(`install_uart_logger` on ThreadX, `install_stdout_logger` on NuttX). The trap is
armed for whoever writes `nros_info!` there first.

## Cause

`sinks::default()` is `PlatformSink`, which forwards every record to the
`nros_platform_log_write` C ABI. Where that symbol comes from, surveyed
2026-08-05:

| platform | `nros_platform_log_write` | status |
|---|---|---|
| posix | strong def, `nros-platform-posix/src/platform.c:614` | works |
| zephyr | strong def, `platform.c:510` | works |
| esp-idf | strong def, `platform.c:520` | works |
| mps2-an385, stm32f4, esp32-qemu, esp32s3 | `nros_platform_export_log!` (Rust macro, `nros-platform-cffi/src/lib.rs:893`) | works |
| **freertos** | strong def at `platform.c:727`, but it dispatches through a **fn-ptr slot** that is NULL until `nros_platform_register_log_writer` runs | **partial** |
| **threadx** | same NULL-slot shape, `platform.c:573` | **broken** |
| **nuttx** | **no definition at all** | **broken** |

Two distinct faults:

**1. The fn-ptr slot is never filled (ThreadX, FreeRTOS-via-Rust).** The only
caller of `nros_platform_register_log_writer` in the tree is
`packages/boards/nros-board-freertos/c/freertos_c_entry.c:212` — the C/C++ app
path. ThreadX has **no caller anywhere**, on either path. So
`nros_platform_log_write` runs, reads a NULL writer, and returns.

**2. NuttX has no implementation.** `nros-platform-nuttx` defines neither the
symbol nor a `platform.c`. It resolves instead to the weak no-op in
`packages/api/nros-c/c-stubs/weak_platform_log_stubs.c` — which exists for a
good reason (workspace metadata builds link `nros-c` with no platform crate
selected) but here silently swallows a real platform's logs.

Both faults share a shape: the ABI is satisfied, so the linker is happy, and the
failure moves from link time to runtime silence.

## Why it matters beyond the trap

phase-338 W7 plans to move the example bodies from `log::info!` to
`nros_info!` so the logging facade stops being a board property leaking into
user source. Doing that before this is fixed would turn **every ThreadX and
NuttX e2e marker into silence**, and because the harness greps for markers, each
would surface as a *timeout*, not an error — the most expensive possible failure
mode to diagnose. W7.a is therefore blocked on this, and phase-338 records it.

## Fix

1. **NuttX** — give it a `nros_platform_log_write`. Either a `platform.c` with
   the fn-ptr-slot shape (matching FreeRTOS/ThreadX) or
   `nros_platform_export_log!` against a `PlatformLog` impl (matching
   mps2/stm32f4). Pick one and record why; the split between the two idioms is
   itself worth a note.
2. **Register the writer from the Rust board entries** on ThreadX and FreeRTOS,
   not only from `freertos_c_entry.c`. This is the same
   "fixed one of N sites" class CLAUDE.md warns about — the C path got it, the
   Rust path did not.
3. **Make the weak stub loud, or narrow it.** A weak no-op that a *real*
   platform can silently fall through to is the mechanism that hid this. Options:
   scope the stub to the metadata-build configuration that needs it; or have it
   emit once on first use so a fallthrough is visible in the log it failed to
   write to.

## Verification

A per-platform assertion that a record emitted through `nros_log` reaches the
same transport `log::info!` does. `logging_smoke.rs` already does this for
mps2-baremetal and zephyr; the gap is that ThreadX, NuttX and FreeRTOS have no
equivalent, which is why the slot could stay NULL indefinitely without a red
test. Add those cells before fixing, so the fix is proven rather than assumed.
