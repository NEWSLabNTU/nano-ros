---
id: 1008
title: "`wait_for_service` returns `Ok(true)` immediately on every real backend —
  its fast path calls `is_server_ready()`, whose trait default is `true` and
  which only zenoh overrides"
status: open
type: bug
area: api, rmw
related: [phase-379, rfc-0018]
---

## Problem

`EmbeddedServiceClient::wait_for_service` (`nros-node/src/executor/handles.rs:2150`)
opens with a fast path:

```rust
// Already proven once — don't re-query.
if self.handle.is_server_ready() {
    return Ok(true);
}
```

`ClientTrait::is_server_ready` has a trait DEFAULT of `true`
(`nros-rmw/src/traits.rs:2631`), and the only override in the tree is zenoh's
(`nros-rmw-zenoh/src/shim/service.rs:1034`).

`self.handle` is `session::RmwServiceClient` =
`<ConcreteSession as Session>::ClientHandle`, and `ConcreteSession` is
`nros_rmw_cffi::CffiSession` (`nros-node/src/session.rs:11`). **cffi does not
override `is_server_ready`.**

So for every backend reached through the cffi vtable — cyclonedds, XRCE, uORB —
the fast path is taken unconditionally and `wait_for_service` returns `Ok(true)`
without waiting, without probing, and regardless of whether any server exists.

## Why it survived

cffi *does* implement the honest probe, `server_available()`
(`rmw/cffi/src/lib.rs:3888`), which returns `Err(TransportError::Unsupported)`
when the vtable slot is NULL and the real answer otherwise. The two methods sit
in the same trait and one of them is right.

`is_server_ready`'s own doc comment says it is distinct from `server_available`
"which collapses 'don't know' and 'no server' into the same `false` answer" —
but the default makes it collapse them into the same **`true`**, which is worse:
`false` would merely make a caller wait unnecessarily, `true` makes it proceed.

Nothing catches it because a passing `wait_for_service` looks exactly like a
successful wait. The failure surfaces later as a request-side timeout — the very
startup-ordering race `server_available` was added to prevent (phase-124.C.1).

## The design defect behind it

Upstream separates "did the check work" from "what is the answer":
`rcl_service_server_is_available(node, client, bool *is_available)` returns
`RCL_RET_OK` "if the check was made successfully (**regardless of the service
readiness**)". rclcpp collapses to a bare `bool` and moves the error to
**exceptions**.

RFC-0018 forbids exceptions, so the bare-bool shape has nowhere to put "cannot
answer" — and the default it chose is the optimistic one. `is_server_ready` is
that collapse without the exception channel that makes it safe upstream.

## Fix

Phase-379 W6 decision 2 deletes `is_server_ready` and keeps the two-channel form
under rclcpp's NAME:

* Rust — `ClientTrait::service_is_ready() -> Result<bool, E>`
* C++ — `Client::service_is_ready() -> Expected<bool>`
* C — `nros_ret_t nros_client_service_is_ready(client, bool *out)`, rcl's shape

The call sites become `matches!(…, Ok(true))`, so `Ok(false)` **and** `Err` both
fall through to the wait loop. That is the behaviour the fast path's comment
already claims: "already proven once" should mean proven, not assumed.
