---
id: 927
title: "The graph probe ignored `ROS_DOMAIN_ID`, so every Cyclone run enumerated
  only itself — and it was filed as a Cyclone reader defect"
status: resolved
type: bug
area: testing
related: [phase-381, issue-0903, issue-0161]
---

## What it actually was

`bins/graph-probe` built its session with `ExecutorConfig::new(&locator)`, which
does **not** read `ROS_DOMAIN_ID` — that lives on the env-aware path. So the
probe sat on domain 0 whatever the environment said, while
`cyclone_enumerates_a_stock_ros2_node` started its talker on a unique domain
(deliberately: Cyclone discovers by multicast SPDP, and a shared domain would
let another test's participants into the graph under assertion).

A participant alone on domain 0 enumerates exactly one node: itself. Which is
what it reported, every time.

zenoh never showed this. A zenoh cell is keyed by a unique **locator**, not a
domain, so domain 0 was always the right answer there and the same bug was
invisible.

Fixed by reading `ROS_DOMAIN_ID` in the probe and passing it to
`ExecutorConfig::domain_id`. Both interop tests now pass against live peers:

```
PASS cyclone_enumerates_a_stock_ros2_node
PASS nano_ros_enumerates_a_stock_ros2_node
```

## This issue's original diagnosis was WRONG

It was filed as *"Cyclone's `ros_discovery_info` reader enumerates its OWN node
and never a live ROS 2 peer"*, with a "what is already ruled out" list of five
items and three suggested places to look inside `graph.cpp`. **The reader is
fine.** Nothing in `graph.cpp` needed changing, and the ruled-out list — which I
wrote — did not contain the actual cause.

The symptom was consistent with the accusation, and a plausible story
(TRANSIENT_LOCAL RxO incompatibility against a VOLATILE remote writer) was
available and wrong. Recorded rather than rewritten, because "the evidence fits"
is precisely how phase-381 shipped twelve slots that did not work.

## What settled it, in order

Each measurement killed the previous hypothesis. None of it was reasoning about
the code.

1. **`dds_get_matched_publications` = 1**, stable. One match — our own writer on
   our own participant. So this was never a delivery or deserialisation
   problem, and a type-descriptor hash mismatch was out too: that shows as
   *zero* matches, not one.
2. **`dds_get_requested_incompatible_qos_status`: `total=0`.** No writer was
   ever refused for QoS. That killed the durability theory outright.
3. **Both sides on domain 0**: `matched_publications=3`, and the probe
   enumerated real remote nodes (`_ros2cli_daemon_0_…`). The reader worked, so
   the fault was on our side of the domain.

Both probes are committed and env-gated under `NROS_GRAPH_DUMP` rather than
patched in and thrown away — the zenoh side had to re-derive its equivalent
(issue 0903), and these two are what turned a wrong accusation into a fix in
three runs.

## The class

This is issue 0161's family — "the phase-180 split-brain silently ran every
cyclone image on domain 0" — reappearing in a test binary rather than a Kconfig.
A cyclone peer's domain is load-bearing and silent when wrong: it does not
error, it reports an empty graph, which is indistinguishable from a broken
reader unless something asserts a peer must be visible.

Any new native cyclone fixture must take its domain from the environment. A
zenoh-shaped fixture will not catch it, because zenoh does not use one.
