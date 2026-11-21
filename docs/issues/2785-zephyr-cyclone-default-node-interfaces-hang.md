---
id: 2785
title: "A Zephyr Cyclone image with the stock node interfaces (rosout, sim_time) boots and then emits no nros log record; with node_interfaces = \"none\" it runs"
status: open
type: bug
severity: high
area: [zephyr, rmw-cyclonedds, logging]
related: [phase-556, issue-2786]
found: 2026-11-21
---

**Home: [phase-556](../roadmap/phase-556-ros2-compat-audit.md) W13**, which made the
stock node interfaces every image's default (6229085857).

## What happens

A consumer's Zephyr controller image on `fvp_baser_aemv8r` with Cyclone DDS
(the safety island's `[image.fvp]`, nano-ros pin 1866c0594f -> 7ca11830f9)
gains `CONFIG_NROS_CAPABILITY_ROSOUT=y` and `CONFIG_NROS_CAPABILITY_SIM_TIME=y`
from the new default. It prints

    nros: sched task priority set sched_task=`control` prio=9

and then no nros log record at all for 500 s: not its stack-headroom warning,
not "Starting Controller Node". It reproduces on a rerun. The same image at
1866c0594f, before the default changed, is Live in about 1.2 s.

With `node_interfaces = "none"` on the image, at 7ca11830f9, it boots: the
parameter store is carved (164 slots) and it is Live at 1.28 s model time. So
the hang comes with the interfaces, most likely rosout (the first nros log
record never appears); sim_time is not ruled out yet.

The FreeRTOS POSIX and AN536 (Cyclone, FreeRTOS) images of the same consumer
at the same pin are not affected.

## Why the consumer cannot work around it

- `node_interfaces = "none"` also removes `param_services`, so `ros2 param`
  stops working on the image.
- A conf fragment with `CONFIG_NROS_CAPABILITY_ROSOUT=n` is overridden: the
  built `.config` still has `=y` (issue 2786).

## Direction

1. Build the image with rosout on and sim_time off, and the reverse, to tell
   which interface hangs.
2. Find where the boot stops (the rosout publisher's creation or its first
   publish on the Zephyr Cyclone road, or the log sink path).
3. Fix the hang. A per-interface opt-out is worth having on its own, but it
   is not the fix.

Logs: the consumer's lane logs `s1-fvp.log` (hang) and `fvp-exp-run.log`
(`none`, boots), kept with the consumer's draft.
