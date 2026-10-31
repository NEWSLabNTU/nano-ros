---
id: 1806
title: "The constrained-link zenoh changes (graph discovery off, latched reads) have never run against the full Autoware graph"
status: open
type: test-gap
area: [rmw-zenoh, interop]
severity: low
found: 2026-10-11
related: [1341, 2163, phase-474]
---

**Home: [phase-474](../roadmap/phase-474-safety-island-board-findings.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## Attempted 2026-10-30: blocked here, and why

Re-checked from fresh `main` on the box that runs this repository's lanes.
The closing run cannot happen on it, for three measured reasons:

* **No island checkout.** No `simple-autoware-safety-island` /
  `autoware-safety-island` tree exists under the project roots, so its
  `demo/l3` setup (where the `l3-autoware` run lives) is not here.
* **No Autoware container.** `docker images` lists `ubuntu:22.04` only; the
  `l3-autoware` image the run boots against is not built or pulled.
* **No in-tree substitute already exists to fall back on.** `git grep` over
  `packages/testing/` finds no test that builds an image with
  `ZPICO_GRAPH_DISCOVERY=0` (phase-473 W1), and none in which a nano-ros
  SUBSCRIBER reads a stock latched publisher's history through the `@adv`
  query (W2). The in-tree late-joiner coverage is the other direction
  (`qos_override_e2e::a_late_joiner_receives_the_declared_history`: a stock
  `ros2 topic echo` reading a nano-ros publisher's cache). Both W1 and W2 were
  measured only on the island's image, against a 100-node synthetic graph.

What the scale question rests on today, so nobody re-derives it: phase-473
measured discovery OFF against 100 `rclpy` nodes (3 publishers, 2
subscriptions, 1 service each) at a heap peak of 105,368 bytes and **zero**
`@ros2_lv` bytes router→island after the island's own declarations, against
ON exhausting a 1 MiB heap in 2.5 s. With the cache compiled out, the router
sends the image nothing that scales with the domain, so 136 nodes is not
expected to differ from 100 for W1 — but "not expected" is the claim this issue
exists to replace with a run. W2 is a single query per transient-local
subscription and its reply count does not depend on the graph size either; the
part a full graph can still break is the 5 s query timeout under a busy router.

An in-tree witness for W1/W2 (a native image built with graph discovery off,
late-joining a synthetic ~136-node `rmw_zenoh_cpp` graph and reading a latched
value) would make the mechanism a regression gate here. It does NOT close this
issue — the acceptance is the island's real Autoware domain — so it is tracked
separately, as [issue 2163](archived/2163-zenoh-constrained-link-no-in-tree-witness.md).

## Closes when

The island's `l3-autoware` run boots the image with graph discovery off, the
image survives joining the Autoware domain, and a late-joining island reads
`/api/operation_mode/state` before the next change is published. Record the
run (date, island commit, nano-ros commit) here.
