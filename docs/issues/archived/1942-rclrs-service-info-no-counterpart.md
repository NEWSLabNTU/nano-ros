---
id: 1942
title: "rclrs hands a service callback its `ServiceInfo` (request id, timestamps); nano-ros's service callback gets the request alone"
status: resolved
type: limitation
area: [api]
severity: low
found: 2026-10-18
resolved: 2026-10-26
resolved_in: phase-515 R7
related: [issue-1789, phase-515, issue-2081]
---

**Home: [phase-515](../../roadmap/archived/phase-515-api-stock-shape-review.md)** — homed 2026-10-24; listed in that phase's table of open API issues.

**Found by** phase-506 W1 ([archived](../../roadmap/archived/phase-506-api-crates-carry-only-the-api.md)). Unhomed since that phase closed, like the other API-parity gaps (#1972–#1978); it is a feature to add, not part of the cleanup that phase did.

## What

rclrs 0.7.0 defines `ServiceInfo` and can pass it to a service callback:

```rust
pub struct ServiceInfo {
    pub source_timestamp: Option<SystemTime>,
    pub received_timestamp: Option<SystemTime>,
    pub request_id: RequestId,   // writer_guid: [u8; 16], sequence_number: i64
}
```

Our typed service callback is `FnMut(&Svc::Request) -> Svc::Reply`
(`nros-node/src/executor/node.rs`, `create_service`): it sees the request and
nothing about who sent it or when. The subscription side has `MessageInfo`
(issue 0242); the service side has no counterpart.

## How it was hidden

Until phase-506 W1 the facade re-exported the RMW seam's `nros_rmw::ServiceInfo`
— a service DESCRIPTOR (name, type name, hash, domain), a different thing under
the same name. The API-parity correlator matched the two by name, so this gap
read as covered. W1 took the seam out of `nros::` (RFC-0071 D12) and the
extraction reported `rust:ServiceInfo` theirs-only.

## What a fix needs

A `*_with_info` service registration whose callback receives the request's
sequence number, the client's GID and the timestamps the backend can supply,
the way `MessageInfo` reaches a subscription. The wire facts exist below the
seam (every backend keys replies on the request's sequence number); the
question is what each backend can report and how the C and C++ shims carry it.

## Acceptance

`rust:ServiceInfo` in the API-parity ledger moves from `gap` to a match or a
`divergence` naming the shape that shipped.

## Resolution (2026-10-26)

**Shape.** `nros::ServiceInfo` and `nros::RequestId`
(`packages/core/nros-core/src/service_info.rs`), rclrs's names and fields:
`source_timestamp`, `received_timestamp`, `request_id { writer_guid: [u8; 16],
sequence_number }`. One divergence: every field is an `Option` and the
timestamps are `nros::Time` (no `SystemTime` in `no_std`), because whether a
request carries a field is a backend property and an absent field must read
`None`, never a zero — the `MessageInfo` precedent. `ServiceInfo::is_reported()`
says whether the backend reported anything. Registration:
`Node::create_service_with_info::<Svc, _>(name, |req, info| ..)` and
`node.service(name).service_info().build::<Svc, _>(..)`, over
`Executor::register_service_with_info_sized_on`.

**Seam.** `ServiceTrait::request_info()` (default `NOT_REPORTED`) plus
`handle_request_with_info`; `handle_request` now delegates to it. The seam
re-exports the type as `nros_rmw::ServiceRequestInfo` — NOT under its own
name, since `nros_rmw::ServiceInfo` is the service descriptor, and two types
under one name is how this issue hid. A Rust backend's info crosses the C
`take_request` slot through the Rust adapter's side table: the subscription
`MessageInfo` table, with a presence byte placed in padding the slot already
had (measured: the slot does not grow, issue 0271).

**Per backend.**

| backend | `request_id` | `source_timestamp` | `received_timestamp` |
| --- | --- | --- | --- |
| zenoh | yes — query attachment, via new `zpico_queryable_request_attachment` reading the cloned query in its reply slot | yes | no (not on the wire) |
| Cyclone, XRCE, any C-ABI backend | no | no | no — issue 2081 |

**Found on the way.** The zenoh CLIENT stamped its request attachment's
source timestamp in MILLISECONDS; `rmw_zenoh_cpp` and our own publisher write
nanoseconds. Nothing read it until now. Fixed (`current_timestamp_ns`); the live
cell below fails against the old code (mutation measured: `1793012402757 ns is
outside the send window`).

**Tests.** `nros-node`: `service_with_info_callback_receives_the_requests_metadata`,
`service_with_info_reports_nothing_a_backend_did_not_say` (mock backend).
`nros-rmw-cffi`: `a_rust_backends_request_info_crosses_the_seam` (production
adapter chain), `service_info_side_channel_keeps_absent_fields_absent`,
`service_presence_does_not_widen_the_message_info_slot`. `nros-rmw-zenoh`:
`request_info_reads_the_clients_attachment`, `an_unparseable_attachment_reports_nothing`,
and the live router cell `zenoh_integration::a_service_request_reports_its_clients_identity`
(real shim client -> router -> real shim server; gid = client's `get_gid`,
sequence = `send_request_raw`'s return, timestamp inside the send window).

**Ledger.** `rust:ServiceInfo` `gap` -> `divergence` / `adopt-bounded` with its
envelope in the module rustdoc; `rust:RequestId` row deleted (now `same`);
`rust:rmw_request_id_t` re-pointed; new rows for `create_service_with_info`,
`register_service_with_info_sized_on`, `ServiceInfo::is_reported`,
`REQUEST_WRITER_GUID_SIZE`.

**Still open:** C-ABI backends (issue 2081); `received_timestamp` on zenoh;
C/C++ surfaces have no service-info callback (out of scope here, rclrs-only).
