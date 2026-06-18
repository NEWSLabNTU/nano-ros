---
id: 84
title: threadx-riscv64 C fixtures fail to link — duplicate symbol `stderr`
status: resolved
type: bug
area: threadx
related: [phase-251]
resolved_in: "removed the redundant stderr stub from threadx-riscv64 syscalls.c"
---

## Resolution (2026-06-18)

Removed the stub `stderr` (+ the bogus `struct __sFILE`) from
`packages/boards/nros-board-threadx-qemu-riscv64/c/syscalls.c`. `startup.c`'s
UART-backed `stderr` is now the single definition. Verified: the zenoh C
fixtures + Rust `logging-smoke` link clean (`duplicate symbol: stderr` gone).

**A second, independent latent dup is now exposed** in the cyclonedds C
fixtures — `duplicate symbol: ddsrt_setsockreuse` (the vendored Cyclone fork's
ThreadX ddsrt port redefines a fn the generic `sockets.c` already provides).
Tracked separately in **issue 0085**; `build-fixture-extras` still fails the
cyclonedds batch until that lands.


## Problem

`just threadx_riscv64 build-fixture-extras` fails linking the C/C++ example
fixtures (the `threadx-riscv64-c-zenoh-all` batch):

```
rust-lld: error: duplicate symbol: stderr
make: *** [.../threadx-riscv64-c-zenoh-all-*.mk:16: fixture-0002] Error 1
make: *** [.../threadx-riscv64-c-zenoh-all-*.mk:25: fixture-0005] Error 1
error: recipe `build-fixture-extras` failed with exit code 2
```

The Rust `logging-smoke-threadx-riscv64` fixture builds fine; only the C
examples (`riscv64_threadx_c_service_server`, etc.) fail.

## Root cause

`stderr` is defined **twice** in the threadx-riscv64 board crate, both as a
non-`static` global:

- `packages/boards/nros-board-threadx-qemu-riscv64/startup.c:53` —
  `FILE *const stderr = &_uart_file;` (the canonical one: a real picolibc
  `FILE` routed to UART, alongside `stdout`).
- `packages/boards/nros-board-threadx-qemu-riscv64/c/syscalls.c:15` —
  `struct __sFILE *const stderr = &_stderr_file;` (a stub: a fake
  `struct __sFILE { int _unused; }`, "for picolibc's `__assert_func`").

`startup.c` is per-app glue linked into **every** app (`THREADX_STARTUP_SOURCE`);
`syscalls.c` lives in the shared `threadx_glue` static lib. When a C example
references syscalls.c's other stubs (`getpid`/`srand`/`_exit`/…), the linker
pulls `syscalls.o`, whose `stderr` then collides with `startup.o`'s already-linked
`stderr`. The Rust fixture escapes only because it does not pull `syscalls.o`.

**Latent since phase-251.** That phase removed `--allow-multiple-definition`
(`cmake/board/nano-ros-board-riscv64-qemu.cmake:262`), which had masked this dup
(it was added for the strong/weak `memset` overlap). The `stderr` double-define
became a hard link error then; the C fixtures just weren't rebuilt until now.

## Fix

Drop the redundant `stderr` (and the bogus `struct __sFILE` stub) from
`syscalls.c`. `startup.c`'s UART-backed `stderr` is the canonical, correctly-typed
definition and is always linked, so `__assert_func` output still reaches the UART.
