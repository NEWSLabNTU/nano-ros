---
id: 508
title: ddsrt freertos/threadx sync init failures still abort with no diagnostic
status: open
type: tech-debt
severity: low
area: rmw
related: [issue-0371]
---

# 0508 — the freertos/threadx ddsrt sync ports abort silently on init failure

**Status:** Open
**Filed:** 2026-08-10
**Affects:** `third-party/dds/cyclonedds/src/ddsrt/src/sync/{freertos,threadx}/sync.c`

## The remaining half of 0371's class

Issue 0371 was a mutex-pool exhaustion that surfaced as a bare `abort()` twenty
seconds later in an unrelated function, because `ddsrt_mutex_init` discarded
`pthread_mutex_init`'s return. The fix (`cyclonedds@5b87ee52`) routed all three
POSIX init entry points through one `sync_init_failed()` helper that names the
object and, under Zephyr, the Kconfig knob to raise.

Sweeping the sibling ports at the time established that **freertos and threadx
already check every init return** — so the *discarded-return* defect, the one
that displaced the crash, has exactly two members and both are fixed. What those
two ports still do is `abort()` with nothing logged:

```c
/* sync/threadx/sync.c */
void ddsrt_mutex_init(ddsrt_mutex_t *mutex)
{
  assert(mutex != NULL);
  if (tx_mutex_create(...) != TX_SUCCESS)
    abort();          /* <- says nothing */
}
```

That is strictly better than the old POSIX behaviour: the abort is at the failure
site, so the stack names `ddsrt_mutex_init` and a reader can work out what
happened. It is still worse than one line of text saying which object ran out of
what.

## Why it is filed low, and what it needs

Neither port has the two things that made the POSIX message worth writing: a
named Kconfig pool to point at (FreeRTOS and ThreadX allocate these from their
own heaps or caller-provided control blocks, so the remedy is not a single knob)
and a `printk` equivalent that is safe at that point. So the useful message is
smaller — object kind plus the RTOS error code — and the work is mostly deciding
how to emit it without pulling a logging dependency into ddsrt's lowest layer.

Worth doing when either port next gets attention, not on its own. The class that
actually caused a multi-hour debug (a failure displaced from its cause) is closed.

If it is picked up: put the message behind one shared helper per port rather than
inlining it at each `abort()`, for the reason CLAUDE.md gives — the POSIX port
had three init sites and fixing them separately would have produced three
spellings.

## Fixed in the fork, awaiting the pin (2026-08-12)

`cyclonedds@8601ca66` on the `nano-ros` branch, one helper per port as prescribed
above. **Still open** deliberately: the fork commit is not pushed, so the
superproject pin cannot advance (push the fork branch FIRST, then bump the
pointer), and until it does no nano-ros build consumes this. Flip to `resolved`
with the pin bump.

### The open question answered itself

The issue said the work was "mostly deciding how to emit it without pulling a
logging dependency into ddsrt's lowest layer". No decision was needed: **both
files already `#include "dds/ddsrt/log.h"` and already use `DDS_FATAL`** for an
unrecoverable sync failure a few lines below the init sites
(`ddsrt_mutex_lock`/`unlock`, freertos 65/79, threadx 36/50). So the fix adds no
facility either port did not already depend on — which also keeps fork
divergence minimal, per issue 0507.

Checked rather than assumed: `dds_log` aborts whenever the category includes
`DDS_LC_FATAL` — in `vlog`, after the sink call and OUTSIDE any level filter — so
`DDS_FATAL` alone preserves the unconditional abort these sites made explicitly.
A level-filtered fatal would otherwise have turned an abort into a fall-through
into code that assumes the object was created.

Residual exposure, unchanged by this: `vlog` takes the sink lock, so a sync-init
failure DURING log-subsystem bring-up would take a lock while reporting that a
lock could not be made. The pre-existing `DDS_FATAL` calls in the same files
share that exposure, so this adds no new class — noted because it is the reason a
raw `printk` was used on the Zephyr POSIX path instead.

### What each port can say, and the verification

ThreadX carries the `tx_*_create` status. `ddsrt_cond_init`'s two creates were a
single `||`, which could only report "one of these two failed"; split into two
`if`s so the message names which, with short-circuit behaviour unchanged.
FreeRTOS has no status to carry — neither `xSemaphoreCreateMutex` nor
`ddsrt_tasklist_init` returns one and both fail only for want of memory — so the
object name IS the diagnostic there: it says which heap to look at
(`configTOTAL_HEAP_SIZE` vs ddsrt's).

Sweep of the class, all four sync ports: every init site now names what failed.
`zephyr` needs nothing (`k_mutex_init`/`k_condvar_init` cannot fail). The aborts
that remain in `posix` and `freertos` are lock/wait-path aborts, not init.

Build coverage is asymmetric and worth recording, because it decides what a
future edit here can rely on: the ThreadX TU compiles in the riscv64 ThreadX
cyclonedds build (`WITH_THREADX=ON`) and was cross-compiled clean to verify.
**Nothing in nano-ros configures `WITH_FREERTOS`** — there is no
freertos×cyclonedds fixture, and the threadx-**linux** cyclone builds use the
POSIX port (pthreads are present there), so that TU has no in-tree build to
join. It was checked with `gcc -fsyntax-only -Wall -Wextra
-DDDSRT_WITH_FREERTOS=1` against the vendored kernel headers plus the generated
ddsrt config from the riscv build tree — zero diagnostics — which is the strongest
check available without wiring a configuration that does not otherwise exist.
