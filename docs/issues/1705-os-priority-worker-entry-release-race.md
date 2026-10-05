---
id: 1705
title: "An entry dispatched on an OS-priority worker thread can be released while the worker is running it — the deferred release covers only the spin thread"
status: open
type: bug
area: [core]
severity: medium
found: 2026-10-06
related: [1667, 1496, 1631]
---

## Summary

Issue 1667 made a release requested while an entry's callback is RUNNING wait
for the callback to return. The executor marks the slot `IN_DISPATCH` around
each `try_process` call on the spin thread (`begin_dispatch` /
`finish_dispatch` in `executor/spin.rs`). A release that arrives during that
window sets `RELEASE_PENDING`, and the entry is dropped when the callback
returns.

The `scheduler-os-priority` path does not go through that bracket. An entry
bound to a `SchedContext` with `os_pri > 0` is handed to a worker task as a
`WorkItem { arena_base, arena_offset, try_process, .. }`
(`executor/os_priority.rs`), and the worker calls `try_process` on its own
thread while `spin_once` carries on. So:

- **A release from the spin thread while the worker runs the entry.** For
  example, `timer_.reset()` called from another callback, or a node destroyed.
  This drops the entry and frees its arena bytes under the worker. The next
  registration can then reuse those bytes while the worker is still writing
  them.
- **A release from inside the worker's own callback.** This calls into the
  executor from the worker thread, concurrently with `spin_once` on the spin
  thread. Nothing synchronises the two.
- **A release while the item is still queued.** The worker then dispatches a
  dropped entry.

None of this is new with 1667: the action release (1496), every kind's release
(1631) and the node-scoped release (phase-476 W0) all had it. 1667 fixed the
same-thread case, which is the common one. This issue records the thread it
did not reach.

## What a fix needs

The worker must tell the executor when a dispatch starts and ends, and a
release must wait for, or be deferred past, an in-flight worker dispatch. For
example, an atomic per-slot in-flight count, set when the item is enqueued and
cleared by the worker after `try_process`, with `release_entry` deferring while
it is non-zero and `spin_once` completing the deferred release once it reaches
zero. A queued item for a released slot must also be dropped, not dispatched.

`SlotTag::flags` is a plain `u8` that only the spin thread touches today. The
worker cannot share it without making it atomic.

## Acceptance

- A test on the `scheduler-os-priority` path: an entry dispatched on a worker
  is released from the spin thread while its callback blocks, and the entry's
  capture is destroyed only after the callback returns.
- A queued item for a released entry is never dispatched.
