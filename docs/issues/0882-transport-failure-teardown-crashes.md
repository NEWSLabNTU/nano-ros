---
id: 882
title: "`_zp_unicast_failed` -> `_z_task_free` panics on an invalid spinlock —
  zenoh-pico's transport-failure path crashes, and latency makes it reliable"
status: open
type: bug
area: rmw
related: [issue-0852, issue-0822, issue-0839, issue-0881]
---

## Symptom

An action goal is **accepted** and its result never arrives:

```
Waiting for an action server to become available...
Sending goal: order: 6
Goal accepted with ID: 1b3fb9b1edfb4e56bda6c411f2434e82
      <- nothing further
```

7 of 10 goals complete normally. The failures all look like this — never a
rejected goal, never a timeout before acceptance.

## What the wire shows

Captured with a `socat` tap, which unlike a debug probe does not halt the core
([issue 0881](0881-the-debugger-is-not-a-passive-instrument.md)). After the
80-byte `send_goal` query reaches the board, every board-to-router frame is a
1-byte keepalive:

```
[42] router->board len=80    <- the goal
[43..60] board->router len=1  x many   <- keepalives only, no reply
```

Then, in a longer run, the wire goes **completely silent** — not even
keepalives. The byte count froze at 66,347 and never moved again across seven
further goal attempts.

## The crash

Read after the hang, so the probe cannot be blamed for causing it — the fault
timestamp (1:00 uptime) precedes the attach:

```
ASSERTION FAIL [z_spin_lock_valid(l)]   Invalid spinlock 0x2040f504
ZEPHYR FATAL ERROR 4: Kernel panic on CPU 0
Current thread: (unknown)

lr 0x0041ccc1 -> z_spinlock_validate_post
   0x0042cc09 -> _z_task_free
   0x00412ee9 -> _zp_unicast_failed
```

A transport failure enters `_zp_unicast_failed`, which tears down and frees a
task, and something then takes a lock belonging to freed or never-initialised
state. `Current thread: (unknown)` is consistent with the TCB itself being gone.

This is the same neighbourhood as [issue 0822](0822-*) (the thread-slot leak) and
the `_z_task_join` / slot-release rule that came out of it: **release only after
a join has returned, never on detach.** A free that races a still-live user of
the same object is exactly what that rule exists to prevent, and the crash says
the rule is not sufficient on this path.

## Reproduction — latency is the trigger

The strongest handle on this bug is that **inserting the serial tap makes it
almost certain**:

| path | goals completing |
| --- | ---: |
| board <-> router direct | **7 of 10** |
| board <-> socat pty <-> router | **0 of 8** |

socat adds a pty hop, roughly doubling the serial path latency and adding
buffering. That is enough to turn a 30 % failure into a 100 % one, and after the
first failure the board is dead for the rest of the run.

**This is useful, not merely a caveat.** It gives a reliable trigger for testing
a fix, on a defect that is otherwise intermittent. It also means the wire
evidence above was gathered under the condition that most provokes the bug,
which is worth stating.

It further suggests the trigger is a **transport timeout**, not the goal payload:
nothing about a `send_goal` query is special except that it is the traffic
present when the link is slow enough to time out.

## Why this matters beyond actions

`_zp_unicast_failed` is the generic transport-failure path. Anything that makes
the link miss a deadline reaches it — load, latency, a debug probe halting the
core ([issue 0881](0881-*)), a lost frame. So this is not an action defect; it is
what happens to this image whenever the transport hiccups, and actions are
simply the heaviest traffic that provokes it.

## Fix direction

Read `_zp_unicast_failed` and `_z_task_free` together and find which lock is
taken after which free. The candidates are the session mutex, a per-task
condvar, and the read-task's own storage, which nano-ros allocates from a slot
table rather than from the heap — so `_z_task_free`'s `k_free` may be freeing
something the slot table still considers live, or vice versa.

Acceptance is the table above inverted: goals complete through the tap, which is
currently 0 of 8.
