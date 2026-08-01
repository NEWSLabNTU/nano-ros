---
id: 376
title: "zpico multi-session is pubsub-only — the Rust service/client layers (SERVICE_BUFFERS, REPLY_WAKERS) are still process-global arrays, so two sessions' queryables and pending-gets collide"
status: resolved
type: limitation
area: core
related: [issue-0348, issue-0347]
resolved_by: phase-328
---

## Resolution (2026-08-01, phase-328 follow-up)

Both process-global arrays are now session-scoped, flattened by the session's
pool index (which the C shim exposes via a new `zpico_session_index(handle)`):

- `SERVICE_BUFFERS` is sized `ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES`,
  indexed `session_index * ZPICO_MAX_QUERYABLES + local`; the per-session
  local index comes from `NEXT_SERVICE_BUFFER_INDEX[session_index]`. The global
  index is the callback's `ctx`.
- `REPLY_WAKERS` is sized `ZPICO_MAX_SESSIONS * ZPICO_MAX_PENDING_GETS`, indexed
  `session_index * ZPICO_MAX_PENDING_GETS + slot`. The C reply-waker callback
  type gained a leading `session_index` arg (`void (*)(int32_t, int32_t)`) —
  `pending_get_reply_handler`/`_dropper` pass `s - g_sessions`; the client
  registers/wakes at the same session-scoped slot (`ZenohServiceClient` caches
  its `session_index`).

At the default `ZPICO_MAX_SESSIONS == 1` both arrays are their original size
and `session_index == 0`, so the layout and footprint are byte-identical to
before. Verified: full `zenoh_integration` suite 15/15 at `ZPICO_MAX_SESSIONS=2`
and 14+skip at the default 1 (single-session unchanged). See
[phase-328](../roadmap/archived/phase-328-zpico-multi-session.md). Original finding below.
---

## Finding (2026-08-01, residual of phase-328 / issue 0348)

Phase-328 made the zpico **C shim** multi-session: `g_session` and every
per-session `g_*` table moved into a pooled `struct zpico_session`, and every
`zpico_*` entry point takes a `zpico_session_t*` handle. Pub/sub is genuinely
multi-session — verified by `two_sessions_deliver_cross_session_through_router`
(15/15 at `ZPICO_MAX_SESSIONS=2`).

The **Rust RMW shim layer above it is not.** Two `static`/`static mut`
process-global arrays in `packages/rmw/zenoh/nros-rmw-zenoh/src/shim/service.rs`
are indexed by a bare handle with no session dimension:

- `SERVICE_BUFFERS: [ServiceBuffer; ZPICO_MAX_QUERYABLES]` (line ~93) — the
  request rings + reply state for every queryable, allocated by a single
  process-global `NEXT_SERVICE_BUFFER_INDEX` counter.
- `REPLY_WAKERS: [AtomicWaker; ZPICO_MAX_PENDING_GETS]` — the async
  service-client reply wakers, indexed by the C pending-get slot.

Both live ABOVE the `zpico_session_t` boundary and are shared across sessions.
Two sessions that each open service servers/clients draw handles from the same
counter/arrays, so:

- session B's queryable at buffer index *i* overwrites the ring/reply state of
  session A's queryable at index *i* (the process-global `NEXT_SERVICE_BUFFER_INDEX`
  does not collide indices between sessions, but the array is capacity-bounded
  by `ZPICO_MAX_QUERYABLES` for BOTH sessions combined, and nothing scopes a
  buffer to its session beyond the callback's stored handle);
- `REPLY_WAKERS[slot]` is woken by C's pending-get callback keyed on the
  per-session C slot index, which is NOT unique across sessions — session A's
  pending-get slot 0 and session B's pending-get slot 0 wake the SAME Rust
  waker.

Single-session is correct (the queryable callback records its owning session in
`ServiceBuffer.session` and calls `zpico_queryable_take_reply_seq(session, …)`
on the right pool slot). The break only manifests with **two sessions each
running services/clients** — a topology nothing in-tree uses today, same as the
motivation for 0348 itself.

## What a fix needs

Move both arrays under the session dimension — either:

1. a per-`ZenohSession` (Rust-side) owned request-ring/waker table, keyed
   locally, so two sessions never share the index space; or
2. keep the process-global arrays but make the index `(session_slot, handle)`
   and size them `ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES` — cheaper but
   multiplies the static footprint the way 0348 deliberately avoided on the C
   side.

Option 1 matches the C-side design (per-session state, not a multiplied global)
and is the honest target. The `no_std` constraint means these stay
statically-sized; the per-session table would need its slot count as a
per-instance parameter (the same shape 0316's enumeration work wants).

## Why it is not urgent

Identical to 0348's rationale: no example, fixture or test opens two zenoh
sessions, let alone two running services. The bridge workspaces pair zenoh with
a different backend. This is a capability gap in the multi-session capability,
not a live break — single-session services are correct.

## Acceptance, if picked up

- Two sessions in one process, each with a service server on the same service
  name (distinct domains), field requests independently; a request to session
  A's server is never delivered to session B's ring.
- Two async service clients across two sessions wake the correct pending-get
  future.
- Footprint of a single-session build with `ZPICO_MAX_SESSIONS=1` is unchanged
  (per-session tables must not multiply on a target that opens one session).

See [phase-328](../roadmap/archived/phase-328-zpico-multi-session.md) and issue 0348 for
the C-side design this extends.
