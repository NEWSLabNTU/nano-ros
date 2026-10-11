---
id: 1806
title: "The constrained-link zenoh changes (graph discovery off, latched reads) have never run against the full Autoware graph"
status: open
type: test-gap
area: [rmw-zenoh, interop]
severity: low
found: 2026-10-11
related: [1341]
---

## Measured

[phase-473](../roadmap/archived/phase-473-zenoh-constrained-link-graph-and-latched-reads.md)
landed both of its backend changes: W1, `NROS_ZENOH_GRAPH_DISCOVERY` (#1435),
and W2, latched reads for a transient-local subscription (#1437). Each was
measured on the safety island's QEMU image (mps2/an385, LAN9118, 1 MiB heap)
against a stock `rmw_zenohd` and `rmw_zenoh_cpp` 0.1.9 host nodes.

One acceptance box was never ticked:

> Against the full Autoware container (the island demo's `l3-autoware`): not
> yet run.

That run is the one that reproduces the failures the phase was opened for: a
domain of about 136 nodes whose liveliness history overran a 921,600-baud UART
and exhausted the heap over TCP. The in-tree measurements used a small host
graph, so they show the mechanism works, not that it holds at that scale.

The run itself lives in the island repository (its `demo/l3` setup), not in
this tree. This issue keeps the open acceptance visible here after the phase
was archived on 2026-10-11.

## Also not done

Per-publisher late-joiner detection. A stock advanced subscriber also watches
the publisher's `@adv` liveliness token; ours issues the history query once,
when the subscription is created. So a latched publisher that appears AFTER the
subscription delivers its first sample live, not from history. phase-473 records
this as out of scope rather than as a defect.

## Closes when

The island's `l3-autoware` run boots the image with graph discovery off, the
image survives joining the Autoware domain, and a late-joining island reads
`/api/operation_mode/state` before the next change is published. Record the
run (date, island commit, nano-ros commit) here.
