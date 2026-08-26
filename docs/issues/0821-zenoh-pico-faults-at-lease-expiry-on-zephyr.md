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

## Eliminated (2026-08-27)

**Not `_z_reopen`.** Built with `Z_FEATURE_AUTO_RECONNECT=0`, so the weak
upgrade and the reopen are compiled out of `_zp_unicast_failed` entirely. The
board still faulted, at `00:00:20.133`, identically. So the reopen-from-inside
-the-dying-task shape is real but is NOT what kills it — whatever does happens
in the teardown before that, or in `_z_task_exit` after it.

## The register dump says stack, not control flow

Every register in the frame is zero — r0-r3, r12, lr, **xpsr**, all of s[0..15],
fpscr, pc. `xpsr = 0x00000000` is impossible for a real exception frame; the
Thumb bit alone is always set. So the CPU did not jump somewhere wrong and
fault — it **unstacked an exception frame out of zeroed memory**. That is a
dead or clobbered stack pointer, which reframes this from "null function
pointer" to "a stack was destroyed".

Note also `Current thread: idle`, not the lease task, which fits: the damage is
observed at a context switch rather than at the instruction that caused it.

## Enabling the MPU guard finds a DIFFERENT overflow

`CONFIG_HW_STACK_PROTECTION` was **off** in this image despite
`CONFIG_ARM_MPU=y`, so stack overflows were silent corruption. Turning it on
gets a named fault immediately — but at boot, not at expiry, and in a place
with nothing to do with zenoh:

```
***** MPU FAULT ***** Data Access Violation
>>> ZEPHYR FATAL ERROR 2: Stack overflow on CPU 0
Current thread: 0x2040c818 (main)
pc  0x00433db0  compiler_builtins::arm::__aeabi_memset4
lr  0x0042a6a6  <nros_node::executor::spin::Executor>::assemble
                packages/core/nros-node/src/executor/spin.rs:1396
```

`Executor::assemble` returns `Self { ... }` — the whole `Executor`, with its
fixed-size tables, is built **by value on the caller's stack** and memset
there before being moved. On `main`'s 8 KiB that is close enough to the limit
that adding an MPU guard region tips it over. This is a nano-ros defect in its
own right and is filed separately from this issue's fault.

**And `main` cannot simply be given more stack**, because zenoh-pico's Zephyr
port ties its own task stacks to it:

```c
#define Z_PTHREAD_STACK_SIZE_DEFAULT CONFIG_MAIN_STACK_SIZE
K_THREAD_STACK_ARRAY_DEFINE(thread_stack_area, Z_THREADS_NUM /* 4 */, Z_PTHREAD_STACK_SIZE_DEFAULT);
```

Raising `CONFIG_MAIN_STACK_SIZE` 8192 -> 16384 therefore also quadruples into
4 x 16 KiB of zenoh stacks, and the image fails to link: *"region `RAM'
overflowed by 21588 bytes"*.

## The fault is build-sensitive, which is itself evidence

Built with `CONFIG_THREAD_ANALYZER` + `CONFIG_INIT_STACKS`, the fault **does
not reproduce**: 230 publishes, zero faults across a 120 s run, where the
baseline build dies at 40 publishes. Peak stack use reported at the end:

| thread | usage |
| --- | --- |
| `main` | 4860 / 8192 (59 %) |
| `sysworkq` | 216 / 4096 (5 %) |
| `idle` | 48 / 320 (15 %) |

A defect that disappears when stacks are pattern-filled and a sampling thread
is added is characteristic of **memory corruption / use of uninitialised
stack**, not of a deterministic logic error. Consistent with the all-zero
exception frame: uninitialised stack on this target reads as zero, and a
frame unstacked from it gives exactly `pc = 0`.

Caveat on that run, stated because it weakens the result: the analyzer
enumerated only `main`, `sysworkq`, `idle` and `thread_analyzer` — **no zenoh
read or lease task** — so the session may not have been established at all,
and "230 publishes" may be the app publishing into a dead session rather than
a healthy link. Not yet distinguished.

## Bisected on hardware (2026-08-27)

Every row is a real flash-and-run against a stock `rmw_zenohd`, `Z_TRANSPORT_LEASE`
at the 10000 default, so the expiry lands at 20 s. "publishes" is the app's own
count at 2 Hz, so 40 == died at expiry and 240 == survived the whole run.

| build | publishes | fault |
| --- | --- | --- |
| baseline | 40 | `00:00:20.128` |
| `Z_FEATURE_AUTO_RECONNECT=0` | 40 | `00:00:20.133` |
| expiry teardown skipped (`close`/`clear` compiled out) | 40 | after the skip, before exit |
| teardown restored, `return` instead of `_z_task_exit()` | 40 | **before** reaching the return |
| **lease task parked in `k_sleep` forever, nothing else run** | **240** | **none** |
| `CONFIG_THREAD_ANALYZER` + `CONFIG_INIT_STACKS` | 240 | none |
| `CONFIG_INIT_STACKS` alone | 40 | `00:00:20.131` |

## What that rules in and out

**It is not one call site.** Skipping the teardown moved the fault later
rather than removing it; restoring the teardown moved it earlier, before the
`_z_task_exit` site was reached. Disabling auto-reconnect changed nothing.

**It is not the teardown work, and not thread termination either.** Each was
eliminated on its own. What removes the fault is parking the lease task so it
executes *nothing* after deciding the peer expired — no liveliness undeclare,
no join, no close, no clear, no reopen, no exit.

**So: anything the lease task runs after expiry faults.** That is the shape of
a thread whose stack is already unusable by the time it takes that branch, not
of a bad pointer at one site. It also explains the all-zero exception frame:
an overrun into zeroed `.bss` gives exactly `pc = 0`, `xpsr = 0`.

The lease task's stack is 8 KiB — `Z_PTHREAD_STACK_SIZE_DEFAULT` is
`CONFIG_MAIN_STACK_SIZE`, and the expiry teardown is by far the deepest call
chain that task ever makes (liveliness undeclare -> transport close -> clear ->
several `_z_mutex_drop`s). Parking is the one path that keeps it shallow.

## The datapoint that does NOT fit, recorded rather than buried

`CONFIG_THREAD_ANALYZER` + `CONFIG_INIT_STACKS` survived **six** full
open/close cycles (router confirmed: 6 transports opened, 6 closed, 5 serial
links) with zero faults, while running the full teardown every time. Neither
option changes any stack size. If this were a plain stack overflow that build
should have died too.

`CONFIG_INIT_STACKS` alone does NOT save it, so the protective ingredient is
`THREAD_ANALYZER` — an extra thread plus periodic stack walking, i.e. timing
and `k_thread` bookkeeping (`THREAD_MONITOR`), not stack content. Until that is
explained the "lease task stack" theory is the leading candidate, not the
answer.

## Blocked measurement

The direct check — `CONFIG_HW_STACK_PROTECTION=y` to get a named
"Stack overflow" naming the thread — cannot run: with the guard enabled `main`
overflows first, at boot, in `Executor::assemble`, and `main` cannot be given
more stack because zenoh-pico ties its four task stacks to
`CONFIG_MAIN_STACK_SIZE` (8192 -> 16384 overflows RAM by 21588 bytes).

Breaking that coupling — a separate stack size for zenoh tasks — is what
unblocks this issue, and is worth doing for its own sake.

## Impact

E2E over serial works — `/talker` visible, `ros2 topic echo /chatter`
streaming — but only inside the window before the first expiry. Any deployment
that runs longer than `2 x lease` without inbound traffic hits this. A
publisher is the worst case, because nothing ever flows back to reset the
lease.
