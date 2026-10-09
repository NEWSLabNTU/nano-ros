---
id: 1752
title: "A host-process RTOS image (threadx-linux, freertos-posix) exits 0 when
  its C app FAILED — the VOID entry shim discards nros_app_main's status"
status: resolved
type: bug
area: [platform, testing]
severity: low
found: 2026-10-08
related: [1741, 1770]
---

## What was measured

`examples/threadx-linux/c/service-server/build-zenoh/c_service_server` with
no router (2026-10-08, after issue 1741's fix):

    [nros] …/service-server/src/main.c:125 nros_support_init(&app.support, locator, domain_id) -> -4
    [app_thread] app_main returned
    exit status 0, 0.5 s

`nros_app_main` returned 1 (`NROS_CHECK_RET(…, 1)`), and the process said 0.

## Why

Every RTOS entry uses `NROS_APP_MAIN_REGISTER_VOID()`
(`packages/api/nros-c/include/nros/app_main.h`), which is
`void app_main(void) { (void)nros_app_main(0, NULL); }` — the status is
dropped before any board sees it. On an MCU that is harmless (nothing reads
it). The two boards that ARE host processes now end the process when the app
returns — `nros-board-freertos-posix`'s entry with `exit(0)`, and since issue
1741 threadx-linux's `nros_board_app_returned()` with `_exit(0)` — so both
report success for a failed app. Before 1741 the threadx-linux image did not
exit at all, so nothing was reporting the status then either; this is not a
regression, but it is now a visible lie a harness could believe.

## Shape of a fix

Carry the status to the board: e.g. the VOID shim stores it where a weak
board hook (`nros_board_app_returned(int status)`) receives it, defaulting to
a no-op on MCUs. Mind the RISC-V weak-undefined relocation note in
`threadx_hooks.c` before adding a weak symbol reference.

## Not established

Whether any harness reads the exit status of a threadx-linux or
freertos-posix image today (the e2e tests grep output).

## Resolution

Fixed 2026-10-09 on `fix/1752-entry-shim-exit-status`.

**The hand-off.** `NROS_APP_MAIN_REGISTER_VOID()` (`<nros/app_main.h>`) now
expands to `app_main(void) { nros_app_main_returned(nros_app_exit_code(nros_app_main(0, 0))); }`
plus a WEAK no-op `nros_app_main_returned` beside it. `nros_app_exit_code`
is a `static inline` in the same header: 0 if and only if the status is 0,
otherwise its low byte, with a low byte of 0 folded to 1 (a raw `exit(256)`
would read as success).

It is a weak DEFINITION, not a weak reference, so RISC-V's `PCREL_HI20` note
in `threadx_hooks.c` does not apply: nothing is ever left undefined. A board
that wants the status defines the strong recorder in the SAME object that
calls `app_main`. A linker never pulls an archive member just to replace a
weak definition, so that object is the only placement that is guaranteed to
be extracted. The header states this contract.

**The boards (the class: every board that is a host process):**
- `nros-board-common/c/threadx_hooks.c` covers every ThreadX board. It holds
  the strong recorder, and `nros_board_app_returned` became `(int exit_code)`:
  the C/C++ arms pass the recorded code, and the Rust arm passes 0. A Rust
  entry ends through `BoardExit` and never returns there.
- `nros-board-threadx-linux` `_exit(exit_code)`.
- `nros-board-freertos-posix/c/freertos_posix_entry.c` has the strong
  recorder and `exit(app_exit_code)`.
- Every other entry spelling was checked. The C and C++ codegen boot wrappers
  expand the macro, so the fix reaches them. The Zig toy entry pack spells the
  macro body itself; it now calls the header's hook and normaliser and
  weak-exports the default. Its goldens were re-recorded. It is not compiled,
  because the host has no Zig toolchain. Rust entries (`nros::main!`, board
  `run` funnels) already end through `BoardExit::exit_success/failure`. The
  `int main` shims (POSIX, Zephyr, NuttX) already returned the status.

**Measured** (exit status of the image, before → after):

| image | how it fails | before | after |
| --- | --- | --- | --- |
| threadx-linux `c/service-server` (zenoh) | `NROS_LOCATOR` at a port nothing listens on, `nros_support_init -> -4`, app returns 1 | **0** | **1** |
| freertos-posix `workspaces/c` entry (Cyclone) | `CYCLONEDDS_URI='<Bogus/>'`, RMW session open fails | **0** | **156** |
| freertos-posix `workspaces/cpp` entry (Cyclone) | same | not run | **156** |

`nm` on each after-image shows `T nros_app_main_returned`, the strong
definition. The success path is unchanged: the threadx-linux zenoh image with
a live router exits 0 after one SIGTERM.

**Tests:**
- `check-c` gets a link-and-run probe
  (`packages/api/nros-c/tests/run/app_main_status_{app,board,weak}.c`). The
  app half is compiled as C and as C++. The board half has a strong recorder
  and must see 0→0, 1→1, 3→3, -4→252 and 256→1. A weak-only link must still
  link and run. **Negative control, measured:** against the pre-fix header,
  the board probe fails all 5 cases with "the shim never called
  nros_app_main_returned".
- `native_api::test_threadx_linux_c_image_exit_status_is_the_apps` checks two
  endings, and each is the other's control. With no router the image must
  exit exactly 1; the old board gave 0. With a live router and one SIGTERM it
  must exit 0; this catches a board that always reports failure.

**Sweep:** `git grep -n "app_main()\|app_main(void)\|app_returned" -- packages cmake 'packages/cli/**/testdata/entry-packs'`.
That lists every caller of `app_main` and every definition of the shim.

**Not done / not measured:**
- The QEMU-run boards still end on a status of their own:
  `freertos_c_entry.c`'s semihosting exit, the mps2 bare-metal `run_bare`
  closures, and NuttX's FFI `main`. They are not host processes; issue 1770
  covers them. FreeRTOS's A32 `SYS_EXIT` call is not run.
- `test_threadx_linux_c_image_ends_on_one_sigterm::case_1_cyclonedds` was not
  re-run, because this branch did not rebuild its fixture (it was STALE).
- The Zig template is not compiled.
