---
id: 2340
title: "On threadx-riscv64 the generated C++ talker publishes once and then spins in `nros_platform_mutex_try_lock`"
status: resolved
type: bug
area: [cpp, threadx, platform]
severity: medium
found: 2026-11-07
related: [2335, phase-535, phase-548]
resolved: 2026-11-07
resolved_in: "p548-w4-riscv-cpp branch: the riscv64 port's interrupt lockout is a volatile asm with a memory clobber"
---
**Home: [phase-548](../../roadmap/phase-548-three-languages-every-board.md).**


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

## Resolution (2026-11-07, phase-548 W4)

The spinner was a symptom: the ThreadX tick had stopped. Halted ten seconds
in, `_tx_timer_system_clock` read 102 on every sample, `mip` had MTIP pending
and `mie` had MTIE set, but `mstatus.MIE` was clear in thread context
(`0xa00002080` at `nros_cpp_spin_once` entry), so no timer interrupt was ever
taken again. Every wait on a deadline then spun, and the talker's next timer
never fired.

Bisected with breakpoints: MIE was set at `app_main`, through
`nros_cpp_init_rmw`, node, publisher and timer creation, the first
`nros_cpp_spin_once` and the first `nros_cpp_publish_raw`, and cleared on
return from the `printf` that follows it. Single-stepping that `printf`
(3140 instructions) found the last transition in `_tx_mutex_delete`, which
newlib reaches by closing the dynamic lock of `__sbprintf`'s stack FILE.

Cause: the riscv64 port's `TX_DISABLE` is
`__asm__ ("csrrci %0, mstatus, 0x08" : "=r" (interrupt_save) : )`, an asm
with an output and without `volatile`, which GCC may treat as a pure
expression. In `_tx_mutex_delete` it hoisted the lockout at the top of the
"resume the suspended threads" loop above the loop test. With no thread
suspended on the mutex, the loop body, and the `TX_RESTORE` inside it, never
ran, and the function returned with interrupts masked. Upstream ThreadX
6.4.1's `ports/risc-v64/gnu/inc/tx_port.h` has the same macro; our copy in
`packages/boards/nros-board-threadx-port-riscv64/port/inc/tx_port.h` now says
`__asm__ volatile (... : "memory")` for both `TX_DISABLE` and `TX_RESTORE`.
The clobber also stops the compiler moving the protected loads and stores
across the lockout.

Why only C++: this C++ talker formats with `snprintf` and prints with
`printf` from the app thread after the scheduler started, and that call
deleted a mutex. The C talker and the hand-written C++ leaf did not reach
`_tx_mutex_delete` that way. Any thread that deleted an unowned mutex with no
waiters would have hit it.

Measured after the fix: the scaffolded C++ talker (`nros new … --board
rv-virt-threadx --lang cpp --template talker`) prints `Hello World: 1`
through `Hello World: 10` in 10.1 s.

Sweep: `git grep -nE 'asm *\(' -- 'packages/**/*.h' 'packages/**/*.c' | grep
-E '"=' | grep -v volatile` returns nothing now; this macro was the only
interrupt-control asm with an output and without `volatile`.

Found on the way and filed as issue 2355, not changed here, because it is
unproven on any image that runs: `nros-c`'s `critical_section` impl on a target that is neither arm
nor aarch64 drops the restore token and releases with a literal 0, and on
this port 0 is `TX_INT_DISABLE`. This C++ image links no `critical_section`
consumer (`nm` finds no `_critical_section_1_0_*`), so it played no part here.

