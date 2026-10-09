---
id: 1770
title: "QEMU-run C/C++ images (mps2 FreeRTOS, mps2 bare metal, NuttX) do not
  report their app's status to the emulator's exit code"
status: open
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
