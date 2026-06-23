---
id: 96
title: "In-process (same-executor) declarative service round-trip: server never receives the client's request"
status: open
type: bug
area: core
related: [phase-263]
---

## Summary

When a declarative service **server** and **client** are registered on the **same**
`Executor` (same process, same RMW session) and the client issues a blocking
`TickCtx::call_for_name` from its `tick`, the server **never receives the request** —
its `on_callback` does not fire, no reply is produced, and the client's bounded wait
times out. Same-session pub/sub on the same executor works fine; only the service
query→queryable path fails in-process.

Discovered running the never-before-run phase-263 A1 feature-showcase entry
(`examples/workspaces/rust`, `native_showcase_entry`), which boots `add_server` and
`add_client` in one process. `add_client` republishes the AddTwoInts reply on `/sum`;
an external `/sum` subscriber sees **zero** messages.

## Evidence (bisected)

Boot the showcase entry (built with `NROS_EXECUTOR_MAX_CBS=8`, see issue 0095) against a
zenohd router with external subscribers:

- `/chatter` (talker → external listener): **12 msgs in 12 s** — same-session pub/sub
  and the timer/tick machinery work.
- Replace `add_client.tick`'s service call with a direct `ctx.publish_to_topic::<Int32>("/sum", a+1)`:
  `/sum` receives **1,2,3,…,8** — so the client's timer fires, `tick` runs, and publish works.
- Restore the real `call_for_name`, and additionally have `add_server.on_callback`
  publish its computed sum to a `/srvhit` topic on each request: `/srvhit` receives
  **0** msgs across 11+ call attempts in 12 s — **the server never receives the request.**

So the failure is on the **request side**, not the reply side, and not first-call
discovery timing (every attempt over 12 s fails).

## Mechanism (where it likely is)

`TickCtx::call_for_name` → `RuntimeClientDispatch::call_raw`
(`packages/core/nros/src/node_runtime.rs`) does `send_request_raw` then spins the
executor **re-entrantly** (`executor.spin_once(10ms)` ×200) waiting for the reply via
`try_recv_reply_raw`. The nested `spin_once` is expected to drive the same executor's
service-server dispatch (registered via `register_service_raw_sized_on`) so the
queryable answers the query — but in-process / same-session that queryable never
matches or fires for the locally-issued query. Candidate causes: zenoh same-session
query↔queryable matching, or the re-entrant `spin_once` not draining the inbound
service-server entry while it is itself nested inside a `tick` dispatched from the
outer spin.

## Impact

A single-process workspace cannot host both a service server and a client of that
service. The phase-263 A1 combined showcase (`showcase.launch.xml`) therefore cannot
demonstrate the service round-trip in one entry. Phase-263 A1's service Track-D demo is
restructured to be **cross-process** (separate server + client entries) — the supported
topology, matching the working imperative cross-process service test
(`native_api.rs::test_native_service_communication`).

## Repro

`examples/workspaces/rust` showcase entry + `tests/service_roundtrip_xprocess_e2e.rs`
(cross-process — passes). The in-process failure reproduces with all four showcase
nodes in `native_showcase_entry`; see the bisect above.
