---
id: 2614
title: "zenoh-pico `TX_SPLIT_LOCK`: a fragmented send and a parked overflow's inline drain hold `_mutex_tx` for the whole write, so a publish on the S32K344 serial link waits ~58 ms at p99"
status: open
type: performance
area: [rmw, zenoh]
severity: low
found: 2026-11-15
related: [rfc-0110, 2575, 2596]
---

**Standalone.** No active phase owns the zenoh-pico TX path. Filed from the
board run that checked [issue 2596](archived/2596-zpico-split-lock-defeated-by-session-transport-mutex.md),
and not started: whether this is worth fixing is still open.

## What happens

On the S32K344 safety island (`TX_BATCH` + `CONFIG_NROS_ZENOH_TX_SPLIT_LOCK=y`,
serial at 921600 baud, a 4 KB UART ring), the p99 publish latency is about
53–58 ms. It does not move with issue 2596's fix, which took the detached
batch write off the publishers' path. The board owner's reading of the run:
the ~58 ms is spent holding `_mutex_tx` for two writes that still happen
under it:

- a fragmented send writes its whole fragment train before it releases the
  lock;
- a parked overflow batch is drained inline, under the same lock.

Both are on issue 2575's path, which made a fragmented send take the link
lock for ordering. A publisher that arrives during either one waits the
write out, which is the stall split lock exists to remove.

## Measured (2026-11-15, CANHUBK344, four 80 s runs per build)

| | 01f5761a | 19a47ae7 (2596's fix) |
| --- | --- | --- |
| publish p99 | ~53–58 ms | ~53–58 ms |
| publish p50, uncontended | 148 µs | 162 µs |
| stall, reset, fault, samples lost | none | none |

Logs: `/mnt/mx500/aeon/worktrees/k3937-logs` on the board owner's host.

## Open questions before any fix

- How much of the 58 ms is the fragment train and how much is the inline
  drain. The run reports them together.
- Whether a fragment train can leave `_mutex_tx` between fragments without
  losing the ordering issue 2575 fixed (a fragment must not overtake an
  older parked batch).
- Whether the island needs it at all: its topics ran at rate with nothing
  lost in every run.
