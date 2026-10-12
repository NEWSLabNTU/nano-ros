---
id: 1833
title: "Test preconditions read `FREERTOS_DIR`/`THREADX_DIR`/… from the environment, which nothing exports since phase-484 W3c"
status: resolved
type: bug
area: [testing, build]
severity: high
found: 2026-10-12
related: [1767, 1811, phase-484]
---

## Measured

From a shell with the SDK variables unset (the state `activate.sh` leaves
since phase-484 W3c), the riscv64 C++ pub/sub cell reported:

```
[UNMET PRECONDITION] threadx_riscv64: THREADX_DIR not set or invalid
```

and `just freertos test-all` stopped with `FreeRTOS not found at .`, while
the SDK store held every tree and the fixtures had been built from it. The
same cells passed from a shell that had inherited the variables from an
older activation, so whether an RTOS test RAN depended on the history of
the shell.

## Cause

W3c moved the trees into the store and stopped exporting their variables;
the build asks the one ladder (`nros_build_paths::locate`), which honours
the variable first and then the checkout and the store. The test-side
probes were not part of that sweep: `is_freertos_available`,
`is_lwip_available`, `is_threadx_available` (twice), `is_netx_available`,
`is_nuttx_available`, `is_nuttx_configured`, `nuttx_kernel_path_for` and
`integration_nuttx`'s guard each read the variable directly. Issue 1811 is
the same miss in four nightly workflow jobs.

## Fix

`nros_tests::sdk_tree::sdk_tree(<source>)` asks the build's ladder, and
every probe uses it, so a test looks where its fixture was built from. The
skip messages name the setup recipe and the override variable.
`check-store-source-readers` now also refuses Rust that reads a store-first
row's `env` variable directly (self-tested; a negative control fails on
the old `is_freertos_available`).

The `just freertos test-all` recipe half is fixed with phase-484 W4b-2
(`nros_locate_sdk_trees`).
