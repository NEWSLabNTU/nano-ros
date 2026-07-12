---
id: 180
title: "Zephyr (native_sim) service server → native client: server replies but the native client never receives the reply"
status: open
type: bug
area: rmw
related: [issue-0164, issue-0153, issue-0173]
---

## Problem

`test_zephyr_server_native_client` (zephyr rust zenoh service SERVER + native rust
service CLIENT, shared host `zenohd`): the Zephyr server RECEIVES every request and
REPLIES to each, but the native client never surfaces a `Result of add_two_ints:`
and exits with no output — the reply does not reach the native client.

## Evidence (2026-07-12, fresh fixtures)

Zephyr server output (works end-to-end):

```
Waiting for service requests
Incoming request
a: 5 b: 3
Incoming request
a: 10 b: 20
Incoming request
a: 100 b: 200
Incoming request
a: -5 b: 10
```

Native client output: **empty**. Test state:
`zephyr_connected=true zephyr_ready=true zephyr_received=true zephyr_replied=true
client_response=false`.

The client (`examples/native/rust/service-client`) sends via `client.call()` then
`promise.wait(5 s)`, printing `Result of add_two_ints:` on a reply or
`Service call failed` on timeout. It prints NEITHER and exits ~8 s in, so it appears
to block in `promise.wait` (reply never routed) rather than take the error path.

## Scope — where it is NOT

- **Not the native client:** the same client passes native↔native
  (`nano2nano::test_talker_listener_communication`-class service tests). So the
  request→reply works when both ends are full-zenoh.
- **Not the server:** it receives + replies to all 4 requests.
- Specific to **zephyr-pico service server → native (full-zenoh) client** reply
  routing — the reply the pico server writes does not reach the native client's
  `get`/`Promise`. The reverse (`native_server_zephyr_client`) PASSES.

## Suspects / direction

- Issue-0153 gossip-gap: `wait_for_service` observes the server's liveliness token
  before its queryable ROUTE is installed at the router, so a `z_get` in that window
  matches no queryable and completes with no reply. The native client already retries
  3× with 1 s backoff to span it — that may be insufficient against a slow
  zephyr-pico server whose queryable declare lands later than a native peer's.
- Compare the zephyr-pico queryable's reply keyexpr / `z_query_reply` shape on the
  wire vs a native server's, at the router (`zenohd` debug), for the same `/add_two_ints`.
- Distinguish "client's `z_get` completed early with no matching queryable"
  (0153 timing) from "reply written by the server never routed to the client"
  (a pico reply-path bug) — the client blocking in `promise.wait` (no early Err)
  points at the latter.

## References

`packages/testing/nros-tests/tests/zephyr.rs::test_zephyr_server_native_client`,
`examples/native/rust/service-client`, issue #164 (surfaced it on fresh fixtures),
issue #153 (the zenoh service gossip-gap), issue #173 (the pub/sub sibling — that one
turned out to be a stale fixture, this one is not).
