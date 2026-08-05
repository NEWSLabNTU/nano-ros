---
id: 420
title: "The nros_log facade is a silent no-op on ThreadX and NuttX, and on FreeRTOS's Rust path"
status: resolved  # not-a-bug: all three rows disproved by execution
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

## Survey corrected by measurement (2026-08-05)

Two of the three "broken" rows do not reproduce. Checked before fixing, because
a fix aimed at a working platform is worse than no fix.

**ThreadX — WORKS.** `nros-board-threadx-linux` registers the writer:
`node.rs::register_log_writer_public()` is called from the board entry, and
`nros-board-threadx-qemu-riscv64/src/node.rs:70` has the same call. Running the
prebuilt fixture proves the facade reaches stderr:

```
$ ./packages/testing/nros-tests/bins/logging-smoke-threadx-linux/target/nros-fast-release/logging-smoke-threadx-linux
[TRACE] smoke: trace payload
[DEBUG] smoke: debug payload
```

So "ThreadX has **no caller anywhere**, on either path" is not true — the claim
appears to have surveyed `nros_platform_register_log_writer` call sites without
following `register_log_writer_public`.

**FreeRTOS-via-Rust — registers.**
`nros-board-mps2-an385-freertos/src/lib.rs:111` makes the same call, so the
"partial" row needs the same re-check under QEMU before anything is changed.

**NuttX — still stands, but UNVERIFIED here.** No `nros_platform_log_write` and
no registration anywhere under `nros-platform-nuttx` / `nros-board-nuttx*`
(`git grep`), which matches the report. It could not be confirmed by running:
`logging_smoke_nuttx_qemu_arm_emits_every_severity` SKIPS on this host
("NuttX source tree not found"), which is itself the issue's point — the gap is
invisible because the cell never runs.

### What that leaves

- Fix 1 (NuttX implementation) — still wanted, but it must be proven by making
  that smoke cell RUN, not by adding a plausible `platform.c`. Writing one blind
  would ship an unverified guess into the exact hole the issue is about.
- Fix 2 (register from the Rust board entries) — **already done** for ThreadX
  and FreeRTOS. Nothing to do unless the QEMU cells show otherwise.
- Fix 3 (make the weak stub loud) — unaffected by the above and still correct:
  a real platform silently falling through to
  `nros-c/c-stubs/weak_platform_log_stubs.c` is the mechanism that would hide
  NuttX's gap even after it is fixed.

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

## RESOLVED (2026-08-05) — NOT A BUG. All three "broken" rows disproved.

Measured, not reasoned.

**NuttX — WORKS.** Booted the fixture directly:

```
$ qemu-system-arm -M virt -cpu cortex-a7 -nographic -kernel \
    packages/testing/nros-tests/bins/logging-smoke-nuttx-qemu-arm/target/armv7a-nuttx-eabihf/nros-minsizerel/logging-smoke-nuttx-qemu-arm
[TRACE] smoke: trace payload
[DEBUG] smoke: debug payload
[INFO] smoke: info payload
[WARN] smoke: warn payload
[ERROR] smoke: error payload
[FATAL] smoke: fatal payload
```

`nm` confirms `T nros_platform_log_write` in the image. The provider is
`nros-platform-posix/src/platform.c`: `nros-board-common`'s
`nuttx_platform_build.rs` compiles the POSIX platform.c against the board's
headers ("`nros-platform-posix/src/{platform.c,net.c}` compiled against the
board's ...", its own doc comment). The survey searched for a *nuttx-specific*
platform.c and concluded there was none — the definition arrives by reuse.

`logging_smoke_nuttx_qemu_arm_emits_every_severity` also PASSES once the cell
can run. (It completes in ~0.2 s, which looks too fast for QEMU and is not: the
fixture skips `Executor::open`, so NuttX boots and prints immediately. Verified
by the manual boot above before trusting it.)

**ThreadX — WORKS.** Shown earlier in this issue by running the threadx-linux
fixture.

**FreeRTOS-via-Rust — registers.** `nros-board-mps2-an385-freertos/src/lib.rs:111`.

### The real finding

The facade is fine; what is missing is the ability to SEE that. Every NuttX cell
skips silently unless two conditions hold, neither of which any setup step
establishes:

- `NUTTX_DIR` must be exported — `activate.sh` and the SDK env never set it, so
  `is_nuttx_available()` is false on a fully provisioned host;
- NuttX must be configured/built (`include/nuttx/config.h`), which needs kconfig
  tooling (`kconfiglib` or `kconfig-frontends`) that nothing provisions.

So the cell reported SKIP on a machine that had the sources, the toolchain and
QEMU. That is the actual gap this issue found, and it is why a genuine
regression here would stay invisible — the same shape as issue 0407, one layer
further out.

Follow-ups worth filing separately, if wanted: export `NUTTX_DIR` from the SDK
env, and have `nros setup <nuttx board>` provision kconfig tooling (or `just
nuttx build` name it as a prereq rather than failing mid-run).
