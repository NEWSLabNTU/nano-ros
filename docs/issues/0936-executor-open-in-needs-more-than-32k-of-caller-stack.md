---
id: 936
title: "Executor::open_in needs more than 32 KiB of the calling thread's stack, and nothing says so"
status: open
area: core, memory
severity: high
found: 2026-08-31
related: [0900, 0271, 0739, phase-403, phase-3]
---

# The init path costs more stack than a small part can give it

## What was measured

mr-canhubk344 (S32K344, 320 KiB SRAM), RFC-0043 C++ components over zenoh
on serial, `CONFIG_MAIN_STACK_SIZE` bisected against a booting image:

| main stack | outcome |
| ---: | --- |
| 8192 | overflow |
| 16384 | overflow |
| 28672 | overflow, NAMED once `CONFIG_HW_STACK_PROTECTION=y` |
| 32768 | no named overflow, but `main` corrupts the idle thread's stack |

Named report at 28672:

```
>>> ZEPHYR FATAL ERROR 2: Stack overflow on CPU 0
Current thread: main
Faulting instruction address (r15/pc): __aeabi_memclr8
r14/lr:                                 Executor::open_in
```

So the requirement is somewhere above 32 KiB, on a part with 320 KiB of SRAM
in total. The consumer is a `memclr` reached from `open_in`.

## Why it is hard to see

**Without the MPU stack guard the overflow does not report as one.** `main`
runs off its stack into whatever is next -- on this image the idle thread's
stack -- and the fault surfaces later, in a different thread, as
`USAGE FAULT: Illegal load of EXC_RETURN into PC` with `pc = 0`. That reads as
memory corruption anywhere in the image, and it names the WRONG THREAD:
`Current thread: idle`. Two separate bring-up sessions were spent on the idle
thread before the guard was enabled.

The guard is not free on a tight image (it selects `MPU_STACK_GUARD`, which adds
a guard region to every thread stack -- measured 23244 B over on this one), so
the tool that names the failure is the tool such an image cannot afford. That is
the same shape as issue 0900's note about the arena being invisible to
`mem-report`.

## What is NOT the cause

* Not the executor arena. Holding the stack at 28672 and taking
  `NROS_EXECUTOR_ARENA_SIZE` from 49152 to 40960 changed nothing -- same
  `MMFAR`, same registers.
* The FPU registers in the fault dump are stale. `s[3]`/`s[4]` read `0x0000c000`
  in both runs, which looks like an arena size and is not one; it did not track
  the knob. Do not infer from them.

## Why it matters beyond one board

`open_in` is on every image's boot path. A hosted target with an 8 MiB main
thread never notices; a 320 KiB part cannot pay it, and gets a fault that names
neither the cost nor the thread that owns it. phase-403 makes receive buffers
type-sized, which took this image's arena from 86108 B to 24516 B -- and none of
that helps, because the stack requirement is independent of it.

## What would resolve it

1. Find what `open_in` zeroes on the caller's stack and whether it can be
   constructed in place instead. `nros_cpp_init` already carves the executor
   from caller-owned storage precisely to avoid a large value transiting the
   stack, so a stack-resident temporary in `open_in` defeats that intent.
2. Failing that, STATE the number. A boot-time check like the heap gate
   (`nros: the executor arena cannot fit in the platform heap`) that names the
   required stack would have turned four bring-up sessions into one build error.
