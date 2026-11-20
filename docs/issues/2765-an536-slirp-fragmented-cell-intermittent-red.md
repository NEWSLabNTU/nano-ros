---
id: 2765
title: "The AN536 Cyclone fragmented-sample live-peer cell went red twice on 2026-11-11 over slirp, where the NIC cannot drop a fragment; cause unknown"
status: open
type: bug
severity: low
area: [rmw, platform, testing]
related: [issue-0917, issue-0889]
found: 2026-11-20
---

## What happened

The live-peer lane runs
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
