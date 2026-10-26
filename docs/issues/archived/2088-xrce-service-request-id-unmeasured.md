---
id: 2088
title: "XRCE's `take_request_with_info` is compiled but never measured against an Agent"
status: resolved
type: test-gap
area: [rmw, xrce, testing]
severity: low
found: 2026-10-26
resolved: 2026-10-27
related: [issue-2081, phase-515]
---

**Home: [phase-515](../../roadmap/phase-515-api-stock-shape-review.md)** (R7). This is residue from issue 2081.

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

## Resolution (2026-10-27)

Measured, and the slot is right. No backend change was needed.

**The cell.** `xrce_ros2_interop::test_xrce_service_reports_the_stock_clients_request_id`,
owned by the existing `native-service-rust-xrce-r2n` interop cell
(`interop::CASE_CELLS`). The nano side is a new bin,
`packages/testing/nros-tests/bins/service-info-server` (fixture row
`service-info-server-xrce`, image `linux_xrce`): an AddTwoInts server
registered through `create_service_with_info` that prints
`REQUEST_ID guid=<hex> seq=<n>` per request. It is a bin and not the
service-server example because `CallbackCtx` exposes no `ServiceInfo` (the
same reason issue 0441 gave `message-info-observer`). The peer is a live
MicroXRCEAgent plus a stock `ros2 service call -r 2` on `rmw_fastrtps_cpp`,
which loops `call_async` on ONE client.

**What it asserts.** At least three answered requests; the server reports
sequence numbers exactly `1..=n`, in order (a stock rclpy server reading the
request header sees 1, 2, 3, … from the same CLI, measured); one GUID across
all requests, non-zero, starting `01 0f` (eProsima's RTPS vendor id) and ending
`03` (a keyless user writer's entity kind). The two bytes sit at opposite ends
of the 16, so a mis-ordered prefix copy or a wrong key/kind split fails.

**What it measured** (ros2 distrobox, Humble, Agent 2.4.3):
`guid=010f149108e98c840000000000001203 seq=1..18` over 18 requests, the
client's writer GUID in wire order.

**Negative control.** Writing the entity kind into byte 12 instead of 15 in
`xrce_fill_service_info` fails the test on the kind-byte assertion
(measured), then reverted.

**One false alarm worth recording.** The first manual run showed `seq=2..7`
plus a duplicated `seq=7`. That was two servers on one Agent: an earlier run
of mine had left an orphan server attached, so the Agent split the client's
requests between them. Issue 0741's hazard (a second `/add_two_ints` server on
the domain); the new test checks for it before it starts, as its sibling does.
