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

## The faulting "spinlock" is allocator metadata — read off the halted board

The assert address is **identical on every run**: `0x2040f504`. Corrupt reused
memory would move; a fixed offset means a specific block. Reading it while the
board sat halted after the fault:

```
2040f4f0:  00000011  2040f3a0  00000002  0000001a
2040f500:  00000011  2040f4f0  80000000  00000000
2040f510:  00000011  2040f500  80000001  00000000
```

A repeating 16-byte header whose second word points at the previous block —
**TLSF free-block metadata**, written by `FreeListHeap` into a block when it is
freed. So the object being locked is a **freed heap block**.

That also explains the assert's exact wording. `z_spin_lock_valid` does not test
for valid memory:

```c
if (thread_cpu != 0U && (thread_cpu & 3U) == _current_cpu->id) return false;
```

Here `thread_cpu` reads `0x2040f4f0` — a heap pointer, low two bits zero, so it
compares equal to CPU 0 and the lock is declared invalid. The message says
"Invalid spinlock" but the condition it actually detects is "this CPU already
holds it"; both readings arrive at the same place because the memory is not a
lock at all.

**Use-after-free is therefore established at the memory level**, not inferred.

## Two hypotheses tested and what they showed

**Self-free of the running task — fixed, insufficient.** `_z_task_free` now
refuses to free the caller's own handle. Correct on its own terms; the crash
reproduces with it in.

**Reuse of a freed mutex by a later allocation — falsified as stated.** A
quarantine holding the last eight dropped `k_mutex` objects back from the arena
did not help (still 0/8 through the tap, same address). The reason matters:
TLSF writes its metadata into a block **at the moment of free**, not when the
block is next handed out. So delaying *reuse* cannot help — only not freeing,
or not holding the reference, can. The quarantine was reverted rather than kept
as an unexplained change.

## The block is NAMED — it is the read task's own handle

Instrumenting `nros_platform_alloc`/`dealloc` for just the 0x2040f4d0..0x2040f530
window, with `__builtin_return_address(0)`, and driving a goal to the crash:

```
ALLOC 0x2040f508 +36  from _z_slice_init        slice.c:45
FREE  0x2040f508      from _z_slice_clear       slice.h:90
ALLOC 0x2040f508 +4   from _zp_start_read_task  session.c:464
ALLOC 0x2040f518 +4   from _zp_start_lease_task session.c:506
```

A 36-byte slice buffer is released, and the freed block is handed straight to
`_zp_start_read_task` for the read task's 4-byte `_z_task_t` — its `pthread_t`.
The lease task's handle lands in the next block.

**`0x2040f504`, the address the assert names, is the TLSF header of the read
task's handle block.** So the object under the bad lock is not a mutex at all:
it is allocator metadata immediately below the task handle that
`_zp_unicast_failed` frees and `_z_reopen` re-allocates.

That closes the loop with the first defect. `_zp_unicast_failed` does:

```c
_z_task_join(read_task);   _z_task_free(read_task);   // block returns to the arena
...
_z_reopen(&zs)  ->  _zp_start_read_task()             // and is handed straight back
```

The read-task free is legitimate — the join returned — so the self-free guard
does not apply to it. What is not legitimate is anything still holding that
pointer, or holding the `pthread_t` VALUE it contained, across the free. On this
port that includes `nros_zephyr_task_slot_release`, which keys the stack-slot
table on `pthread_t` (issues 0822, 0839): a recycled handle value makes the
table ambiguous.

## Not a race with the application — four things ruled out by measurement

**Not an application race.** With the board **idle** — no goals, no traffic of
any kind — killing the router forces the lease to expire, and the board crashes
anyway: same address `0x2040f504`, same thread, 21 ms after the expiry message.
Only the lease and read tasks are involved; nothing the executor does matters.

**Not accumulation.** It crashes on the **first** failure. One
`Closing session because it has expired after 10000ms`, then the panic 21 ms
later. No repeated reopens, no leak building up over reconnects.

**Not slot exhaustion.** Zero `OUT OF THREAD SLOTS` messages in the run.

**Not a failed join.** Instrumenting `_z_task_join` across the forced failure:

```
JOIN rc=0 after 20ms owner=0x80000000
```

`pthread_join` returns success, so the read task genuinely exited before its
handle was freed. This mattered because `_zp_unicast_failed` **ignores** the
join's return value and frees the handle regardless (`lease.c:62-63`) — that
would be a real defect if the join ever failed, but it is not what is happening
here.

**It does require the reopen.** With `Z_FEATURE_AUTO_RECONNECT` off the same
failure path runs and does not crash (8/8 through the tap). So the fault is in
`_z_reopen` or after it, on a single pass.

## The sequence, now fully bounded

```
20.678  lease expires                              (lease task)
        _z_task_join(read_task)      rc=0, 20 ms   -> read task genuinely gone
        _z_task_free(read_task)                    -> its 4-byte block returns to the arena
        _z_unicast_transport_clear(detach=true)    -> drops the transport mutexes
        _z_reopen -> _zp_start_read_task           -> allocates INTO that same block
20.699  panic: "Invalid spinlock 0x2040f504"       -> the block's TLSF header
```

Everything in that window is accounted for except the one thing that matters:
which surviving reference reaches into the freed block.

## What is still unknown

Which reference survives the free. The block is named and the collision is
reproducible, but the code that dereferences the stale pointer has not been
caught in the act.

The step that would catch it: poison the handle block on free (write a pattern
instead of leaving it to TLSF) and fault on first read, or set a watchpoint on
`0x2040f504` and let the debugger name the writer. Both are cheap now that the
address is deterministic and the allocation is attributed.

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
