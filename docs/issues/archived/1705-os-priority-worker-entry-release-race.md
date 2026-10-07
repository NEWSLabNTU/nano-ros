---
id: 1705
title: "An entry dispatched on an OS-priority worker thread can be released while the worker is running it — the deferred release covers only the spin thread"
status: resolved
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

## Resolution (2026-10-07)

A hand-off protocol on the entry's `SlotTag`, in the new
`executor/worker_item.rs`. `SlotTag::flags` became a `portable_atomic::AtomicU8`
in the same padding byte, so the tag is still 4 bytes and no backing size moved.

1. **Claim.** The spin thread sets a new `WORKER_QUEUED` bit before it
   enqueues the item. If the pool declines the item, the bit is cleared and
   the entry runs cooperatively.
2. **Exclusive access, enforced.** While `WORKER_QUEUED` is set the spin
   thread does not re-send the entry and does not run it cooperatively.
   `os_priority`'s `Send` comment had claimed this invariant ("won't re-send
   the same offset until the worker drains the previous one") and nothing
   enforced it: every ready cycle re-sent the entry.
3. **Release defers.** `release_entry` treats `WORKER_QUEUED` like
   `IN_DISPATCH`: it sets `RELEASE_PENDING` and returns. A release requested
   from inside the worker's own callback is that one atomic flag, and touches
   nothing the spin thread is using.
4. **The worker skips released entries.** `run_work_item` runs an item only
   if no release is pending, then clears `WORKER_QUEUED` with Release
   ordering.
5. **The spin thread reaps.** `reap_deferred_releases`, at the top of every
   `spin_once`, completes a pending release once nothing is in flight
   (Acquire).

**A gap one level wider, found by the first test.** The readiness scan,
LET pre-sample, timer sweep and dispatch loops did not consult the flags. So
an entry whose release was pending, or that a worker held, could still be
read or dispatched on the spin thread: the test deadlocked with the timer's
callback running on both threads. All four now skip a held slot
(`slot_held`).

**Tests.** They drive the worker's real per-item body from a `std::thread`,
since the pool itself needs a platform these unit tests do not link:

- `a_release_during_a_worker_dispatch_waits_for_the_worker`: the release is
  accepted while the callback blocks on the worker. `spin_once` keeps the
  entry, the capture is intact when the callback reads it, and the drop
  happens once, after the worker lets go.
- `a_queued_item_for_a_released_entry_is_never_dispatched`.
- `a_release_from_inside_a_worker_callback_is_completed_by_the_spin_thread`.

Each of the first two fails when its protection is removed (the
`IN_FLIGHT` deferral; the worker's pending-skip).

**The pool compiled nowhere.** No recipe enabled `scheduler-os-priority`, so
the pool and its dispatch site were built by no lane. `node-std-tests` now
clippies `alloc,rmw-cffi,scheduler-os-priority` with `-D warnings`.
