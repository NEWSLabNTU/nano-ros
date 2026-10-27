---
id: 2110
title: "XRCE answers get_gid_for_publisher / get_gid_for_client UNSUPPORTED, though graph.c already computes each endpoint's DDS GID"
status: resolved
type: enhancement
area: [rmw, xrce]
severity: low
found: 2026-10-27
resolved: 2026-10-27
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

## Resolution (2026-10-27)

Both slots are filled (`src/vtable.c`, `xrce_get_gid_for_publisher` /
`xrce_get_gid_for_client`) over one graph.c helper,
`xrce_graph_endpoint_gid`: a tracked endpoint answers `gid_of(prefix, key,
kind)` — the 16 bytes `ros_discovery_info` lists for it, zero-extended to 24 —
stamped with the backend's identifier (`"xrce"`, the string
`get_implementation_identifier` returns). A client's gid is its REQUEST
writer, the GUID a server reads in `take_request_with_info`'s request id. An
endpoint graph.c never tracked (probe unanswered, key withheld after a failed
create) answers UNSUPPORTED; so does one untracked by its destroy.

- **No-Agent:** `tests/gid.c` (`nros_rmw_xrce_gid`, `just check rmw-xrce`) —
  publisher = prefix + key + `0x03`, client = its writer key and not its reader
  key, the publisher's gid is one the serialized `ros_discovery_info` sample
  carries, untracked → UNSUPPORTED, NULLs → INVALID_ARGUMENT. Negative control:
  answering the client's reader and dropping the `linked` check failed four of
  its checks.
- **Live:** `xrce_ros2_interop::test_xrce_publisher_gid_is_the_one_a_stock_graph_reports`
  (fixture `graph-probe-xrce`, the probe's `GRAPH_PROBE_GID` mode; cell
  `native-pubsub-rust-xrce-n2r`, so the nightly `live-peer` lane runs it once
  the verdict records it). Against the pinned Agent (2.4.3-nros1) and a Humble
  `rmw_fastrtps_cpp` `ros2 topic info --verbose`, the peer attributed the
  publisher to `graph_probe` and printed `010f14914eb503d60000000000000603`, the
  16 bytes our `get_gid()` reported. Negative control: offsetting the key by one
  in `xrce_graph_endpoint_gid` failed the case (`…0603` vs `…0703`).

The client half is not compared against a peer live: no stock tool prints a
client's GID, and no nano-ros language surface exposes `get_gid_for_client`
yet. It is the same derivation as the publisher's, over the client's request
DataWriter — the GUID a server reads as `request_id.writer_guid` (the field
issue 2088 measured from the server side).
