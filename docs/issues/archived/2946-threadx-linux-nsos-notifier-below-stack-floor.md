---
id: 2946
title: "threadx-linux: the nsos receive notifier's 16 KiB stack is below the port's thread floor, so it never starts and no receive notification is installed"
status: resolved
type: bug
area: [threadx, platform, net]
severity: high
found: 2026-11-24
resolved: 2026-11-24
resolved_in: "nsos-netx: the notifier's stack is raised to TX_MINIMUM_STACK; its cmake build takes the kernel's defines; sched-task stacks are raised too"
related: [2800, 0667, 2896]
---

## What happened

The nros12 release `run-matrix` run 45351389552 (tier 2 nightly, `main` at
0d9fa3e470) failed
`sched_dims_applied_e2e::threadx_thread_census_is_placed_by_the_priority_plan`:

```
[threadx-linux rust] no `nsos-rx-notify` in [("System Timer Thread", 0), ("nros_app", 16), ("nros_level", 15)]
```

It was the test's first run in any tier-2 nightly, so no earlier result exists
to compare against.

## Cause

Issue 2800 (2026-11-22) made each Linux thread run on its own ThreadX stack,
so every stack must be one the host can run a thread on. The board's
`config/tx_user.h` sets `TX_MINIMUM_STACK` to the host's `PTHREAD_STACK_MIN`
plus 128 bytes: 16,512 on x86_64, 131,200 on aarch64.

`nsos_rx_notify.c` creates its watcher thread on a static stack of
`NSOS_RX_NOTIFY_STACK`, 16 KiB = 16,384 bytes. That is below the floor on
every architecture, so `tx_thread_create` refused it with `TX_SIZE_ERROR`.
`nx_bsd_socket_set_rx_notify` then failed with `ENOMEM`, and
`nros_platform_tcp_set_rx_notify` refused zenoh's install. Nothing reported
it: the session runs on, but no receive notification ends the executor's
park in zenoh CALLER mode, and the thread the census looks for never exists.

Measured on host 142 (aarch64), with `zpico_set_link_rx_notify`
instrumented: the probe succeeds (`cb=NULL`, session open, TCP socket), and
the install with the executor's callback reaches the platform and is refused.

The phase-581 W6 renames, the first suspect, are not involved.

## Same class, same fix

- **The cmake road built `nsos_netx` without the kernel's defines.**
  `nros_threadx_build_netstack_nsos` passed no `DEFINES`, so a C/C++ image's
  `nsos_rx_notify.c` compiled without `TX_INCLUDE_USER_DEFINE_FILE`. There it
  saw the port's own `TX_MINIMUM_STACK` and a `TX_THREAD` without the board's
  `TX_THREAD_USER_EXTENSION` (`int bsd_errno`): a different layout from the
  kernel's. It now gets `NROS_THREADX_DEFINES`, and the TU refuses to compile
  without `TX_INCLUDE_USER_DEFINE_FILE`.
- **A sched task's stack was not raised to the floor.**
  `nros_threadx_create_task` passed a stated stack straight to
  `tx_thread_create`, while the boot app thread beside it raises its own
  (`NROS_THREADX_RAISED_STACK`). On aarch64 the realtime image's level `high`
  (65,536 bytes stated) failed with `TX_SIZE_ERROR` and never ran:
  "nros: failed to spawn level `high`". It is raised now, and the boot log
  says so, as the app thread's does. A stated stack is a floor the port
  raises (issue 0667).

## Fix

- `nsos_rx_notify.c`: the watcher's stack is the larger of
  `NSOS_RX_NOTIFY_STACK` and `TX_MINIMUM_STACK`.
- `nros-threadx.cmake`: `nsos_netx` gets `NROS_THREADX_DEFINES`.
- `threadx_hooks.c`: `nros_threadx_create_task` raises a stated stack below
  `TX_MINIMUM_STACK` and logs it.

## Verified (host 142, aarch64)

- `examples/workspaces/realtime-rust` `demo_bringup:threadx`, run against a
  router: the census is `System Timer Thread` 0, `nros_app` 16,
  `nsos-rx-notify` 31, `nros_level` 15. Both levels publish. Before the fix:
  no notifier on the x86-shaped path, and on aarch64 no level `high` either.
- `examples/threadx-linux/c/talker` (cmake road) builds and publishes.

## Not fixed here

That C talker prints "/rosout could not be created for node /talker
(Transport(PublisherCreationFailed))". It does the same on unchanged `main`,
so it predates this change.
