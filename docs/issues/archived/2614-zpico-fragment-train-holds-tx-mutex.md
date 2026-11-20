---
id: 2614
title: "zenoh-pico `TX_SPLIT_LOCK`: a fragmented send and a parked overflow's inline drain hold `_mutex_tx` for the whole write, so a publish on the S32K344 serial link waits ~58 ms at p99"
status: resolved
type: performance
area: [rmw, zenoh]
severity: low
found: 2026-11-15
related: [rfc-0110, 2575, 2596]
resolved: 2026-11-20
resolved_in: "zenoh-pico fork d7d0094c (nano-ros-1.10.1)"
---

**Standalone.** No active phase owns the zenoh-pico TX path. Filed from the
board run that checked [issue 2596](2596-zpico-split-lock-defeated-by-session-transport-mutex.md).
**Resolved 2026-11-20**: the open questions are answered below with board
numbers, and the fix is on the fork's patch line.

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

## Answers (2026-11-20, CANHUBK344)

The island's board image at its nano-ros pin 3c2e8cf685 (zenoh-pico
19a47ae7, issue 2596's fix), with the same test publishers as the 2026-11-15
run: A, 64 B every 20 ms, every `zpico_publish` timed; B, once a second,
alternately 8 x 900 B (MED: each overflows the batch, so a parked batch is
drained inline) and 2 x 3000 B (BIG: each is a fragment train). Four 80 s
runs per build, the record read over SWD, a host subscriber checking every
sequence number. A measurement-only probe in `tx.c` timed each write and each
wait for `_mutex_tx`.

**Where the 58 ms goes.** It is neither the train nor the drain alone; it is
every write that happens while `_mutex_tx` is held, and each of those waits
for the 4 KiB UART transmit ring, which drains at 921,600 baud (~44 ms when
full). Per run, before the fix:

| write under `_mutex_tx` | count | longest | >= 20 ms |
| --- | --- | --- | --- |
| fragment train (BIG) | 86-88 | 45.6-68.2 ms | 43-44 |
| parked batch drained inside a train | 44-46 | 0.1 ms | 0 |
| parked batch drained inline by an overflow (MED) | 214-217 | 43.8-44.5 ms | 42-43 |
| express flush (declarations, express publishers) waiting for the link | ~5,300 | 44.6-44.7 ms | 3-6 |

So in a MED burst A waits out the overflow's inline drain, in a BIG burst
the train itself; the parked batch a train drains first is small. Any write
under the mutex costs every other publisher the same.

**Can a train leave `_mutex_tx` without breaking 2575's order?** Yes, if it
takes the link mutex before it lets `_mutex_tx` go. The receiver's
defragmentation needs a train unbroken and its SNs consecutive, so the train
reserves all its SNs first (it counts its fragments ahead of cutting them),
takes the link mutex, releases `_mutex_tx`, then drains the parked batch and
writes the fragments from the spare buffer. Anything encoded later has a
later SN and needs the link after the train. Releasing between fragments
would not work: a frame written between two fragments breaks the train at
the receiver.

The same rule then covers the other three: an overflow or an express flush
that would wait for a busy link undoes its encode, releases `_mutex_tx`,
waits for the link and starts over (a parked batch in the way ships first,
under the link mutex alone); an express flush on a free link writes after
releasing `_mutex_tx`, as the batch flush does; the periodic flush only tries
the link (`zp_batch_flush_try`) and leaves the batch when it is busy.

| | 19a47ae7 (before) | + d7d0094c (after) |
| --- | --- | --- |
| A publish p99, all samples | 26.8-29.1 ms | 174-179 us |
| A publish max | 57.5-60.0 ms | 0.2-35.2 ms |
| A p99 during a MED burst | 57.5-60.0 ms | 165-183 us |
| A p99 during a BIG burst | 52.8-55.2 ms | 168 us-2.6 ms |
| A publish p50, uncontended | 170-171 us | 168-169 us |
| waits >= 20 ms for `_mutex_tx`, all publishers, per run | 122-288 | 0-2 |
| samples lost or out of order at the host (A, B, island) | none | none |
| BIG samples arriving whole at the host, per run | 86-90 | 82-90 |
| stall, reset, fault | none | none |

The writes themselves are as long as before (they are the line rate); what
changed is that no publisher waits for one it did not make. A publisher whose
own sample finds both batch buffers full still waits, by design: there is no
third buffer. That is what the remaining max (up to 35 ms, once per run or
less) is.

Host side, `tests/z_tx_split_lock_order_test.c` on the fork (fake link,
20 ms per write) checks the same four paths and that every write's SN is
exactly one more than the one before it; before / after: 80 ms / 0 ms for a
small publish during a train, 95 ms / 0 ms behind an express sample, p90
35.3 ms / 0.02 ms during overflow drains.

Logs, probe and scripts: `/mnt/mx500/aeon/worktrees/k2614-logs` on the board owner's host
(`base*` before, `fxd*` after).
