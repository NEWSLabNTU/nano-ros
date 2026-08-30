---
id: 927
title: "Cyclone's `ros_discovery_info` reader enumerates its OWN node and never
  a live ROS 2 peer"
status: open
type: bug
area: rmw
related: [phase-381, issue-0903, issue-0791]
---

## Measured

Distrobox `ros2`, ROS 2 Humble, `rmw_cyclonedds_cpp`, its own domain, stock
`demo_nodes_cpp talker` confirmed up and confirmed VISIBLE to ROS itself:

```
talker_publishing=1
=== ros2 node list (cyclone, domain 79) ===
/talker
=== nros cyclone probe (domain 79) ===
GRAPH_NODE /|graph_probe
GRAPH_PROBE_NODE_COUNT 1
GRAPH_PROBE_FAIL: expected a node matching "talker", saw ["/|graph_probe"] after 15000 ms
probe_rc=2
```

So the reader WORKS — it enumerates a node — and the node it enumerates is
always and only our own. 15 s, and `ros2 node list` on the same domain answers
`/talker` in under one.

Reproduced by `cyclone_enumerates_a_stock_ros2_node` (`graph_interop.rs`, cell
`native-graph-rust-cyclone-r2n`), which is where it was found.

## What is already ruled out

* **Not the peer.** `ros2 node list` sees `/talker` on the same domain, in the
  same container, at the same time.
* **Not the probe's convergence.** That WAS a defect and is fixed: "non-empty
  and stable" settled instantly on our own node, and the loop now requires the
  expected node before converging. The measurement above is from the fixed
  probe, polling the full budget.
* **Not the wrong binary.** Two `[[fixture]]` rows build a `graph-probe`; the
  zenoh one aborts on a cyclone domain with no router (`rc=134`), which reads as
  a cyclone defect. The run above is the CYCLONE build, identified by symbol.
* **Not the topic or type name.** `graph.cpp` uses `ros_discovery_info` and
  `rmw_dds_common::msg::dds_::ParticipantEntitiesInfo_`, matching stock.
* **Not reader QoS on its face.** RELIABLE + TRANSIENT_LOCAL + KEEP_LAST(
  kMaxSamples), deliberately matching the writer so a late reader gets latched
  snapshots.

## Where to look next

The shape — reads our own writes, never a remote's — is what you get when the
reader and the local writer agree and the REMOTE writer never matches. Worth
checking in this order, with a live peer up:

1. Whether the remote writer is discovered at all (`dds_get_matched_publications`
   on `g->graph_reader`); that separates "not matched" from "matched, sample
   dropped".
2. The type descriptor, not just the type NAME — a differing hash or member
   layout fails matching while both sides spell the type identically.
3. Whether `dds_read` is returning remote samples that then fail to convert.

## Why this was not caught earlier

phase-381 W5 built this reader and nothing ever ran it against a peer. That is
precisely the state zenoh was in until issue 0903, where twelve slots that were
`produced`, mutation-tested and parity-clean did not work at all. The phase doc
predicted it in its own words — every check was against our own builders, our
own parser and our own vtable — and `check-rmw-slot-producers` cannot see it,
because it asks whether a slot has a producer, not whether the producer is
right.

Cyclone also answers only 2 of the 11 graph slots (`get_node_names`,
`get_topic_names_and_types`) and reports the other nine `UNSUPPORTED`. That part
is NOT a bug: it is what W5 built, and W6 requires "cannot tell you" to stay
distinguishable from "nothing is there".
