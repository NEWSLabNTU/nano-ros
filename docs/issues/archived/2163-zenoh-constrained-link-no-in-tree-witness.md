---
id: 2163
title: "Graph discovery off and the transient-local subscriber's history query (phase-473 W1/W2) have no in-tree live test"
status: resolved
resolved: 2026-10-31
type: test-gap
area: [rmw-zenoh, interop, testing]
severity: low
found: 2026-10-30
related: [1806, 1341, phase-473]
---

## Measured

[phase-473](../../roadmap/archived/phase-473-zenoh-constrained-link-graph-and-latched-reads.md)
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

This does not close [issue 1806](../1806-zenoh-constrained-link-unvalidated-on-full-autoware-graph.md),
whose acceptance is the island's real Autoware domain; it makes the mechanism
a regression gate in this repository.

## Resolution

`interop_e2e::interop::case_13_zenoh_constrained_link_latched_late_join`, on
the existing `native-pubsub-rust-zenoh-n2r` cell (ROS-2-to-nano, like
case_02), with its nano side in the new
`packages/testing/nros-tests/bins/constrained-link-probe`. One program, two
images (fixture rows `constrained-link-probe-zenoh` and
`constrained-link-probe-no-graph-zenoh`); `linux_no_graph` states
`[image.linux_no_graph.knobs.zenoh] graph_discovery = false`, which reaches the
build as `NROS_ZENOH_GRAPH_DISCOVERY = "0"`.

The stock side is `Ros2Process::latched_int32_with_graph`: one `rclpy` process
on `rmw_zenoh_cpp` that publishes one Int32 latched (TRANSIENT_LOCAL,
KEEP_LAST(1), RELIABLE) next to 135 filler nodes, 136 in all. Every probe
starts after it prints that it has published.

1. Discovery OFF, TRANSIENT_LOCAL: receives the value exactly once (W2), and
   its graph query answers Unsupported (W1).
2. Discovery ON, TRANSIENT_LOCAL (control): receives the value and lists the
   graph, at least 20 nodes.
3. Discovery OFF, VOLATILE (control): receives nothing.

How criterion 3 of "Closes when" is met: not by counting `@ros2_lv` bytes on a
tap. The graph cache's liveliness subscriber is declared lazily, on the first
graph query, so an image that never asks receives no liveliness traffic in
either build. The cell therefore asks once, after the receive window, and a
build that answers Unsupported has no subscriber for the router to send to.
The discovery-on control lists 44 of the 136 nodes with the default
`NROS_GRAPH_CACHE_SIZE`; the cache counts overflow rather than storing it, so
the control asserts that the burst arrived, not its size.

Measured (ros2 distrobox, Humble): pass in 21 s. Negative controls, each
measured and reverted:

* `query_history` in `shim/subscriber.rs` returning before it sends: case 1
  fails at the W2 assertion.
* `linux_no_graph` built with `graph_discovery = true`: case 1 fails at the W1
  assertion.

Issue 1806 stays open: its acceptance is the island's real Autoware domain.

