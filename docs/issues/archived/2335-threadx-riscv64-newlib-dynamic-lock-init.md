---
id: 2335
title: "threadx-riscv64's newlib `__retarget_lock_init` read `*lock` instead of allocating one, so the first unbuffered `fprintf` faulted"
status: resolved
type: bug
area: [boards, threadx, libc]
severity: high
found: 2026-11-07
related: [0680, 1753, phase-535]
resolved: 2026-11-07
resolved_in: "phase-535 W5 (threadx-riscv64) PR: the init hooks hand out a lock from a static pool"
---

## Measured

Phase-535 W5 gave threadx-riscv64 its first generated C++ entries
(`examples/workspaces/cpp`'s `riscv_threadx_<role>` images). Every one of
them died as soon as it reached its router:

```
[app_thread] Calling c_app_main (FFI)...
[EXCEPTION] mcause=0x5 mepc=0x800695da mtval=0xefefefefefeff04f
  Load access fault
```

Under gdb (QEMU `-gdb`, `riscv-none-elf-gdb`) the fault is in
`__retarget_lock_init_recursive`, called from newlib's `__sbprintf`, with
`*lock == 0xefefefefefefefef`: ThreadX's stack fill pattern.

## Code site

`packages/boards/nros-board-threadx-qemu-riscv64/c/reent.c` (issue 0680's
retargetable locks). `__retarget_lock_init(_LOCK_T *lock)` and its recursive
twin did `nros_lock_ensure(*lock)`: they READ `*lock` as an existing lock.
newlib's contract is the other way round: `*lock` is an output and the hook
allocates. The static locks never come through init (newlib names the
`__lock___*` objects directly), so every caller is a dynamic lock. Most of
those sit in newlib's zeroed FILE glue, where `*lock` happened to be NULL and
the read was harmless. `__sbprintf` (an unbuffered stream's `fprintf`) puts
its FILE on the stack, so `*lock` is whatever the stack held. The hand-written
C/C++ leaves never took that path; the generated C++ entry does.

## Resolution

The init hooks hand out a lock from a 16-entry static pool
(`NROS_LIBC_DYNAMIC_LOCKS`) under the interrupt lockout, and `close` returns
it. An exhausted pool stores NULL, which every hook treats as "no lock", the
state the zeroed FILEs had before. The mutex itself is still created lazily on
first acquire.

Verified: the C++ `riscv_threadx_service_{server,client}` images pass the
rtos_e2e service cell (they faulted before), and the errno-isolation probe
(the fixture that found 0680's lock bugs) still passes. The C++ talker gets
past the fault and then hangs after its first publish, which is a different
defect (issue 2340).
