---
id: 1770
title: "QEMU-run C/C++ images (mps2 FreeRTOS, mps2 bare metal, NuttX) do not
  report their app's status to the emulator's exit code"
status: resolved
type: bug
area: [platform, testing]
severity: low
found: 2026-10-09
related: [1752, 1741]
---

## What remains after issue 1752

Issue 1752 made `NROS_APP_MAIN_REGISTER_VOID()` hand `nros_app_main`'s status
to the board through `nros_app_main_returned(int exit_code)` (weak no-op in the
shim, strong recorder in the board object that calls `app_main`), and the two
HOST-PROCESS boards now exit with it. The QEMU-run boards still end on a
status of their own, so a harness reading QEMU's exit code learns nothing
about the app:

- **`nros-board-freertos/c/freertos_c_entry.c`** (mps2-an385, mps3-an536,
  s32z270 C/C++ lane): after `app_main()` it issues semihosting `SYS_EXIT`
  (`0x18`) with `r1` pointing at the block `{ADP_Stopped_ApplicationExit, 0}`.
  That block form is the A64 calling convention. On A32/T32 QEMU reads `r1`
  as the REASON itself, and per QEMU's `arm-compat-semi.c` anything other than
  `ADP_Stopped_ApplicationExit` exits 1. If that reading is right, these images
  exit **1 whatever the app did**. The Rust lane's `cortex_m_semihosting::debug::exit`
  passes the reason in `r1`, and `SYS_EXIT_EXTENDED` (`0x20`) is the A32 call
  that takes a block with a status.
- **`examples/mps2-an385-baremetal/{c,cpp}/*/src/main.rs`**: the closure
  passed to `run_bare` calls `app_main()` and returns `Ok(())`, so
  `run_bare` always takes `exit_success`.
- **`nros-board-nuttx-qemu/nros-nuttx-{,riscv-}ffi/src/main.rs`**: `main`
  returns 0 after `app_main()`. That status reaches nsh, not QEMU, so whether
  it matters depends on the harness.

## Shape of a fix

Each board defines the strong `nros_app_main_returned` in the object that
calls `app_main` (the contract in `<nros/app_main.h>`) and ends with it:
`freertos_c_entry.c` via a correct A32 exit (`SYS_EXIT` with the reason in
`r1`, or `SYS_EXIT_EXTENDED`); the bare-metal leaves via a board helper that
returns `Err` from the `run_bare` closure on a non-zero code. A Rust-defined
recorder must live in an object the linker is guaranteed to load (the bin
crate, or the same module as the code that calls `app_main`), or the shim's
weak no-op wins silently.

## Not measured

None of the above was run. The FreeRTOS reading comes from QEMU's semihosting
source (QEMU 9.0.2 on this host) and has not been checked against an image.
Whether any test reads these exit codes today is also unknown; the QEMU e2e
tests grep output.

## Resolution

Fixed on `fix/1770-qemu-board-exit-status`.

**Measured before (2026-10-11, this worktree's builds, QEMU 9.0.2).**

- FreeRTOS C `service-client` (mps2-an385, zenoh), with a router and the C
  `service-server`: the app completed (`Goodbye!`) and **QEMU exited 1**. With
  no router it failed and also exited 1. So the issue's reading of
  `arm-compat-semi.c` is right: the A64 block form of `SYS_EXIT` on A32 exits 1
  whatever the app did.
- mps2-an385 bare-metal C `service-client`, no router: `nros_support_init`
  failed, the board printed `nros: application complete` and **QEMU exited 0**.

**Fix.**

- `nros-board-freertos/c/freertos_c_entry.c`: a strong
  `nros_app_main_returned` recorder in the object that calls `app_main`
  (issue 1752's contract), and `SYS_EXIT_EXTENDED` (0x20) with
  `{ADP_Stopped_ApplicationExit, status}`. One file serves every FreeRTOS C/C++
  QEMU board (mps2-an385, mps3-an536, s32z270).
- `examples/mps2-an385-baremetal/{c,cpp}/*/src/main.rs` (12): the `run_bare`
  closure calls `nros_app_main(0, NULL)` and returns `Err(status)` on non-zero.
- `nros-board-nuttx-qemu/nros-nuttx-{,riscv-}ffi/src/main.rs`: `main` returns
  `nros_app_main`'s status (the `int main` shim's contract) instead of 0.

The Rust callers call `nros_app_main` directly instead of defining a Rust
recorder, which keeps them out of the weak/strong archive-member question the
issue warned about. Every C/C++ entry defines `nros_app_main` (the examples and
the codegen boot wrappers alike).

**Measured after.** FreeRTOS: success **0**, no-router failure **1**.
Bare-metal no-router failure: `nros: application error: 1`, **QEMU 1**.

**Regression test.** `QemuProcess::wait_exit` (new), and
`test_rtos_service_e2e` requires the FreeRTOS C/C++ client to end QEMU with 0
after its result. FreeRTOS C: PASS (25.2 s). Negative control (old entry,
rebuilt): FAIL, `service client ended QEMU with exit status: 1 after printing
its result`.

Sweep: `git grep -nE "0x20026|mov r0, #0x18|unsafe \{ app_main\(\) \}" -- packages examples`.

## Not measured (at resolution)

- The bare-metal SUCCESS path end to end: no in-tree QEMU networking connects
  these leaves (the guest comes up at 192.0.3.10 against a baked
  `tcp/10.0.2.2:10500` locator), and their fixture rows are build-only. The
  change there is `status == 0 → Ok`.
- NuttX: not built or run. Its status reaches nsh, not QEMU, so whether any
  harness reads it is still open.
- The FreeRTOS C++ client and the mps3-an536 / s32z270 images (same file).
