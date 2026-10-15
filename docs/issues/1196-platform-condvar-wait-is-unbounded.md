---
id: 1196
title: "`nros_platform_condvar_wait` blocks forever by construction, beside a bounded variant nothing forces you to prefer"
status: open
type: tech-debt
area: platform
related: [issue-1192, phase-436, phase-502, issue-1906, issue-1907, issue-1750]
---

**Home: [phase-502](../roadmap/phase-502-unbounded-condvar-wait-retirement.md)** — the remaining closing condition (Zephyr and FreeRTOS zenoh paths, `zpico_get`, the class gate) after phase-436 D2 took the ABI half.

## Problem

The platform API exposes an unbounded condvar wait on every RTOS port:

```c
int8_t nros_platform_condvar_wait(void *cv, void *m) {
    if (cv == NULL || m == NULL) return -1;
    nros_platform_mutex_unlock(m);
    UINT rc = tx_semaphore_get((TX_SEMAPHORE *) cv, TX_WAIT_FOREVER);
```
(`packages/platform/nros-platform-threadx/src/platform.c:683`; the FreeRTOS and
ESP-IDF ports use `portMAX_DELAY` at `platform.c:702` and `platform.c:423`)

A bounded `nros_platform_condvar_wait_until(cv, m, abstime_ms)` sits directly
beneath it, and the executor's own wait path correctly uses the *bounded*
`nros_platform_wake_wait_ms` instead.

## Correction (2026-09-07): it HAS callers

**The original claim here — "no callers" — was wrong, and the error was in the
grep rather than the reading.** The search run was for
`nros_platform_cond_wait`; the symbol is `nros_platform_condvar_wait`. The typo
produced a clean false negative, and this issue was filed calling a live call
site latent. Phase-436 W5's gate found the real callers on its first run:

```
packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c:302  _z_condvar_wait
packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c:320  _z_condvar_wait_until, NULL deadline
```

Both are **correct as written**, which is why this stays an issue about the API
rather than becoming a bug about those lines. `_z_condvar_wait` is zenoh-pico's
own primitive and its contract is an unbounded wait; mapping an unbounded
upstream call onto the unbounded platform call is the honest bridge, and
narrowing it there would silently change zenoh-pico's semantics. The `_until`
variant falls back only when handed a NULL deadline.

The accurate statement: the executor's **own** wait path is bounded
(`nros_platform_wake_wait_ms`), and the unbounded primitive is reached only
through a shim implementing an upstream API that is itself unbounded.

## Why file it anyway

nano-ros claims real-time properties, and "no unbounded wait" is only a
property of the system if it is a property of the **API**, not of the current
call graph. An unbounded primitive sitting in the platform header with a
neutral name is an invitation: the next port, or the next backend, reaches for
`condvar_wait` because it is the obvious spelling, and the bound is lost
silently and without review.

The mutex waits in the same files (`K_FOREVER`, `portMAX_DELAY`,
`TX_WAIT_FOREVER`) are **not** part of this issue — those are short critical
sections and all three ports have priority inheritance: ThreadX creates with
`TX_INHERIT` (`platform.c:611`), Zephyr's `k_mutex` inherits, and FreeRTOS
recursive mutexes route through `xQueueSemaphoreTake` → `xTaskPriorityInherit`
(`third-party/freertos/kernel/queue.c:1791`). That was checked because
recursive mutexes are a common priority-inheritance exception; here they are
not one.

## Fix direction — DONE in phase-436 W5

Deleting the symbol was rejected: it is exported, out-of-tree ports link it,
and one in-tree caller legitimately needs unbounded semantics.

1. `include/nros/platform.h` marks it **NOT REAL-TIME — this wait is
   UNBOUNDED**, names the bounded alternatives, and says why it was kept.
2. `scripts/check-no-unbounded-condvar-wait.sh` confines it to the zenoh-pico
   alias shim **by path**. A call site anywhere else fails the gate and has to
   be argued for, rather than inheriting a blanket allowance.

Verified by adding a call outside the shim and watching the gate reject it.

## What would close this

Either the unbounded entry point goes away — every remaining caller bridged
through a deadline the caller owns — or zenoh-pico gains a bounded
`_z_condvar_wait_until` upstream and the last legitimate caller disappears with
it. Until one of those happens the gate is the answer, and this issue is the
record of why the exemption exists.

## Design study (2026-10-15): the gate watches one path of three

A read-only study of every port, zenoh-pico's call sites and the vendored
kernels found that this issue looks at the wrong layer. Its findings, each
re-checked against the tree:

**zenoh-pico reaches an unbounded condvar wait by three paths, and the gate
sees one.**

| Platform | Path | Unbounded wait |
| --- | --- | --- |
| POSIX, NuttX, ThreadX | alias shim, `platform_aliases.c` `_z_condvar_wait` | `nros_platform_condvar_wait` (the gate's only subject) |
| Zephyr | the shim's system section is skipped; `zephyr/nros_zenoh_zephyr_system.c` | `pthread_cond_wait` (line 277) |
| FreeRTOS | the full alias TU is excluded; vendored `zenoh-pico/src/system/freertos/system.c` | `xSemaphoreTake(cv->sem, portMAX_DELAY)` |

So the FreeRTOS `nros_platform_condvar_wait` this issue cites has no zenoh
caller at all, and the gate's "confined to the shim" claim is about one third
of the platforms. Our own code has one more: `zpico.c:3969` (`zpico_get`)
calls `_z_condvar_wait` directly. `zpico_get` has no in-tree caller (nros-c
and nros-cpp say so explicitly). Its wait cannot simply take a deadline,
because its reply context lives on the stack and leaving early would be a
use-after-free. That is an ownership problem, and no wake period fixes it.

**Real defects in the implementations underneath, which matter more than the
API shape,** are filed and fixed as issue 1906: the shim's timeout code, the
ThreadX broadcast that woke one waiter, the POSIX/NuttX wall-clock timed wait
and timeouts rounded down. Issue 1907 records the lock-contract gaps: no
priority inheritance on POSIX or NuttX (which removes this issue's premise
that every mutex wait is bounded by PI), and a condvar wait under a
recursively held mutex deadlocking on three ports.

Issue 1750's justification for refusing a foreign thread's signal
(`threadx platform.c`: "every wait here takes a timeout ... so a lost signal
costs latency and nothing else") is false while an unbounded `condvar_wait`
exists. A waiter that loses its only signal there waits forever.

### Recommended design: retire the symbol as RFC-0073 retired `clock_ms`

1. `nros_platform_condvar_wait` becomes a `static inline` in
   `nros/platform.h`: one `condvar_wait_until(now + NROS_PLATFORM_CONDVAR_WAKE_MS)`,
   with a timeout reported as a spurious wake (`0`). Ports stop defining it,
   and the Rust trait gets a default `condvar_wait` that `export_platform!` no
   longer exports. An out-of-tree C port that still defines the symbol gets a
   `NROS_PLATFORM_LEGACY_CONDVAR_WAIT` escape for one release.
2. This is legal and changes no caller. A condvar may wake spuriously, and
   every unbounded caller loops on its predicate (`fifo_mt`, `ring_mt`,
   `sync_group`, `scheduler`, `zpico_get`). The period (100–1000 ms) is not
   about latency. It recovers a lost signal, such as issue 1750's refusals,
   and a counter makes a wait that overruns its expectation observable. The
   power cost is close to zero, because nothing reachable blocks indefinitely
   on these waits in normal operation.
3. Apply the same change to `zephyr/nros_zenoh_zephyr_system.c`. On FreeRTOS,
   route condvars through the shim (a `Z_FEATURE_NROS_PLATFORM_SYNC` guard on
   the zenoh-pico fork, like the existing ALLOC/SLEEP/RANDOM ones) or patch
   the vendored forever-wait there.
4. Delete `zpico_get`, the last unbounded wait we own outright.

Rejected:

- Looping inside the shim only rebuilds the unbounded wait one level up.
- An "infinite" sentinel deadline is the same unbounded wait spelled
  differently.
- A new bounded API upstream is unnecessary: zenoh-pico already has
  `_z_condvar_wait_until`. Worth sending upstream instead: the sites that
  return with the mutex HELD on a wait error, namely
  `_Z_RETURN_IF_ERR(_z_condvar_wait(...))` in `fifo_mt.c:81,116` and
  `ring_mt.c:106`, and the early `return ret` in `sync_group.c:103`.

### What the gate becomes

Two gates replace `check-no-unbounded-condvar-wait.sh`:

- A retired-symbol gate modelled on `check-retired-platform-clock-symbols`:
  nothing defines or calls `nros_platform_condvar_wait` outside the header's
  inline.
- A class gate for forever-waits on a condition or semaphore in code we own:
  `pthread_cond_wait`, `k_condvar_wait`/`k_sem_take` with `K_FOREVER`,
  `portMAX_DELAY` and `TX_WAIT_FOREVER` on anything but a mutex, and
  `_z_condvar_wait(`. Its self-test includes `zpico.c:3969`. Mutex locks stay
  exempt, on the condition that issue 1907's priority inheritance is real on
  every port.

### What closes this issue

Steps 1–4 landed, and both gates green with the old one deleted. That is
phase-sized work, and it should follow issue 1906, because a bounded wait
leans on exactly the timeout codes and broadcast that 1906 fixes.

## Progress (2026-10-15, phase-436 D2): the symbol is out of the ABI

Steps 1 and 2 of the recommended design landed in a slightly different shape,
and the retired-symbol half of the gate with them. Steps 3 and 4 and the class
gate remain, so this stays open.

**What landed.** `nros_platform_condvar_wait` is deleted rather than turned
into a `static inline`: it is gone from `nros/platform.h`, from every C port
(FreeRTOS, POSIX/NuttX, ThreadX, both Zephyr arms), from
`examples/linux/c/custom-platform`, from the cffi export macro and
`CffiPlatform`, and from the Rust `PlatformThreading` trait with its three
bare-metal implementors (esp32-qemu, mps2-an385, stm32f4). The bounded
"timeout is a spurious wake" period lives in the one remaining caller instead:
`_z_condvar_wait` in `zpico-sys/c/zpico/platform_aliases.c` calls
`nros_platform_condvar_wait_until(now + NROS_ZP_CONDVAR_SLICE_MS)` (1000 ms)
once and returns 0 for a signal or an expired slice, -1 for an error, relying
on issue 1906's uniform 0/1/-1 codes. It does not loop, so the rejected
"looping inside the shim" shape is not what this is. Deleting instead of
inlining was chosen because no in-tree caller is left to keep the name for,
and an out-of-tree port that still defines it links harmlessly while one that
calls it fails to compile, which is the point; the cost is that the
`NROS_PLATFORM_LEGACY_CONDVAR_WAIT` escape is not needed.

`scripts/check-no-unbounded-condvar-wait.sh` is now the retired-symbol gate:
no exemption list, and any definition, declaration or call of
`nros_platform_condvar_wait`, or a Rust `fn condvar_wait(`, fails it (verified
by planting a call in the POSIX port). Checked on a native zenoh build: the
linked `_z_condvar_wait` calls `nros_platform_clock_ns` and
`nros_platform_condvar_wait_until` and nothing else, and a service pair round
trips through it.

**What still closes this issue:**

- Step 3: Zephyr's own `_z_condvar_wait` (`zephyr/nros_zenoh_zephyr_system.c`,
  `pthread_cond_wait`) and FreeRTOS's vendored one
  (`zenoh-pico/src/system/freertos/system.c`, `portMAX_DELAY`) are untouched.
  The FreeRTOS half needs a commit on the zenoh-pico fork.
- Step 4: `zpico_get` (`zpico.c`) still waits on `_z_condvar_wait`; on the
  alias-shim platforms that wait is now sliced, but its stack-owned reply
  context is unchanged, and deleting it is still the plan.
- The class gate for forever-waits on a condition or semaphore in code we own.
