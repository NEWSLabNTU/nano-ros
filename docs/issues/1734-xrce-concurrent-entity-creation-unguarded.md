---
id: 1734
title: "The XRCE shim has no guard for entity creation from more than one thread — its slot claims are the same check-then-act zenoh's were (issue 1711), on top of a client library that is not thread-safe"
status: open
type: bug
area: [rmw-xrce, tiers]
severity: medium
found: 2026-10-07
related: [1711]
---

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
