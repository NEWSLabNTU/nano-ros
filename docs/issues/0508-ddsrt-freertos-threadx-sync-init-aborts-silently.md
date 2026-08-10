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
