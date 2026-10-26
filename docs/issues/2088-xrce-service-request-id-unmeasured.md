---
id: 2088
title: "XRCE's `take_request_with_info` is compiled but never measured against an Agent"
status: open
type: test-gap
area: [rmw, xrce, testing]
severity: low
found: 2026-10-26
related: [issue-2081, phase-515]
---

**Home: [phase-515](../roadmap/phase-515-api-stock-shape-review.md)** (R7). This is residue from issue 2081.

## What

Issue 2081 gave XRCE-DDS a `take_request_with_info` slot
(`packages/rmw/xrce/nros-rmw-xrce/src/service.c`, `xrce_fill_service_info`).
It reports the Agent's `SampleIdentity` as the request id: the 12-byte GUID
prefix plus the 4-byte entity id, and the sequence number as
`(high << 32) | low`. That is how `rmw_microxrcedds` fills it.

`just check rmw-xrce` compiles the slot. Nothing runs it against a live
Agent. The Cyclone half of 2081 was measured against a stock
`rmw_cyclonedds_cpp` client (`ros2_srv_e2e.sh`); the XRCE half was not.
So nothing yet confirms that the GUID it reports is non-zero, that the
sequence matches the client's, or that the byte order is right.

## What would close it

An XRCE service cell, interop or loopback through the Agent, that asserts
the server's `ServiceInfo::request_id` is reported: a non-zero GUID and the
client's sequence number. For example, a Rust server registered with
`create_service_with_info` on XRCE, plus a stock client.
