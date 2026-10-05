---
id: 1706
title: "The parameter store's 285,696-byte heap allocation is sized from the declaration only on Zephyr, and only when the bringup declares `param_services`"
status: open
type: tech-debt
area: [sizing, zephyr, freertos, threadx, cpp]
severity: low
found: 2026-10-06
related: [1702, 1677, 1529, 0756, phase-382]
---

## What

Issue 1702 made the Zephyr nros heap (`CONFIG_NROS_ZEPHYR_HEAP_SIZE`) take its
DEFAULT from the declared capability axes. A bringup that declares
`param_services` gets 512 KiB, and one that declares `lifecycle` gets 192 KiB.
Both values are measured, and the carrier is the generated west application's
`CONFIG_NROS_CAPABILITY_*` assignment.

The allocation that motivated it can still happen in three places that
derivation does not reach:

1. **Other RTOS heaps.** `Executor::leak_parameter_storage` boxes one
   `ParameterStorage<MAX_PARAMETERS>`, which is 285,696 bytes at the default 32
   slots (each slot is ~8.5 KiB, sized by `ParameterValue`'s `StringArray`
   variant). On FreeRTOS that comes out of heap_4 (`configTOTAL_HEAP_SIZE`), on
   ThreadX out of the byte pool, and on NuttX out of the system heap. No in-tree
   image on those platforms declares `param_services`, so nothing has measured
   it there, and no default follows the declaration.
2. **C++ images that never declare the axis.** A C++ image always carries the
   store (`param-store`, issue 1529). Anything that declares a parameter builds
   it: a launch `<param>` seed, `declare_parameter`, or a `ComponentNode` that
   declares. On Zephyr such an image keeps the 64 KiB default and halts at the
   first declaration with `HEAP EXHAUSTED (TOO SMALL): request 285696`, which is
   1702's symptom without 1702's trigger.
3. **The store's size is not consulted.** A bringup whose contracts declare
   `params:` derives a far smaller store (phase-446 W4: 25 scalar slots measured
   at 4,200 B). It still gets the 512 KiB default. That over-sizes the heap, the
   safe direction, and stating the heap overrides it. It still costs RAM that
   nothing derived.

## Rejected direction (measured, issue 1702)

One option was to make the store a named `.bss` static. That reserves the
bytes in every image that LINKS the declare path, not only in those that run
it. A plain mps2 C++ talker gained 280,832 B of `.bss` for a store it never
builds.

## Fix direction

The sound end state is phase-382 W3', the store carved from caller-placed
storage, together with a store whose footprint follows the parameters actually
declared. Until then, give each RTOS heap knob the same capability-conditional
default Zephyr now has, measured on a real image per platform, and make the
derivation also fire for a C++ image whose launch file seeds a `<param>`.
