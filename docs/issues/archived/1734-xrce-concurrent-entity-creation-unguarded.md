---
id: 1734
title: "The XRCE shim has no guard for entity creation from more than one thread — its slot claims are the same check-then-act zenoh's were (issue 1711), on top of a client library that is not thread-safe"
status: resolved
type: bug
area: [rmw-xrce, tiers]
severity: medium
found: 2026-10-07
resolved: 2026-10-18
related: [1711, phase-507]
---

**Home: [phase-507](../../roadmap/archived/phase-507-xrce-and-action-client-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## Summary

Issue 1711 found that the zenoh shim claimed an entity slot by finding the
first `!active` entry, declaring into it, and only then setting `active`. Two
tier threads sharing one session and declaring at the same time took the same
slot and corrupted the heap. The fix there was a session mutex around the
CLAIM (`zpico_claim_slot`).

The XRCE shim has the same shape in four places, none under any lock:

- `subscriber.c` — `st->subscriber_slots[i].active`
- `service.c:120` — `st->service_server_slots[i].active`
- `service.c:590` — `st->service_client_slots[i].active`
- `service.c:306` — `slot->reply_tokens[i].in_use`

The slot race is the smaller half. Every create also drives
Micro XRCE-DDS client calls (`uxr_buffer_create_*`, `uxr_run_session_*`) on the
session, and that library is not thread-safe. A grep of
`packages/rmw/xrce/nros-rmw-xrce/src/` for any lock or thread-safety note finds
none.

## Not measured

Whether any XRCE image creates entities from more than one thread today. A
multi-tier XRCE image would, if one exists: each tier's setup runs on its own
thread against the shared session. This issue was filed from the code while
fixing 1711, not from a failure.

## What a fix needs

Either a session-level lock that serialises every entity create and delete,
covering the uxr calls and not only the slot claim, or a stated rule, with
enforcement, that an XRCE session is driven from one thread, and a refusal when
a second thread tries.

## Acceptance

- A concurrent-create test on one XRCE session (the zenoh shim's
  `concurrent_declares_on_one_session_never_share_a_slot` is the shape), or the
  single-thread rule enforced with a test showing the refusal.

## Measured (2026-10-18)

`packages/testing/nros-tests/tests/xrce_concurrent_entities.rs` calls the
shim's own entry points against a live Agent: four threads, released by a
barrier, each create two subscriptions and a publisher on ONE session while a
fifth thread drives it (`xrce_session_drive_io`, 5 ms passes), for five rounds.
Every create must return OK, and no two live subscriptions may share a slot.

| shim | runs | result |
| --- | --- | --- |
| `origin/main` | 5 | 5 FAIL — `xrce_subscription_create` returns non-OK in round 0, on two threads at once |
| this change | 20 | 20 pass, ~2.5 s each |

So the uxr half was the live one: concurrent `uxr_run_session_until_all_status`
calls on one session lose each other's status replies, and the create fails
before a slot race can even be observed.

## Fix

One RECURSIVE lock per session (`api_lock` in `xrce_session_state`, the
platform's `nros_platform_mutex_rec_*`), held across every public entry point
that touches the uxr session or a slot pool: the four creates and destroys,
node create/destroy, publish (raw and streamed), take, has_data, the in-place
take, the service and client request/response paths, ping, and drive_io. Each
entry point is now a thin wrapper around its unchanged `*_locked` body.

* drive_io holds the lock for ONE `uxr_run_session_time` pass and never across
  its 1 ms yield, so a create on another thread gets in between passes.
* Recursive because the uxr callbacks run inside `uxr_run_session_*` on the
  holding thread, and entry points call entry points.
* A destroy that leaves a closed session empty no longer frees it in place:
  `free_pending` defers the free to the OUTERMOST unlock, so no thread unlocks a
  mutex inside freed memory. Every free of the state goes through
  `xrce_session_free`, which drops the mutex.
* A state that never went through `xrce_session_create` (the transport-free
  lifetime test's) has no mutex, and the lock degrades to its depth
  bookkeeping, which is what a single-threaded caller needs.

The test runs in `just test-all`, in the same invocation as `xrce_guard_wake`
(same feature, same Agent need). `xrce_guard_wake` still passes, so the
per-pass lock does not delay a poll-only executor's wake.
