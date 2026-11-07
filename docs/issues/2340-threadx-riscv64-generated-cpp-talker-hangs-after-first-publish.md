---
id: 2340
title: "On threadx-riscv64 the generated C++ talker publishes once and then spins in `nros_platform_mutex_try_lock`"
status: open
type: bug
area: [cpp, threadx, platform]
severity: medium
found: 2026-11-07
related: [2335, phase-535]
---

## What happens

Phase-535 W5 built `examples/workspaces/cpp`'s pubsub images for
threadx-riscv64 (`riscv_threadx_talker` / `riscv_threadx_listener`). With
issue 2335 fixed they boot, and against a router on the cell's port the
talker prints exactly one line and goes silent:

```
[app_thread] Calling c_app_main (FFI)...
Publishing: 'Hello World: 1'
```

The rtos_e2e C++ pubsub cell then hears 0 of 70 samples. Halted under gdb
ten seconds in, the CPU is in `_tx_mutex_get` called from
`nros_platform_mutex_try_lock`, in a thread that keeps calling it, which
looks like a busy retry against a holder that never runs.

The C++ SERVICE images of the same workspace and configure pass their cell,
the C pubsub images pass theirs, and the hand-written C++ talker leaf
(`examples/rv-virt-threadx/cpp/talker`) publishes forever. So it is specific
to the generated C++ entry's timer/publish path on this board.

## Where it stands

The threadx-riscv64 C++ pubsub cell keeps resolving the leaves
(`examples/rv-virt-threadx/cpp/{talker,listener}`); the C++ pubsub images
were not added. Retiring the C++ listener leaf waits on this.

## Next step

Get the full thread list at the hang (`info threads` needs a ThreadX-aware
gdb script; failing that, dump `_tx_thread_created_ptr` and each thread's
state and saved PC), find who holds the mutex the spinner wants, and why the
holder does not run: a priority inversion between the app thread and the
zenoh read/lease tasks would match.
