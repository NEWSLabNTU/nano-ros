---
id: 2953
title: "The heap defaults priced on one lifecycle machine still carry its old 40,960 B of service buffers, about 39 KB more than a machine now boxes"
status: open
type: tech-debt
severity: low
area: [sizing, lifecycle, zephyr, freertos, threadx]
related: [phase-554, 2919]
found: 2026-11-24
---

## What

Until phase-554 W5 (issue 2919), a lifecycle state machine's five servers each
owned a request and a reply buffer of the image's generic service size, 4,096 B
by default: 40,960 B of buffers per machine. W5 sizes each buffer for its own
lifecycle message instead, 1,744 B per machine
(`LIFECYCLE_MACHINE_BUFFER_BYTES`, nros-node `lifecycle_services.rs`).

The per-machine term of the heap floor (`NanoRosServiceHeap.cmake`,
`nros_sizing_descriptor::service_heap`) moved with it. The ONE-machine bases
did not, because they are measured, not derived:

- Zephyr: `default 196608 if NROS_CAPABILITY_LIFECYCLE` in `zephyr/Kconfig`,
  and `NROS_ZEPHYR_HEAP_ONE_LIFECYCLE_MACHINE` (196,608) in
  `zephyr/cmake/nros_heap_floor.cmake`. The knob help's measurement says
  lifecycle cost about 64.5 KiB of that, with the 40,960 B of buffers in it.
- FreeRTOS and ThreadX: the heap or byte-pool defaults their lifecycle images
  were measured on.

Each now carries about 39,216 B (40,960 - 1,744) that no image allocates.
That is the safe direction, so nothing fails, but it is RAM a small board
cannot spare.

## Fix

Re-measure each base on a one-machine image built after W5, as the knob help's
existing measurements were taken (boot record, `read-boot-report.py
--heap-headroom`, or the image's last `nros: heap peak` line), and lower the
arms to the new peak plus the documented floor. Re-check the per-machine
remainders at the same time: they are the cost beyond the buffers, so they
should not move.

## Acceptance

- Each arm is re-measured, with the number and the image named beside it.
- Features' `zephyr_mps2_c_lifecycle_pair_xrce` (two machines, no heap
  stated) still boots on the derived floor.
