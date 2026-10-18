---
id: 1942
title: "rclrs hands a service callback its `ServiceInfo` (request id, timestamps); nano-ros's service callback gets the request alone"
status: open
type: limitation
area: [api]
severity: low
found: 2026-10-18
related: [issue-1789, phase-506]
---

**Home: [phase-506](../roadmap/phase-506-api-crates-carry-only-the-api.md)** — found by its W1; listed in that phase's issue table.

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
