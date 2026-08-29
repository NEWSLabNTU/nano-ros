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

## Cause CONFIRMED by disabling the reopen path

`Z_FEATURE_AUTO_RECONNECT` defaults to 1 in zenoh-pico and no image could
override it. Adding `CONFIG_NROS_ZENOH_AUTO_RECONNECT` and turning it off
isolates the defect exactly:

| build | through tap | direct |
| --- | ---: | ---: |
| reconnect **on** (default) | **0/8** | 7/10 |
| reconnect **off** | **8/8** | 4/10 |

Through the tap — the reliable trigger — the crash goes from certain to absent.
That places the defect on the reopen path and nowhere else.

**It is not a fix.** Off, the session never recovers from a transport failure,
which is why the direct column gets *worse*: a hiccup that reconnect used to
survive now ends the session for good. Neither setting is acceptable, and the
image is therefore left at the default.

## What is actually wrong

`_zp_unicast_failed` runs **on the lease task** and does, in order:

```c
_z_task_join(read_task); _z_task_free(read_task);   // fine
_z_unicast_transport_close(ztu, _Z_CLOSE_EXPIRED);
_z_unicast_transport_clear(ztu, true);   // detaches+frees the LEASE task (itself)
                                         // and _z_mutex_drop()s the transport mutexes
ret = _z_reopen(&zs);                    // ...then reopens, on that torn-down state
_z_task_exit();
```

Two distinct defects on that path:

1. **The lease task frees the storage it is running on.** `_z_common_transport_clear`
   frees `_lease_task` while that task is the caller. `_z_reopen` then starts a
   new lease task, and `z_malloc` hands back the block just freed — two live
   threads sharing one `pthread_t`. This matters especially here because
   `nros_zephyr_task_slot_release` keys the stack-slot table on `pthread_t`
   (issues 0822, 0839), so the wrong stack can be released.

   **Fixed** in the Zephyr port: `_z_task_free` refuses a self-free and defers
   the handle to the next free from another thread. Necessary, and on its own
   **not sufficient** — the crash still reproduces with it in.

2. **The mutexes are destroyed before the reopen that needs them.** The faulting
   lock resolves inside `nros_platform::zephyr_heap::HEAP` (+676): a
   heap-allocated mutex used after its allocation was released. That is the
   remaining defect and it is where the real fix belongs.

## Fix direction

`_z_common_transport_clear` must not tear down state that the reopen on the
same session is about to use, and must not free the calling task. Either the
clear should be split so the reopen path skips the mutex drop and the self
free, or the reopen should build a fresh transport before the old one is
released rather than after.

This is shared code on a path every platform takes, so it belongs upstream in
eclipse-zenoh rather than in the Zephyr port. The port-side half (defect 1) is
already fixed here.

Acceptance is the table above inverted: goals complete through the tap, which is
currently 0 of 8.
