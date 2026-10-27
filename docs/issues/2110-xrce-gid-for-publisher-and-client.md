---
id: 2110
title: "XRCE answers get_gid_for_publisher / get_gid_for_client UNSUPPORTED, though graph.c already computes each endpoint's DDS GID"
status: open
type: enhancement
area: [rmw, xrce]
severity: low
found: 2026-10-27
related: [1292, 1495, 1978]
---

## What is missing

Both gid slots are NULL for XRCE. The XRCE protocol itself hands the client no
GUID for an entity it creates — but issue 1292 already solved that for
`ros_discovery_info`: `src/graph.c` learns the participant's GUID prefix with a
self-addressed probe and numbers each endpoint from Fast-DDS's per-participant
counter, and `xrce_graph_endpoint` holds the key and kind for every tracked
publisher, subscription, service and client. `gid_of()` turns those into the
24-byte GID a stock peer prints in `ros2 topic info -v`.

So the slots are implementable with no new protocol work: for a publisher, its
`graph_writer`; for a client, its request writer (`graph_writer`). Answer
UNSUPPORTED exactly when the endpoint is not tracked (attribution off after an
unanswered probe, or the count unknown after a failed create) — never a
fabricated prefix.

Stock `rmw_microxrcedds` copies the XRCE object id into the gid
(`rmw_get_gid_for_publisher.c`), which is client-local and matches no DDS GID;
answering the DDS GID is strictly more useful, and it is the same bytes this
backend already publishes, so the gid a caller is told and the gid a peer sees
agree (the issue-1495 rule). The identifier stamped beside it is `"xrce"`.

## Acceptance

`graph_sample.c`-style no-Agent test: a tracked endpoint's gid equals
`gid_of(prefix, key, kind)`; an untracked one answers UNSUPPORTED. Live, the
issue-1292 rig's `ros2 topic info -v` GID equals `get_gid()`.
