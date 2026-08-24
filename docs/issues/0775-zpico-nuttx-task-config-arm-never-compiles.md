---
id: 775
title: "`zpico_set_task_config`'s NuttX arm never compiles — `__NuttX__` is not defined in that TU, so the transport band I added in issue 0736 has never run"
status: open
type: bug
area: rmw, boards, build
related: [issue-0736, issue-0765, issue-0766, rfc-0079]
---

## What is wrong

`zpico.c`'s per-platform arms are selected like this:

```c
#elif (defined(ZENOH_LINUX) || defined(ZENOH_MACOS) || defined(__NuttX__) ||
       defined(ZENOH_ZEPHYR)) && !defined(ZENOH_THREADX)
    ...
#elif defined(__NuttX__)
    zpico_posix_fifo_set_priority(&g_default_read_task_attr, read_priority);
```

**`__NuttX__` is not defined when this file is compiled for NuttX.** It appears
four times in `zpico.c` and is defined nowhere in this repo's build — it is a
NuttX header macro, and this TU does not pull the header that sets it.

So the NuttX arm is dead code, and `zpico_set_task_config` does nothing on
NuttX: the read and lease tasks keep inheriting their creator, which is what the
NuttX `[board.priority_plan]` already records as `transport 100..100
(INHERITED)`.

## How it was proven, and why the first attempt did not prove it

`#warning` in the NuttX arm did not appear in the build log — and that showed
nothing, because `cargo:warning` lines are not surfaced by this lane at all
(count: 0 across a whole fixture build). Errors are.

So: `#error` in the arm → the NuttX fixture built **rc=0**, i.e. the arm is not
compiled. Then the CONTROL that makes that reading valid — `#error`
unconditionally, at the top of the same function → build **rc=101**, message
seen 4 times. The file IS recompiled and `#error` IS surfaced; the conditional
one is simply never reached.

Without the control the first result would have been indistinguishable from a
stale object.

## What this retracts

Issue 0736 records, from me:

> The spread between 46 and 69 is also the control that makes the negative
> result mean something: the knob demonstrably reaches the threads, so "no
> effect on the failures" is a finding rather than a no-op.

**That is wrong.** The knob reaches nothing on NuttX. Both arms of that
experiment — `transport_prio = 1` and `transport_prio = 111` — compiled to the
same image, so 69 vs 46 is run-to-run noise, and the experiment had no control
at all.

The conclusion it supported ("NuttX's TX failures are not caused by priority
ordering") survives on OTHER evidence gathered later and independently: the
publish failures were only 3-6 against 75 deliveries, and the rate shortfall was
isolated to the kernel sporadic server by disabling `apply_tier_sporadic` alone
(4/4 FAIL with it, 3/3 PASS without). But it no longer rests on that spread, and
anyone re-reading 0736 should discount that paragraph.

## Second defect, same neighbourhood, already fixed

`zpico_posix_fifo_set_priority` (added for NuttX in 0736, widened for
Linux/macOS in 0765) was DEFINED inside `#if defined(ZENOH_ZEPHYR) &&
defined(CONFIG_POSIX_PRIORITY_SCHEDULING)`, a block that closes ~120 lines
later. So it existed only on Zephyr while its callers are the NuttX and
Linux/macOS arms — a helper guarded more narrowly than its call sites. Moved
beside the function that uses it (issue 0765's commit); that is what made the
Linux arm compile at all.

## What to do

1. Find what actually identifies NuttX in this TU. `__NuttX__` comes from
   `nuttx/config.h`; either include it, or have the build define a
   `ZENOH_NUTTX` alongside the `ZENOH_LINUX`/`ZENOH_ZEPHYR` family it already
   emits. The family macros are set by `nros-zpico-build`, so that is where a
   NuttX one belongs.
2. Re-run 0736's transport-priority experiment ONCE the knob works, because it
   has never actually been run. It may still show nothing — the later evidence
   suggests it will — but that will then be a measurement rather than an
   assumption.
3. A gate would help more than either: nothing catches a `#elif defined(X)` arm
   whose `X` no build defines. The `#error`-plus-control technique above is the
   cheap manual version.
