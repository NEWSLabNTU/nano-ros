---
id: 956
title: "Four W4 parity slots are declared, unfilled, and undecided: content
  filtering and network flow endpoints"
status: resolved
type: tech-debt
area: rmw
related: [phase-406, phase-407, 0800]
---

## Problem

Four vtable slots have no producer and no consumer, and — unlike the other ten
inert slots — no decision behind them:

```
subscription_set_content_filter          subscription_get_content_filter
publisher_get_network_flow_endpoints     subscription_get_network_flow_endpoints
```

`check-rmw-slot-producers.py`'s `INERT_FAMILIES` records why each landed:

* **content-filter** — "DDS content-filtered topics. Landed in W4 for shape
  parity; no backend implements filtering and the runtime never asks."
* **network-flow** — "reporting the transport's endpoints. Landed in W4 for
  shape parity; diagnostic only, and nothing diagnoses."

Read those against the other inert families and the difference is sharp. The
others say what answers the capability INSTEAD — `set_wake_callback` for the
per-entity callback trio, the message attachment for the `_with_info` takes, the
runtime for identity. These four say only "nothing implements it", which is a
description of the present, not a decision about the future.

## Why it needs an id rather than a shrug

phase-406 added a `status` axis to `docs/reference/rmw-api-map.toml`, and its
rule is that `not-implemented` MUST carry an issue. The reason is the failure
mode it replaces: with `status` absent, an unfilled slot was counted as
ANSWERED — 15 of them were — so a gap and a design were indistinguishable, and
the gap quietly became the design by never being written down.

These four are the honest `not-implemented` set. The other ten inert slots are
`re-mapped` (capability elsewhere, slot kept for parity) or `not-supported`
(a decision — blocking acks do not fit a decomposed wait; graph guard
conditions are a platform primitive nothing consumes).

## What would resolve this

Either is a fine outcome; leaving it unstated is not:

1. **Implement**, in the backend that can. Cyclone has content-filtered topics
   natively and can report network flow endpoints; zenoh-pico and XRCE have
   neither, so they stay NULL and the ABI's nullity contract already covers it.
2. **Decide against**, and move them to `status = "not-supported"` with the
   constraint named — at which point the slots should probably go, since a
   declared slot nothing will ever fill is the shape issue 0800 was written
   about.

The one thing that must not happen is a third phase in which they are still
declared, still unfilled, and still unexplained.

## Resolution, 2026-08-31 — DECLINED, and the four slots are deleted

Option 2 of the two above, for both families, and the deciding fact for content
filtering was measured rather than argued:

**Our pinned Cyclone cannot do it.** 0.10.5 exposes `dds_set_topic_filter`,
`dds_set_topic_filter_and_arg` and `dds_set_topic_filter_extended` — a LOCAL
sample callback applied after the message arrives — and NOT
`dds_create_topic_filtered`, the one that propagates the predicate to the writer.
So "implement it on the backend that can" had no backend. What was available was
a different feature under upstream's name: it would promise saved BANDWIDTH and
deliver saved CPU. That is the shape of deviation reason W4 spent its time
undoing, so it was not worth having.

**Network flow endpoints** are diagnostics whose value is in a multi-homed
deployment with dynamic discovery — the case least like this ABI's target, where
one interface and a static config mean the answer is in the file you already
wrote. No consumer in-tree. Declined, with the note that Cyclone-only support is
cheap to revisit if `ros2` tooling ever needs to introspect a nano-ros node: it
is read-only and has no wire effect.

Deleted rather than left NULL, per issue 0800: a declared slot nothing will ever
fill reads as capability, and that is exactly how these four came to be counted
as answered for two phases.
