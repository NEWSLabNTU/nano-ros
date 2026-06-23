---
id: 96
title: "In-process (same-executor) node-to-node delivery does not happen — pub/sub AND service"
status: open
type: bug
area: core
related: [phase-263]
---

## Summary

When two nodes are registered on the **same** `Executor` (same process, same RMW
session), one node does **not** receive what the other sends. This affects **both**:

- **Service round-trip:** a client issuing a blocking `TickCtx::call_for_name` to a
  same-process server — the server's `on_callback` never fires, no reply, the client's
  bounded wait times out.
- **Pub/sub:** a subscriber node never receives a same-process publisher node's
  messages — its subscription `on_callback` never fires (verified separately: a plain
  in-process subscriber on a topic the same entry publishes gets **zero** callbacks,
  while an *external* process subscribed to the same topic receives normally).

So it is **not** service-specific (an earlier draft of this issue wrongly said
"same-session pub/sub works" — it does not). The root cause is intra-session delivery:
zenoh does not, by default, loop a session's own publications back to that session's own
subscribers/queryables, and the nodes of one `nros::main!` entry share one session.

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

A single-process workspace cannot host two nodes that talk to each other — neither a
service server+client, nor a publisher+subscriber. This affects every single-entry
multi-node demo whose nodes are meant to communicate: the phase-263 A1 combined showcase
(`showcase.launch.xml`), the B1 safety workspace (talker → safe_listener), and the basic
talker+listener quickstart entry (the internal listener never receives — only an external
process does). The phase-263 A1 service and B1 safety Track-D demos are therefore
**cross-process** (separate entries) — the supported topology, matching the working
imperative cross-process tests (`native_api.rs::test_native_service_communication`,
`safety_e2e.rs`).

## Repro

`examples/workspaces/rust` showcase entry + `tests/service_roundtrip_xprocess_e2e.rs`
(cross-process — passes). The in-process failure reproduces with all four showcase
nodes in `native_showcase_entry`; see the bisect above.
