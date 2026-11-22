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

## Directions

1. Size the hold queue per CPU or per logging thread on SMP images, using the
   board's CPU count as the floor.
2. Let a record that meets a busy dispatcher go to sinks that declare
   themselves re-entrant (the Zephyr sink's claimed pool is). The guard then
   only keeps a sink that logs from recursing into itself.
3. Block briefly instead of holding, for thread context only. An ISR keeps
   holding.

## Ruling, 2026-11-22: direction 2

The user chose direction 2, relayed by the manager.

- Each sink DECLARES whether it is re-entrant: safe to enter from several
  threads, CPUs and ISRs at once. A record that meets a busy dispatcher goes
  straight to every re-entrant sink, and waits in the hold queue only for the
  sinks that are not.
- The dispatch guard then stops only SELF-recursion: a sink whose own `log()`
  logs again does not re-enter itself.
- The Zephyr sink is re-entrant, since its buffers are claimed with one atomic
  bit each. Its pool default becomes 2 x `CONFIG_MP_MAX_NUM_CPUS`, one
  thread-context record and one ISR-context record in flight per CPU.
- Records from different threads can then interleave in arrival order.

Rejected: direction 1, because a deeper queue only moves the loss to a larger
K, at RAM cost per slot. Direction 3, because it stalls a logging thread
behind a slow sink (a priority-inversion risk) and still loses ISR records.

## Acceptance

1. **Concurrent loggers lose nothing on the Zephyr sink.** In the stress cell
   above on `qemu_cortex_a53/smp`, K concurrent loggers, with K well above
   the pool size and the ISR running, lose no record. A record that finds
   every pool buffer claimed takes the sink's truncating fallback, which
   delivers it marked as truncated and counts it; that is not a loss. A gate
   cell runs it.
2. **A non-re-entrant sink keeps today's one-at-a-time delivery** (the
   negative control). A host test registers a sink that does not declare
   itself re-entrant and asserts that it is never entered twice at once,
   while a re-entrant sink on the same dispatcher is.
3. **A sink that logs from inside its own `log()` is still stopped** from
   recursing into itself. Host test.

## Fix, 2026-11-23

- `LogSink::reentrant()` (default `false`). A record reaches every
  re-entrant sink directly, in the context that raised it, with no guard; the
  others still go one at a time behind the dispatch guard, and a record that
  finds it held waits in the hold queue for THEM only (each held record
  remembers what it is still owed, so nothing reaches a sink twice).
- The bound on a re-entrant sink that logs from inside its own `log()`:
  `REENTRANT_DEPTH` (8) direct deliveries in progress at once. Past it, a
  record takes the guarded path for every sink, and a held record's
  re-entrant delivery counts as the whole depth, so whatever such a sink logs
  is held and the drain passes bound it. That costs nothing for a sink that
  does not log: the counter is only spent by deliveries in progress.
- `PlatformSink` is re-entrant where the linked port says so: the
  `nros-platform-cffi/reentrant-log-write` feature, which `platform-zephyr`
  enables. The Zephyr sink's `CONFIG_ASSERT` check that it is never entered
  more times than it has buffers is gone (it encoded the old one-at-a-time
  delivery). Its pool default is now 2 per CPU (2, 4, 8, 16 for 1, 2, 4, 8
  CPUs), 1280 bytes a buffer.

Host tests (`packages/core/nros-log/tests/`):

- `reentrant_sink_concurrent`: 6 threads x 2,000 records into one
  re-entrant and one serialised sink. The re-entrant sink gets all 12,000 with
  several threads inside it at once; the serialised one is never entered twice
  and gets every record or counts it lost. 0 failures in 200 runs. With the
  direct path disabled (mutant) the re-entrant sink got 1,078 of 12,000.
- `reentrant_sink_logs`: a re-entrant sink that logs on every write is
  stopped: 27 lines from one record, against a derived bound of
  1 + 8 + 9 x 2.

Still owed for acceptance 1: the stress cell on `qemu_cortex_a53/smp`.

