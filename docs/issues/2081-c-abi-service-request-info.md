---
id: 2081
title: "C-ABI RMW backends (Cyclone, XRCE) report no per-request `ServiceInfo`: the `take_request` slot has nowhere to put it"
status: open
type: limitation
area: [rmw, api]
severity: low
found: 2026-10-26
related: [issue-1942, phase-515, rfc-0054]
---

**Home: [phase-515](../roadmap/phase-515-api-stock-shape-review.md)** (item R7) — the residue of issue 1942.

## What

Issue 1942 shipped `nros::ServiceInfo` (rclrs's per-request metadata: client GID +
client sequence number, source/received timestamps) and the seam method
`ServiceTrait::request_info`. A Rust backend's info crosses the C ABI through the
Rust adapter's side table, so zenoh reports `request_id` and `source_timestamp`.

A backend written against the C vtable reports nothing: every field is `None`.
The `take_request` slot (`packages/core/nros-rmw-abi/include/nros/rmw_vtable.h`)
returns only `seq_out`, and its doc says that token "stands in for
`rmw_service_info_t *` — an RTOS reply needs the sequence and nothing else in that
struct". Cyclone has the facts (DDS sample info: source timestamp, the request
header's writer GUID and sequence number) and is never asked for them.

## What a fix needs

A `take_request_with_info` vtable slot (or a nullable info out-parameter), the
service twin of `take_with_info`: header edit, `scripts/gen-abi-bindings.sh`,
`rmw-api-parity` classification, `CffiService::take_request` dispatching to it when
non-NULL (falling back to the side table, as `take_serialized_with_info` does),
and Cyclone's `vtable.cpp` filling it. The ledger row
`c:take_request_with_info` (`declined` because "the ABI DECLARES the omission")
would then reopen — its own text says to re-open it if the ABI grows the pair.

## Acceptance

A Cyclone service server's `ServiceInfo` carries the requester's writer GUID and
sequence number, measured against a stock `rmw_cyclonedds_cpp` client.
