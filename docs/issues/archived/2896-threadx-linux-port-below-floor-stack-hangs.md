---
id: 2896
title: "The ThreadX fork's Linux port hangs on a thread stack below PTHREAD_STACK_MIN instead of raising it: the C-port smoke's 400-byte timer thread runs to its deadline"
status: resolved
type: bug
area: [threadx, platform, ci]
severity: high
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "ThreadX fork ee703dae (nros-lp64-ulong): raise a below-floor thread stack, never hang on it"
related: [2800, 0667, 2888, phase-590]
---

## What failed

The nightly's `ThreadxLinux` job (run 45115997343, 2026-11-23, main
`a1cbbb6b04`), step "Platform C-port smoke (threadx_linux)"
(`just threadx_linux test-c-port`):

```text
ThreadX Linux error: thread stack of 400 bytes is below PTHREAD_STACK_MIN (16384)!
error: recipe `test-c-port` failed with exit code 124
```

Release blocker B14 in [phase-590](../../roadmap/phase-590-nros12-release-plan.md).

## Cause

The fork's `9fdb09be` (issue 2800) runs each Linux thread on its ThreadX
thread's own stack, and refused a stack below `PTHREAD_STACK_MIN` with
`printf` + `while(1)`. The C-port smoke (`tests/threadx-c-smoke`) builds the
kernel with no `tx_user.h`, so the kernel's System Timer Thread keeps its
400-byte default and the image hangs at kernel start. The board images do not
hit it because the board's `tx_user.h` raises `TX_TIMER_THREAD_STACK_SIZE`.

That breaks the repo rule issue 0667 set: a task's stack size is a floor the
port raises to its own minimum, never a size the caller must get right. No
portable caller can name a size legal on every host (16 KiB on x86_64 glibc,
128 KiB on aarch64).

## Fix

Fork commit `ee703dae` on `nros-lp64-ulong`: a thread whose stack is below
the floor runs on a host-allocated stack of `PTHREAD_STACK_MIN` bytes, with a
warning naming the thread and the size it asked for. A stack at or above the
floor still runs on the ThreadX stack. The superproject pin moves to it.

## Measured (x86_64, `tests/threadx-c-smoke`)

| kernel | result |
|---|---|
| `9fdb09be` (before) | the error above, then a hang (timeout rc 124) |
| `ee703dae` (after) | `nros-platform-threadx-c smoke PASS`, with one warning: `thread 'System Timer Thread' stack of 400 bytes is below PTHREAD_STACK_MIN (16384); it runs on a host stack of 16384 bytes instead` |
