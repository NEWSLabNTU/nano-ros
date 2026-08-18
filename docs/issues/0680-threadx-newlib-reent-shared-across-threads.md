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
