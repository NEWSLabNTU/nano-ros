---
id: 756
title: "`NROS_MAX_PARAMETERS=256` hangs Zephyr boot right after
  `dds_create_participant` (bisected; 32 boots clean)"
status: open
type: bug
area: zephyr, memory
related: [issue-0749]
---

## Symptom

On the Zephyr FVP lane (autoware-safety-island controller image,
cyclonedds RMW), building with `NROS_MAX_PARAMETERS=256` — the value the
FreeRTOS lanes run, and the natural setting for a controller declaring
150+ parameters — produces an image that boots to
`dds_create_participant` and then hangs: no fault, no panic, no further
output. Bisected on the knob alone (2026-08-22, consumer side):
`NROS_MAX_PARAMETERS=32` with everything else identical boots and runs
the full autonomous-driving loop.

The knob only ACTS on the Zephyr Rust lane since `d1c5b3b3b` (issue 0749
made the sizing knobs reach cargo at all), so this is the first time any
Zephyr image actually built a 256-slot param store — the hang was
unreachable before.

## Consumer state

autoware-safety-island `build.sh` pins the Zephyr lane back to the old
effective value (`NROS_MAX_PARAMETERS=32`; params past the 32nd fall back
to compiled defaults — the behaviour every Zephyr image has always had)
and documents this issue as the unpin condition. FreeRTOS lanes run 256
without trouble.

## Suspicion

A large-parameter-store stack temporary: the param store scales with
`MAX_PARAMETERS` and something on the boot path (store init, or the
first param-service registration inside participant/node bring-up)
plausibly constructs it — or an array of it — on a Zephyr thread stack
sized for the 32-slot layout. A hang rather than a MPU fault is
consistent with a clobbered adjacent stack. Not yet confirmed upstream;
the bisect evidence is knob-level, not frame-level.

## Direction

1. Reproduce on a stock Zephyr cell (native_sim or FVP) with
   `NROS_MAX_PARAMETERS=256` — no consumer code needed.
2. Find the frame that scales with the knob (param store init path);
   move it to static/arena storage or size the owning stack from the
   knob.
3. Whatever the fix, boot should FAIL LOUD when a sizing knob makes a
   stack unviable — a silent hang after `dds_create_participant` took a
   consumer-side bisect to attribute.
