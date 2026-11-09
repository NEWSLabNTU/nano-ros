---
id: 913
title: "attaching pyocd RTT kills the zenoh session -- the debugger perturbs the
  system under test, and issue 0879 makes the perturbation permanent"
status: resolved
resolved: 2026-11-09
resolved_in: "phase-512 (board run w1, 2026-11-09): survives halts under the lease, recovers by itself past it"
type: bug
area: testing, embedded
related: [issue-0852, issue-0879, issue-0839, phase-478, phase-512]
---

**Home: [phase-512](../../roadmap/phase-512-on-hardware-measurements.md)** (moved
2026-11-09 from [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)).

## What was open

On the S32K344, attaching `pyocd rtt` made the image vanish from the ROS 2
graph, detaching did not bring it back, and only a router restart did
([issue 0879](0879-serial-link-has-no-resync-after-peer-reset.md), resolved
since). Memory reads over the debug port were taken to halt the core, so every
observation made through RTT was suspect.

## Resolved 2026-11-09: measured on the MR-CANHUBK344

Image: the safety island's `build-board` image (island main 04fd11e, nano-ros
ac23c0155b), 921,600 baud serial with `UART_ASYNC` DMA RX,
`CONFIG_NROS_ZENOH_LEASE_MS=60000`, behind the island gateway (a stock
`rmw_zenohd`, lease 60 s, keep_alive 6) and Autoware's planning simulator
(137 nodes). A host reader stamped every sample of the island's
`/system/mrm/emergency_stop/status` (30 Hz); the gateway's transport log shows
whether the session was kept. The image has no RTT control block (no
`_SEGGER_RTT`), so the instruments are SWD memory access and `pyocd rtt`
itself.

**Reads without halting do not perturb the session.** pyocd in attach mode
reads memory through the AHB-AP while the core runs. 60 s of back-to-back
reads (5,156 reads, 10.7 MB): the stream stayed at 30.3 /s, largest gap
65.5 ms, no UART overrun.

**Halts shorter than the lease are survived.** The core halted for h seconds,
resumed, 25 s between steps:

| halt | session | first sample after resume | rate after |
| ---: | --- | ---: | ---: |
| 0.30 s | kept | 5.5 ms | 30.4 /s |
| 1, 2, 5, 10 s | kept | 64, 42, 9, 20 ms | 30.4 /s |
| 20, 30, 45 s | kept | 34, 34, 15 ms | 30.3-30.4 /s |
| 55.08 s | kept | 39 ms | 30.4 /s |
| 60.06 s | closed by the gateway at the end of the halt | recovered on its own (below) | |
| 65.07 s | closed by the gateway 60.3 s into the halt | 70.2 s | 30 /s |

A halt costs at most one UART overrun and one bad frame (one frame lost):
0 for the 0.30 s halt, 1 for each halt of 0.38 s and longer. `pyocd rtt -t
s32k344` scanned RAM for 2.1 s with the core stopped ("Control block not
found"); the stream had one 2,118 ms gap and the session was kept.

**The threshold is the lease, on the router's side.** Kept at 55.08 s, closed
at 60.06 s: the gateway expires the island after the 60 s lease the island
announces, with nothing received. The island never expires first, because its kernel
clock stops while the core is halted: after the series, 1,047.5 s of host time
since reset read as 663.0 s of island uptime, the 384.5 s difference being the
383.7 s of halts plus the readout's. Its lease and keep-alive timers do not see
a halt at all.

**Past the lease the session recovers by itself.** After the 65.07 s halt the
island noticed the silence by its own lease and re-opened; the gateway logged a
new client transport and samples resumed 70.2 s after the resume (135 s of
outage), with no router restart. That is 0879's fix (the reopen sends one INIT,
not a flood) on the serial link; the first report predates it.

**The demo's acts already depend on this.** Each board act opens three SWD
sessions while the island's session is live: the serial counters at the act's
start (attach, no halt), the trace re-arm (attach, no halt; in the CARLA act
nl-cb1 it stays attached about 75 s through the act), and the readout after
the verdict, halted 0.326-0.377 s (nl-pe1 0.343, nl-cb1 0.334, z1806-lj1 0.326,
w1 0.377). Those acts pass.

## What still holds from the first report

A halt does stop the image: a probe halt longer than a monitored deadline is a
fault the island reports, and a halt past 60 s costs about two minutes of
session. Read with attach mode and no halt when the session matters; keep a
halted read short (the readout's 0.33 s is), and never hold the core near the
lease.
