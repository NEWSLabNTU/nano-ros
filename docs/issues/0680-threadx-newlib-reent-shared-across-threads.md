---
id: 680
title: "threadx-riscv64 now links newlib with no per-thread `_reent`: `errno` is shared across ThreadX threads and `malloc` has no lock"
status: open
type: bug
severity: medium
area: boards, platform
related: [issue-0678, issue-0674, issue-0664, issue-0657]
---

## What changed

[Issue 0678](archived/0678-threadx-rv64-cpp-cyclone-emutls-errno-undefined.md)
moved this board off the injected Debian picolibc and onto the C library its own
toolchain ships. That is the right fix — it is the rule Zephyr states for
picolibc ("toolchain-bundled … guaranteed to be in sync"), and it removes a TLS
model mismatch that could not be resolved from the consumer side.

It also changes who owns per-thread libc state, and that half is not wired.

## The gap

picolibc kept `errno` in **compiler TLS**, so every thread got its own for free
(when the compiler supported it — which is what 0678 is about). newlib does not:
its `errno` resolves through `_impure_ptr`, a single global pointer to one
`struct _reent`.

Nothing in this tree points that pointer anywhere per-thread:

```
$ grep -rn "_impure_ptr\|__retarget_lock" packages/ cmake/ --include='*.c' --include='*.h' --include='*.cmake'
(nothing)

$ grep -n "TX_THREAD_EXTENSION_[0-3]$" .../risc-v64/gnu/inc/tx_port.h
193:#define TX_THREAD_EXTENSION_0
194:#define TX_THREAD_EXTENSION_1
195:#define TX_THREAD_EXTENSION_2
196:#define TX_THREAD_EXTENSION_3     <- all four EMPTY
```

So on a multi-threaded image:

* **`errno` is shared.** A failing call on the zenoh/Cyclone RX thread can be
  read by the application thread, and neither gets a diagnostic. This is not
  hypothetical for socket code, which is what this board runs.
* **`malloc`/`free` are not thread-safe.** newlib guards them with
  `__retarget_lock_*`, whose default implementations are no-ops. Note the board
  also has a bump `_sbrk` over a 64 KiB `.heap` (issue 0664) with no free, so the
  exposure is a torn bump pointer rather than arena corruption — smaller, but not
  nothing.

## What ThreadX says to do

ThreadX's mechanism for per-thread library state is `TX_THREAD_EXTENSION` in
`tx_port.h`: the port adds a field to the thread control block and the context
switch saves/restores it. Microsoft's guidance for the BSD layer says the library
"must define errno … in the thread local storage" via `TX_THREAD_EXTENSION_3`, or
another free slot. `TX_ENABLE_IAR_LIBRARY_SUPPORT` is the same idea wired for
IAR's runtime.

For newlib specifically the documented shape is a per-thread `struct _reent`,
`_impure_ptr` swapped on context switch, and `__retarget_lock_*` implemented on
ThreadX mutexes. ThreadX ships no equivalent of FreeRTOS's
`configUSE_NEWLIB_REENTRANT`, so this is integrator work —
[eclipse-threadx#448](https://github.com/eclipse-threadx/threadx/issues/448) asks
for the recommended practice and has no maintainer answer.

All four extension slots on this port are free, so nothing has to be displaced.

## Scope, honestly

This is a pre-existing property of newlib made REACHABLE by 0678, not a
regression 0678 introduced: before it, the board did not link at all on a
provisioned host. Nor is it a reason to go back — picolibc's per-thread `errno`
only worked where the compiler had native TLS, which the provisioned toolchain
does not.

It is also not obviously urgent. `errno` is read on error paths, and the tests
that exist assert delivery rather than error reporting, so a shared `errno`
would most likely surface as a confusing message rather than a failure. That is
an argument about priority, not about correctness.

## Direction

1. Add a `struct _reent` to the board's thread wrapper and swap `_impure_ptr`
   through `TX_THREAD_EXTENSION_3`.
2. Implement `__retarget_lock_*` on `TX_MUTEX`.
3. A test that actually distinguishes it: two threads, one forced into a failing
   libc call, asserting the other's `errno` is untouched. Without that, both the
   bug and the fix are invisible to the suite.

## Sources

* ThreadX errno / `TX_THREAD_EXTENSION` — <https://learn.microsoft.com/en-us/answers/questions/1245073/integrating-bsd-library-with-azure-threadx-rtos>
* ThreadX + IAR runtime TLS — <https://learn.microsoft.com/en-us/answers/questions/490810/azurertos(threadx)-tx-enable-iar-library-support-c>
* ThreadX + newlib reentrancy, unanswered — <https://github.com/eclipse-threadx/threadx/issues/448>
* Zephyr picolibc integration (the "one install" rule) — <https://docs.zephyrproject.org/latest/develop/languages/c/picolibc.html>
* picolibc TLS design + build options — <https://github.com/picolibc/picolibc/blob/main/doc/tls.md>, <https://github.com/picolibc/picolibc/blob/main/doc/build.md>

## Design exploration 2026-08-18 — the mechanism is forced, the hook is a choice

### The libc decides the mechanism, and it rules out the cheap options

`__errno()` in this newlib is three instructions:

```
<__errno>:  auipc a0, _impure_ptr ; ld a0,0(a0) ; ret
```

It returns `_impure_ptr` itself — `_errno` is at **offset 0** of `struct _reent`
(measured). So `errno` IS `_impure_ptr->_errno`, and per-thread errno means
per-thread `_impure_ptr`. Two shortcuts look attractive and are both unsound for
the same reason #0678 was:

* **Override `__errno()` with our own per-thread version.** Misses the `_r`
  objects — `libc_a-writer.o`, `libc_a-unlinkr.o`, `libc_a-execr.o` and friends
  write `ptr->_errno` through the reent pointer they were handed, never through
  `__errno()`. A failing `write()` would set one storage and the app read
  another.
* **Compile with `-D__DYNAMIC_REENT__` so `_REENT` becomes `__getreent()`.** The
  hook exists in `sys/reent.h:792`, but this `libc.a` was built WITHOUT it
  (`__DYNAMIC_REENT__` is not predefined and nothing in `newlib.h` sets it), so
  libc internals keep using `_impure_ptr`. Declaration vs archive again — the
  exact shape that made #0679's attempt link and still be wrong.

So: swap `_impure_ptr` on context switch. That is also what the ThreadX and
newlib documentation describe, and it is what FreeRTOS's
`configUSE_NEWLIB_REENTRANT` does.

### Storage

`sizeof(struct _reent)` = **344 bytes** (`_REENT_SMALL` build), one per thread
that calls libc. Allocate it in `nros_platform_task_init`, which already owns the
thread's memory (it allocates the stack when `stack_mem == NULL`), and keep
newlib's own `_impure_data` as the value for anything we did not create —
ThreadX's system/timer thread and ISR context must still find a valid pointer.

### Where to hook: two candidates

**A — three instructions in `tx_thread_schedule.S`,** right after
`_tx_thread_current_ptr` is stored (line ~104). Cheapest at runtime.
Costs an assembly edit, in a file the board owns a private copy of, repeated for
every port that ever needs this.

**B — `TX_ENABLE_EXECUTION_CHANGE_NOTIFY` + a C function.** RECOMMENDED. The
board's port assembly ALREADY carries the call sites, currently compiled out:

```
tx_thread_schedule.S        call _tx_execution_thread_enter
tx_thread_system_return.S   call _tx_execution_thread_exit
tx_thread_context_save.S    call _tx_execution_isr_enter
tx_thread_context_restore.S call _tx_execution_isr_exit
```

Defining the macro turns them on; `_tx_execution_thread_enter` then reads
`_tx_thread_current_ptr` (already set at that point) and assigns `_impure_ptr`.
The other three are empty stubs. **No assembly changes at all**, and the same
approach works on any ThreadX port, because those call sites are part of the
port contract rather than of our copy.

Neither `TX_ENABLE_EXECUTION_CHANGE_NOTIFY` nor `TX_EXECUTION_PROFILE_ENABLE` is
defined anywhere in the tree today, so this is a free choice.

### Which extension slot

**`TX_THREAD_EXTENSION_3`**, and the apparent conflict is worth writing down
because the two authorities read differently at first glance.

`tx_api.h:632` says *"in ThreadX 5.x, user would define
TX_ENABLE_EXECUTION_CHANGE_NOTIFY and use TX_THREAD_EXTENSION_3 … For Azure RTOS
6, user shall use TX_EXECUTION_PROFILE_ENABLE instead, and SHALL NOT add
variables to TX_THREAD_EXTENSION_3."* That prohibition is about the PROFILE
variables, and the two macros are mutually exclusive in the header's own `#if`.
Choosing `TX_ENABLE_EXECUTION_CHANGE_NOTIFY` puts us in the 5.x arrangement the
note describes, where `EXTENSION_3` is the sanctioned slot — which is also what
Microsoft's BSD-layer guidance says to use for `errno`.

All four slots are empty on this port. (For contrast: the ThreadX **linux** port
already uses `EXTENSION_0` for the pthread id and `EXTENSION_1` for a generic
pointer — irrelevant here, since that board is hosted and glibc's errno is
already per-thread, but it is why "just pick slot 1" is not portable advice.)

### A hazard this touches, currently unguarded

The port's assembly addresses the thread control block by **hand-maintained byte
offsets**:

```
TX_TCB_ID_OFF 0   RUN_COUNT 4   STACK_PTR 8   STACK_START 16
STACK_END 24      STACK_SIZE 32 TIME_SLICE 36 NEW_TIME_SLICE 40
```

Adding a field to `TX_THREAD` can silently invalidate all of them. It does not
here — every offset the assembly actually uses (4, 8, 24, 36) lies in the fixed
prologue that precedes `TX_THREAD_EXTENSION_0` — but that is a fact nobody
checked, and it is CLAUDE.md's hand-mirrored-struct class exactly: a mirror with
no gate drifts on append. Whoever implements this should add
`_Static_assert(offsetof(TX_THREAD, …) == TX_TCB_…_OFF)` for each, in a C TU that
sees both. That is worth doing whether or not #0680 is fixed.

### The other half: `__retarget_lock_*`

Per-thread `_reent` does not make `malloc` safe. newlib guards the arena with
`__retarget_lock_acquire/release`, whose default implementations are no-ops;
they want implementing on `TX_MUTEX`. Exposure is currently narrow — issue
0664's `_sbrk` is a bump pointer over a 64 KiB `.heap` with no free — so the
failure mode is a torn bump pointer rather than a corrupted free list, but a
torn bump pointer hands two threads the same memory.

### Acceptance

A test that can distinguish the fix from its absence, which the suite cannot
today: two ThreadX threads, one driven into a failing libc call (`write()` to a
closed fd sets `EBADF`), asserting the other thread's `errno` is unchanged
across it. Without that, both the bug and the fix are invisible.
