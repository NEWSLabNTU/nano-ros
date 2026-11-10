---
id: 2403
title: "On the emulated Pixhawk 6X (PX4 NuttX), a nano-ros uORB subscription registers and its broker callback fires, but the executor never takes a sample: `vehicle_status` is published by PX4's commander and the demo counts 0"
status: resolved
type: bug
severity: high
area: [px4, nuttx, rmw, executor]
related: [phase-547, 2218, 2225, 1940, 2398, 2410]
found: 2026-11-10
resolved_in: "the issue-2403 pull request: on the PX4 glue build the uORB subscription is held by pointer (a `uORB::SubscriptionCallback` slot), never by file descriptor"
---

**Home: [phase-547](../../roadmap/phase-547-px4-pixhawk-emulator-spike.md)**, W3
(the uORB round trip). Found once [issue 2398](2398-px4-thread-stacks-below-executor-floor.md)'s
stack remedy removed the `mm_free` fault that had stopped the module before.

## What

The firmware example's `nros_uorb_demo` (examples/px4/cpp/firmware) publishes
`debug_key_value` and subscribes `vehicle_status`. On the emulated 6X
(Renode 1.17.0, the phase-547 scripted repro, image `px4_fmu-v6x_wqstk`:
`STACK_MAIN 16384`, `CONFIG_WQ_RATE_CTRL_STACKSIZE=8192`) publish worked
(PX4's `listener debug_key_value` printed `key: "nros"`) and subscribe did
not: commander published `vehicle_status`, the broker called our callback,
and the demo's stats read `status=0` for 30 s of virtual time.

## Root cause

**A uORB subscription handle on NuttX is a file descriptor, and NuttX gives
every task group its own fd table.**

- `orb_subscribe_multi()` returns an fd (PX4's `px4_open` is `::open` on
  NuttX).
- The subscription is created by the transient `nros_uorb_demo start` task:
  `NrosPx4Entry::task_spawn`, then `start()`, then `SchedTaskWorkItem::open()`,
  then the spec's setup. That task exits once the module is started.
- The executor runs on the `wq:rate_ctrl` work-queue thread, in another task
  group. There `orb_check(fd)` fails with EBADF.
- `subscription_has_data` returned `NROS_RMW_RET_ERROR`, and the RMW C shim's
  `has_data` mapped any error to "no data", so `take` was never called
  ([issue 2410](2410-cffi-poll-swallows-backend-error.md)). That is why the
  first measurement read as "the executor never asks".

Publish and the callback worked because neither uses an fd: PX4's
`orb_advertise_multi` closes the fd it opens before returning and hands back a
`DeviceNode` pointer, and `uORB::SubscriptionCallback` registers by pointer.
PX4 SITL never showed it because PX4's POSIX cdev layer keeps one
process-wide fd table.

## Evidence (instrumented scratch build, not committed)

`printf`s in the scratch `subscriber.cpp` (the diff is
`/mnt/wd/nt/p547/repro/d2403-diag.diff`, outside the repository):

```
D2403 create vehicle_status handle=3 pid=296 tid=296 fcntl=1
D2403 has_data vehicle_status handle=3 pid=298 tid=298 ready=1 rc=-1 errno=9 updated=0 fcntl=-1
D2403 resub in executor ctx: new handle=3 pid=298
INFO  [nros_uorb_demo] recv vehicle_status: nav_state=4 arming_state=1 (1 samples)
INFO  [nros_uorb_demo] stats: ticks=30 status=50 jitter_max_us=328
```

Created in pid 296, polled from pid 298: `errno=9` (EBADF) and `fcntl=-1`,
for the same fd number that is valid in the creating task. Re-subscribing
from the executor's own task (a diagnostic hack, never a fix) made samples
arrive.

## Fix

On the PX4 glue build (`NROS_RMW_UORB_PX4_GLUE`) `subscriber.cpp` opens no fd.
`px4_callback_glue.cpp` exports `nros_orb_sub_open` / `_check` / `_copy` /
`_close`, keyed by the glue's slot pointer: the slot's
`uORB::SubscriptionCallback` is both the broker callback and the
subscription (`updated()` / `copy()`), usable from any task. The fd-keyed
`nros_orb_register_callback` pair it replaces had a second defect with the
same root: two modules started from different tasks can both get fd 3, so a
lookup by handle could find the wrong slot. The host test double keeps the
fd API.

The same change also makes the backend compile for PX4 NuttX as committed,
which it did not before: PX4 builds NuttX C++ `-nostdinc++`, so `<atomic>`
does not exist there (`subscriber.cpp` and `session.cpp` now use the
`__atomic` builtins on plain fields), and its `-Wcast-align -Werror` refused
the glue's slot cast (now through `void *`). The phase-547 spike had patched
both in its scratch tree only.

Verified on the emulator with the repository's own files, copied byte for
byte over the scratch tree (`subscriber.cpp`, `session.cpp`,
`px4_callback_glue.cpp`, `uorb_abi.hpp`, `NrosRmwUorbSources.cmake`), no
resubscribe hack; image `build-fix2403-repo`, same `wqstk` config, console
log `/mnt/wd/nt/p547/repro/run-fix2403-repo/console.log`:

```
INFO  [nros_uorb_demo] recv vehicle_status: nav_state=4 arming_state=1 (1 samples)
INFO  [nros_uorb_demo] sched task default runs=14 wake_max_us=3
INFO  [nros_uorb_demo] stats: ticks=10 status=12 jitter_max_us=884764
INFO  [nros_uorb_demo] stats: ticks=20 status=31 jitter_max_us=494276
INFO  [nros_uorb_demo] stats: ticks=30 status=51 jitter_max_us=12066
```

## The class: every nano-ros fd opened in one task and used in another on NuttX

| handle | opened by | used by | verdict |
|---|---|---|---|
| uORB subscription (`orb_subscribe_multi`) | PX4 `start` task | `wq:*` work-queue thread | **exposed; fixed here** |
| uORB publisher (`orb_advertise_multi`) | PX4 `start` task | work-queue thread | safe: PX4 closes the fd before returning and hands back a `DeviceNode` pointer (measured: publish works) |
| uORB broker callback (`SubscriptionCallback`) | PX4 `start` task | publisher's context | safe: registered by pointer (measured: it fires) |
| zenoh-pico socket on PX4 NuttX | PX4 `start` task (the session opens in `start()`) | work-queue thread, and zenoh-pico's read/lease pthreads, which belong to the `start` task's group and end with it | exposed by construction, unverified: no networked backend has been built for PX4 NuttX (the module helper's networked backends run on SITL only) |
| XRCE-DDS UDP socket or serial fd on PX4 NuttX | PX4 `start` task | work-queue thread | exposed by construction, unverified, as zenoh |
| Cyclone DDS sockets on PX4 NuttX | as above | as above | unverified; no Cyclone build for NuttX exists |
| zenoh-pico / XRCE sockets on a plain NuttX app (`nros-board-nuttx`) | the app task | its own pthreads (sched tasks via `pthread_create` in nros-platform-posix) | safe: pthreads share their task group's fd table |
| executor wake pipe (`nros_platform_wake_park_handles_us`, nros-platform-posix) | made lazily by the parking task | written by `nros_platform_wake_signal` from any signaller | safe on plain NuttX (one task group). Latent on PX4 NuttX: a signaller in another task group (the uORB broker runs in the publisher's task) would write a foreign fd number. Not reached today: a uORB session states Notify and a PX4 sched task is woken by `ScheduleNow()`, so no handle park runs. Unverified for a networked PX4 NuttX session |
| `Executor::signal_fd()` (eventfd) | | | not compiled on NuttX (`target_os = "linux"` only) |
