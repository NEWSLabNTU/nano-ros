---
id: 1752
title: "A host-process RTOS image (threadx-linux, freertos-posix) exits 0 when
  its C app FAILED — the VOID entry shim discards nros_app_main's status"
status: open
type: bug
area: [platform, testing]
severity: low
found: 2026-10-08
related: [1741]
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
