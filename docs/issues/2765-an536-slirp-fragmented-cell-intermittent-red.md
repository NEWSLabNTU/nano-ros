---
id: 2765
title: "The AN536 Cyclone fragmented-sample ros2-interop cell went red twice on 2026-11-11 over slirp, where the NIC cannot drop a fragment; cause unknown"
status: open
type: bug
severity: low
area: [rmw, platform, testing]
related: [issue-0917, issue-0889, issue-2820]
found: 2026-11-20
---

## What happened

The ros2-interop lane runs
`pubsub_freertos_an536_cyclone_ros2_interop_e2e::nros_freertos_an536_cyclone_listener_keeps_receiving_a_fragmented_sample`
(mps3-an536 FreeRTOS, Cyclone, slirp, `-icount shift=auto`, a host ROS 2
publisher sending a 10,576-byte sample at 10 Hz) on fixtures it builds in the
same run. It failed twice on one day:

| run | date | head | verdict | host publisher sent | listener received |
| --- | --- | --- | --- | --- | --- |
| 42957501616 | 2026-11-11 | eb248aec9f | FAIL (9.6 s) | 32 | 0 |
| 42978245897 | 2026-11-11 | 684011c5be | FAIL (77.7 s) | 750 | 0 |

Both are the binary shape: the guest reports `Network ready` and its stack and
heap peaks, and no sample arrives at all. Neither run retried the cell.

## Since then

The cell did not run on 2026-11-12 or 2026-11-13. From 2026-11-14 to
2026-11-20 it passed on the first try in seven runs in a row (43555266126,
43675564346, 43802612787, 44015390025, 44235514551, 44456527279, 44672701128;
77-90 s each). It was never reproduced solo on a workstation.

## 2026-11-21: red again, with a different shape and a known cause

Run 44864729982 (head 3b6959ca7f) failed this cell with `1 received of 751
sent`, and `..._keeps_up_with_a_line_rate_burst` in the same run delivered
`1 of the 380` samples. One sample then silence is not the 2026-11-11 shape
(none at all). That head carries 6229085857, which made every image serve
the parameter services, and not be27dad878 (#4589), which removed the four
64 KiB stack frames in Cyclone's service path that those services reach on
this board (issue 2820). The 2026-11-11 heads carry neither, so the frames
were not on their path and are not their cause.

The next run, 44983865499 (head 94a190df4e, which has #4589), passed every
AN536 Cyclone cell, this one in 78.1 s. So the 2026-11-21 red is issue
2820's, and this issue's own cause is still unknown and still unseen since
2026-11-11.

## Why this is not issue 0917

Issue 0917 was a late RX drain losing the tail of a fragment burst at the
NIC. That cannot happen on this cell: the pinned QEMU's LAN9118 model
back-pressures a full RX FIFO instead of dropping a frame, so over slirp a
late drain costs latency and never a fragment. Issue 0917's lane only shows
the drop through a paced relay that puts the overflow back. Whatever lost
every sample on 2026-11-11 sits elsewhere: discovery, the reader's
reassembly or history, or the host side.

## Not this issue

The island's freshness residual above lwIP (the executor's take rate on an
emulated Cortex-R52 under a fragmented stream) is issue 0889.

## Direction

Watch the cell. If it goes red again, keep the run's artifacts. The 9.6 s
failure ended early, which points at discovery or matching rather than
reassembly. Capture the guest console, a Cyclone discovery trace on the host
peer, and a pcap of the slirp side, then decide between discovery and
reassembly. Close this as not reproduced if the cell stays green through
2026-12-20.

## No recurrence (2026-11-24)

The failed runs among the last 15 of `platform-matrix`, `host-suite`, `ros2-interop`
and `nightly` (12 failed runs, 2026-11-14 to 2026-11-23, read with
`gh run view --log-failed`) were scanned for this cell:

- The `platform-matrix` hits (7 lines, `FAIL [0.004-0.008s]`) are `[SKIPPED:lane]`
  panics in a bare nextest run: the cell is out of that lane, so it was never
  run there. They are not reds.
- The only real failure is ros2-interop 44864729982 on 2026-11-21, already
  attributed above to issue 2820, which #4589 fixed.
- The cell passed in every ros2-interop run since, including the scheduled
  45324656188 on 2026-11-24.

The 2026-11-11 shape (no sample at all) has not been seen since 2026-11-11.
The close rule above stands: not reproduced if green through 2026-12-20.
