---
id: 821
title: "The board takes a USAGE FAULT with pc=0 at exactly 2 x Z_TRANSPORT_LEASE —
  the auto-reconnect teardown runs inside the task it is dismantling"
status: open
type: bug
area: rmw
related: [issue-0822, phase-391]
---

## Problem

On mr_canhubk3/s32k344 with zenoh over serial, the board publishes perfectly and
then dies at the first lease expiry:

```
Publishing: 'Hello World: 40'
[00:00:20.128,000] <err> os: ***** USAGE FAULT *****
[00:00:20.128,000] <err> os:   Illegal use of the EPSR
[00:00:20.128,000] <err> os: Faulting instruction address (r15/pc): 0x00000000
[00:00:20.129,000] <err> os: >>> ZEPHYR FATAL ERROR 35: Unknown error on CPU 0
[00:00:20.130,000] <err> os: Current thread: 0x2040c5a0 (idle)
[00:00:20.130,000] <err> os: Halting system
```

`pc = 0x00000000` with every register zero, and "Illegal use of the EPSR" is
what a Cortex-M reports when it branches to an address whose Thumb bit is
clear. That is a jump through a null or garbage function pointer, not a clean
fault. `35` is not a Zephyr fatal reason code, which fits: the reason argument
is itself corrupt.

## It is the lease, exactly

The fault time tracks `Z_TRANSPORT_LEASE` with no slack:

| `Z_TRANSPORT_LEASE` | publishes before the fault | fault at |
| --- | --- | --- |
| 10000 (default) | 40 | `00:00:20.128` |
| 60000 (`CONFIG_NROS_ZENOH_LEASE_MS`) | 240 | `00:02:00.229` |

Exactly `2 x lease` both times — which is when `_zp_unicast_lease_task` gives
up on a silent peer (first period resets on `_received`, second finds nothing)
and calls `_zp_unicast_failed`.

## Why that path is unsound

`Z_FEATURE_AUTO_RECONNECT` is 1 in this build, so `_zp_unicast_failed` runs
**on the lease task itself** and does this
(`src/transport/unicast/lease.c`):

```c
_z_unicast_transport_close(ztu, _Z_CLOSE_EXPIRED);
_z_unicast_transport_clear(ztu, true);   /* detach_tasks = true */
    /* -> detaches AND `_z_task_free`s the LEASE task's own handle,
       which is the thread currently executing this function,
       and drops _mutex_tx / _mutex_rx / _mutex_peer underneath it */
z_result_t ret = _z_reopen(&zs);         /* then keeps going, builds a new session */
_z_task_exit();
```

The thread frees its own task handle and tears down the transport it is still
running inside, then calls `_z_reopen`. That is a use-after-free by
construction, and it is entered on every expiry.

## What this is NOT

**Not [issue 0822](archived/0822-zephyr-thread-stack-slots-unbounded.md).** That is a
real, separate defect in the same area (thread stacks handed out past the end
of a fixed array). It was found while chasing this fault and fixed, and the
fault still reproduces with slots to spare and no exhaustion diagnostic.

**Not fixed by the router keepalive config.** Setting the router's
`keep_alive` so it speaks every 10 s — inside the board's 20 s tolerance, and
verified on the wire — was predicted to prevent the expiry and therefore the
fault. It did not: the board still faulted at 20 s. Either the config is not
reaching the session under test, or a received keepalive does not reset
`_received` the way the lease task's reset path assumes. **Unresolved, and it
should be settled before anyone trusts the config as a mitigation.**

**Not fixed by `CONFIG_NROS_ZENOH_LEASE_MS`.** Raising the lease only moves
the fault later, exactly proportionally, as the table above shows.

## Next step

The one-flag experiment that would confirm the teardown outright is
`Z_FEATURE_AUTO_RECONNECT=0`: if the board then survives expiry (session
simply ends, no reopen), the fault is in the reopen-from-inside-the-task path
and the fix is to hand the reconnect to a thread that is not the one being
torn down. Not yet run.

## Impact

E2E over serial works — `/talker` visible, `ros2 topic echo /chatter`
streaming — but only inside the window before the first expiry. Any deployment
that runs longer than `2 x lease` without inbound traffic hits this. A
publisher is the worst case, because nothing ever flows back to reset the
lease.
