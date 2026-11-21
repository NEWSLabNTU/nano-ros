---
id: 2762
title: "Concurrent loggers lose most of their records: one delivery at a time, and the hold queue behind it holds 4"
status: open
type: bug
severity: medium
area: [logging, core, zephyr]
related: [issue-2000, issue-2738]
found: 2026-11-20
---

## Measured

Host 241, Zephyr 3.7, `qemu_cortex_a53/smp` with 2 CPUs, the issue 2738
stress image: K threads each log 100 `RCLC_LOG_INFO` records while a 1 ms
timer ISR logs too. The Zephyr sink had N buffers. Every run reached its end,
and no record took the sink's truncating fallback (`fallbacks 0` in all 12
cells). The records lost came from `nros_log` itself, which reported them:

    log record(s) lost: more were waiting for a sink than NROS_LOG_EARLY_RECORDS = 4 holds

| lost of K x 100 | K=1 | K=2 | K=4 | K=8 |
| --- | --- | --- | --- | --- |
| N=2 | 0 | 92 | 226 | 559 |
| N=4 | 0 | 0 | 648 | 485 |
| N=8 | 0 | 17 | 596 | 976 |

N4/K4 lost 648 of 800. Out of hundreds of timer ticks, 0 or 1 ISR records
reached the console per run.

## Why

`dispatch_to_sinks` (`packages/core/nros-log/src/lib.rs`) holds ONE
process-wide guard across every `sink.log()`. Issue 2000 stopped a record that
finds the guard held from being dropped: it waits in `nros_log::early`'s hold
queue, and the holder delivers it before letting go. That queue is
`NROS_LOG_EARLY_RECORDS` deep (builtin 4). A record that arrives while it is
full is counted and lost. A single delivery on Zephyr formats into a 1280-byte
buffer and goes through `LOG_*`, so K threads logging at once fill 4 slots
quickly.

Two consequences:

- The loss is reported, but only as a count, after the fact. Which records
  went, and from which thread, is gone.
- The Zephyr sink can never be entered by two threads at once, so its buffer
  pool (issue 2738) never sees concurrent claims through `nros_log`. Its
  default is 1 for that reason, and more buffers buy nothing until delivery
  stops being serialised.

## Directions (not decided)

1. Size the hold queue per CPU or per logging thread on SMP images, using the
   board's CPU count as the floor.
2. Let a record that meets a busy dispatcher go to sinks that declare
   themselves re-entrant (the Zephyr sink's claimed pool is). The guard then
   only keeps a sink that logs from recursing into itself.
3. Block briefly instead of holding, for thread context only. An ISR keeps
   holding.

## Acceptance

The stress cell above, with K=8 and the ISR running, delivers every thread
record, or states per record which were lost. A gate cell runs it on
`qemu_cortex_a53/smp`.
