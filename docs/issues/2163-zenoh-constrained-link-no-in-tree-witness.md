---
id: 2163
title: "Graph discovery off and the transient-local subscriber's history query (phase-473 W1/W2) have no in-tree live test"
status: open
type: test-gap
area: [rmw-zenoh, interop, testing]
severity: low
found: 2026-10-30
related: [1806, 1341, phase-473]
---

## Measured

[phase-473](../roadmap/archived/phase-473-zenoh-constrained-link-graph-and-latched-reads.md)
landed two `nros-rmw-zenoh` behaviours, and both were measured only on the
safety island's QEMU image, in the island's own repository:

* **W1** — `ZPICO_GRAPH_DISCOVERY=0` compiles the graph cache out, so the router
  sends the image no `@ros2_lv` history and the heap stops growing with the host
  graph.
* **W2** — a TRANSIENT_LOCAL subscription issues one `<key>/@adv/**` history
  query (with `Z_REPLY_KEYEXPR_ANY`), so a late joiner reads a stock latched
  publisher's value.

On `main` (2026-10-30), `git grep` over `packages/testing/` finds:

* no test, fixture row or example image built with graph discovery off;
* no test in which a nano-ros SUBSCRIBER late-joins a stock `rmw_zenoh_cpp`
  latched publisher. The only late-joiner test is the other direction —
  `qos_override_e2e::a_late_joiner_receives_the_declared_history`, a stock
  `ros2 topic echo` reading a nano-ros publisher's cache (phase-455 W5).

So a regression in either — the `_anyke` line W2 found it needed, the
latched-answer path of `zpico_graph_cache_start`, the define delivery — turns no
lane red. W2's own record shows how quietly it fails: without `_anyke` the router
propagated only the final reply and the value simply never arrived.

## Closes when

A live cell (`interop::CELLS`) on the native board:

1. a stock `rclpy` / `rmw_zenoh_cpp` process publishes a value ONCE, latched,
   alongside a synthetic graph of ~136 nodes (issue 1806's Autoware scale);
2. a nano-ros image built with graph discovery off, transient-local
   subscription, starts AFTER that publish;
3. it receives the value, and the router sends it no `@ros2_lv` traffic after
   its own declarations (a tap or the router's debug log can count it).

With negative controls: the same image with discovery ON receives the graph
burst, and a VOLATILE subscription does not receive the value.

This does not close [issue 1806](1806-zenoh-constrained-link-unvalidated-on-full-autoware-graph.md),
whose acceptance is the island's real Autoware domain; it makes the mechanism
a regression gate in this repository.
