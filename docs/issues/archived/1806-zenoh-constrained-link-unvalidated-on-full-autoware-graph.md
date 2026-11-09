---
id: 1806
title: "The constrained-link zenoh changes (graph discovery off, latched reads) have never run against the full Autoware graph"
status: resolved
resolved: 2026-11-09
resolved_in: "phase-512 (board runs z1806-lj1 and w1, 2026-11-09)"
type: test-gap
area: [rmw-zenoh, interop]
severity: low
found: 2026-10-11
related: [1341, 2163, 2366, phase-474, phase-512]
---

**Home: [phase-512](../../roadmap/phase-512-on-hardware-measurements.md)**.

## What was open

[phase-473](../../roadmap/archived/phase-473-zenoh-constrained-link-graph-and-latched-reads.md)
landed W1 (`NROS_ZENOH_GRAPH_DISCOVERY`, #1435) and W2 (one `<key>/@adv/**`
history query per TRANSIENT_LOCAL subscription, #1437). Both were measured on
the island's QEMU image against a 100-node synthetic graph only. The
acceptance was the island's real Autoware domain over its serial link: the
image survives joining it with graph discovery off, and a late-joining island
reads `/api/operation_mode/state` before the next change.

## Resolved 2026-11-09 on the MR-CANHUBK344

Image: the safety island's `build-board` image (island main 04fd11e, nano-ros
ac23c0155b, zephyr.elf sha256 5cfb4091b53f6891), 921,600 baud serial with
`UART_ASYNC` DMA RX, behind the island gateway router (a stock `rmw_zenohd`,
demo f7c60e1) and Autoware 1.5.0 on `rmw_zenoh_cpp`.

**W1 is in the image.** Kconfig derives `CONFIG_NROS_ZENOH_GRAPH_DISCOVERY=n`
for a serial-only image, CMakeCache records
`NROS_RESOLVED_NROS_ZENOH_GRAPH_DISCOVERY=0` (rung `kconfig`), and all 430
compile commands carry `-DZPICO_GRAPH_DISCOVERY=0`. The image holds the
"graph discovery is compiled out" log string (the board's console is unwired,
so the line itself is not visible).

**The domain.** `ros2 node list` during the runs: 140 nodes (z1806-lj1:
play_launch's 113 Autoware members, the gate, probe and relay, the 3 island
nodes) and 137 nodes (w1). The CARLA act nl-cb1 (2026-11-07, same nano-ros
pin) runs 204 Autoware members (63 nodes, 16 containers, 125 composables).

**W1 at that scale, without the gateway's safety net.** The demo gateway
denies `@ros2_lv/**` liveliness tokens toward the island (an ACL older than
W1), so the acts alone do not show W1 holding. Run w1 used the same gateway
config with that ACL removed, so only W1 stands between the domain and the
board. Island serial counters, read over SWD without halting, with the
planning simulator up (137 nodes):

| window | rx bytes | rx frames | rate |
| --- | ---: | ---: | ---: |
| quiet, 20.5 s | 148,659 | 821 | 7.25 KB/s |
| 40 extra host nodes declared, then destroyed (3 pubs, 2 subs, 1 service each), 21.6 s | 156,673 | 867 | 7.25 KB/s |
| quiet again, 20.5 s | 148,784 | 824 | 7.25 KB/s |

Graph churn of 40 nodes adds nothing on the wire to the island. Before
Autoware started the island had received 299 B in 13 frames; the 113-member
startup brought data samples only. Heap peak 67,720 of 94,720 B (41,232 at
FirstSpin) with `CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL=y`; 0 overruns, 0 bad
frames, ring high water 302 B.

**The acts, with the deployed gateway.** z1806-lj1 (encore, late join,
2026-11-09): heap peak 68,272 of 94,720 B, 0 overruns / 0 bad frames before
and during the act, ring high water 1,341 B, one island transport for the
run. nl-pe1 (encore, PASS): 68,272 B, 0/0, 1,006 B. nl-cb1 (CARLA b, PASS,
204 Autoware members): 68,456 B, 0/0, 1,106 B.

**W2: the late joiner reads the latched value.** z1806-lj1 used run-board's
`--late-join`: the core is held halted until Autoware, the gate and the probe
are up, then reset. A VOLATILE host reader of `/api/operation_mode/state`
stamped every live publication: one at Autoware's startup (mode STOP) 6.9 s
BEFORE the island's reset, the next 19.7 s after it (the act's initial pose).
The island's `mrm_handler` subscribes TRANSIENT_LOCAL and cannot leave INIT
without an operation-mode sample; its SWD record reads
`handler armed the monitors: INIT_DONE at uptime 1313 ms`. No live
publication fell in that window, so the sample came from the publisher's cache
through the history query, 1.3 s after boot.

That act ended with no verdict: the island supervisor's route 1 latched its
safe-state output at the same tick (uptime 1313 ms), so the vehicle never
moved. That is the island's late-join handling, not this transport; it is
reported to the island.

## Not done here

Per-publisher late-joiner detection, and re-issuing the history query when the
session re-opens, are [issue 2366](../2366-transient-local-history-query-is-issued-once.md).
