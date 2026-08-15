---
id: 620
title: "`NROS_PLATFORM_TASK_STORAGE_SIZE` is 256 B, sized from a 32-bit port —
  ThreadX's `TX_THREAD` is 352 B on a 64-bit host, so threadx-linux cannot compile"
status: open
type: bug
area: platform
related: [issue-0617, issue-0570, issue-0582]
---

## Symptom

`just build-test-fixtures lane=tier2` loses the whole threadx-linux family:

```
packages/platform/nros-platform-threadx/src/platform.c:587:1:
  error: static assertion failed: "NROS_PLATFORM_TASK_STORAGE_SIZE too small for this port"
```

## Mechanism

`packages/platform/nros-platform-api/include/nros/platform.h`:

```c
#ifndef NROS_PLATFORM_TASK_STORAGE_SIZE
/* ThreadX `TX_THREAD` is the large one (~232 B on 32-bit). */
#  define NROS_PLATFORM_TASK_STORAGE_SIZE     256
#endif
```

The comment records where the number came from, and that is the defect: 256 is
a bound over the 32-bit ports. Measured on this host, with the board's own
config on the include path:

```
sizeof(TX_THREAD) = 352
```

ThreadX-Linux is a HOSTED port — 64-bit pointers — so its `TX_THREAD` carries
the same field count at twice the pointer width and overflows a bound chosen for
`thumbv7m`. 96 bytes over.

The header is explicit that these are "UPPER BOUNDS over every supported port,
not per-port exact sizes"; the ThreadX-Linux port was simply not in the set the
number was taken over.

## Note on the assert

The `_Static_assert` is doing exactly its job and should stay. It is the
phase-360 W5 mechanism that replaced `zpico-sys`'s hand-computed "≈ with a 2×
safety margin" table (issue 0570) — a bound that stops being true is now a
compile error in the port instead of a silent overrun in the consumer. This
issue is the first time it has fired in anger, and it fired on a real overflow,
not a false positive.

## Fix

The header already documents the intended escape hatch:

> A port may raise a bound by defining it before including this header.

So this is a per-port raise from the ThreadX side, not a bump of the shared
bound: raising 256 globally would cost every consumer that embeds by value —
including the 32-bit ports where 256 is generous and RAM is scarce — for a size
only the hosted port needs.

Whoever takes it should also check the sibling bounds against a 64-bit
`TX_MUTEX` / `TX_SEMAPHORE`; the same pointer-width reasoning applies and only
the task assert has fired so far because it is the largest struct.

## Probably not aarch64-specific

Filed from an aarch64 host, but the cause is the DATA MODEL, not the
architecture: any LP64 host makes `TX_THREAD`'s pointers 8 bytes. An x86_64
build of threadx-linux should hit the identical assert, so this is likely a
break for everyone rather than a port-on-a-new-host problem. Worth confirming on
x86 before assuming otherwise — the inverse mistake (calling something
arch-specific when it is data-model-specific) is what issue 0582 was made of.
