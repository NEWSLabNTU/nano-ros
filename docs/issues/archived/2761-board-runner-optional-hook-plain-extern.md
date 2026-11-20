---
id: 2761
title: "Board runners reached the optional `nros_board_network_wait` hook through a plain `extern`, so a FreeRTOS POSIX image whose entry never includes `<rclc/detail/main.h>` failed to link"
status: resolved
type: bug
area: [boards, freertos, zephyr, nuttx, testing]
severity: high
related: [issue-1459, issue-1286]
found: 2026-11-20
resolved: 2026-11-20
resolved_in: "the change that archives this issue"
---

## What happened

An unmodified ROS 2 C++ program built for the FreeRTOS POSIX simulator with
`nano_ros_add_executable(<name> <src>.cpp ROS2_MAIN)`, whose source includes
only `<rclcpp/rclcpp.hpp>`, failed at link:

```
freertos_run_sched_tasks.c:(.text+0xc8d): undefined reference to `nros_board_network_wait'
freertos_run_sched_tasks.c:(.text+0x1811): undefined reference to `nros_board_network_wait'
nros_rtos_run_components.c:(.text+0x23a): undefined reference to `nros_board_network_wait'
nros_rtos_run_components.c:(.text+0x244): undefined reference to `nros_board_network_wait'
collect2: error: ld returned 1 exit status
```

Reproduced in-tree with the fixture row this issue adds
(`freertos-posix-cpp-port-minimal-publisher`) against the unfixed runners.

## Why

`nros_board_network_wait` is an OPTIONAL board hook: its default is a weak
no-op body emitted by `<rclc/detail/main.h>`, and a board may override it
strongly. A header-emitted body exists only in the TUs that include the
header. The `ROS2_MAIN` entry includes `<rclc/detail/app_main.h>`, not
`main.h`, so no TU of that image emits the default.

Issue 1459 fixed the ThreadX twin in the shared runner
(`nros_rtos_run_components.c`) with a weak reference and a guarded call. The
three per-RTOS sched-task runners kept a plain `extern` and an unguarded call:

| site | hook | was |
| --- | --- | --- |
| `nros-board-freertos/c/freertos_run_sched_tasks.c:86` (calls :840, :1245) | `nros_board_network_wait` | plain `extern` |
| `nros-board-zephyr/c/zephyr_run_sched_tasks.c:300` (call :942) | `nros_board_network_wait` | plain `extern` |
| `nros-board-nuttx-qemu/c/nuttx_run_sched_tasks.c:82` (calls :775, :1250) | `nros_board_network_wait` | plain `extern` |
| `nros-board-common/c/nros_rtos_run_components.c:119` | `nros_board_network_wait` | weak ref (1459), own macro |

They linked on the boards that link with `--gc-sections` (MPS2/MPS3/S32Z
FreeRTOS, Zephyr on hardware targets, NuttX), which discard the unreferenced
runner functions together with their undefined references. FreeRTOS POSIX
links without it (`nros_board_link_app` adds only pthread, though the board
cmake's comment claimed otherwise), so the runner object stays whole and the
plain reference is reported. One plain reference is enough even beside the
1459 weak one: GNU ld merges the bindings of every reference to a symbol, so
the weak reference sites in `nros_rtos_run_components.c` were reported too.

The sweep found no other optional hook of this shape: the only
`__attribute__((weak)) void name(void) {...}` body in `packages/api/*/include`
is `nros_board_network_wait`. `nros_app_main_returned` (weak body in
`app_main.h`) is defined strongly by the board that calls `app_main`, never
declared `extern` by a runner; `nros_platform_wake_park_*` are defined by the
FreeRTOS and Zephyr ports those runners are compiled for.

## Fix

- `packages/boards/nros-board-common/c/nros_board_optional_hook.h`: the one
  spelling. `NROS_BOARD_OPTIONAL_HOOK(name)` declares a weak reference,
  `NROS_BOARD_CALL_OPTIONAL_HOOK(name)` calls it only when defined; without GNU
  C both fall back to a plain declaration and call. The header declares
  `nros_board_network_wait` once. All four runners include it; 1459's
  `NROS_RUN_NETWORK_WAIT` is gone. Behaviour is unchanged: the header default
  or a strong override runs when linked, and when neither is the call is
  skipped, which is what the no-op default did. A guarded weak-undefined call
  also links on RISC-V at 0x80000000 (checked, medany and medlow).
- `scripts/check/check-board-hook-weak-ref.py` (`just check
  board-hook-weak-ref`, fast lane): every header-emitted weak `void(void)` hook
  is declared in that header, and no tracked C/C++ file under `packages/` or
  `zephyr/` names one by a plain declaration or call unless it includes the
  header that emits the default or defines the hook strongly. It flags all
  four pre-fix runners.
- Fixture row `freertos-posix-cpp-port-minimal-publisher`
  (`examples/templates/cpp-port-minimal-publisher/freertos-posix`), built by
  `just freertos build-fixtures-posix`: fails to link before, links after.
  The boards that link with `--gc-sections` already carry `ROS2_MAIN` rows
  with no `main.h` (`cpp-port-minimal-publisher` on `mps2-an385-freertos` and
  Zephyr `mps2/an385`), but those cannot catch a regression, since garbage
  collection hides it; the buildless check is their guard.
