---
id: 766
title: "Zephyr tiers are native `k_thread` priorities and its transport is a POSIX pthread on a normalised band — two namespaces, so no reserved band can be stated"
status: open
type: limitation
area: boards, rmw, zephyr
related: [rfc-0079, issue-0623, issue-0626, issue-0506]
---

## The two halves, and the units each speaks

**Tiers — native, raw.** `nros-board-zephyr`'s `entry_tiers.rs` creates one
`k_thread` per tier at the RAW Zephyr priority, straight from
`[tiers.<name>.zephyr] priority`:

> one `k_thread` per priority tier … `k_thread_create` on a static pool, RAW
> Zephyr priority — negatives = cooperative, exactly the
> `[tiers.<name>.zephyr].priority` value

Zephyr counts SMALLER as more urgent, and NEGATIVE as cooperative — a range
that is not merely inverted relative to FreeRTOS/NuttX but signed.

**Transport — POSIX, normalised.** zenoh-pico's Zephyr platform does not create
native threads. `src/system/zephyr/system.c`'s `_z_task_init` calls
`pthread_create`, so the read and lease tasks are Zephyr POSIX-layer threads.
Their priority, when it is set at all, goes through
`zpico_posix_set_priority(attr, normalized)`: a NORMALISED 0–31 band, mapped
round-to-nearest onto `[0, CONFIG_NUM_PREEMPT_PRIORITIES - 1]`, applied with
`SCHED_RR` (round-robin deliberately, so a busy transport at a high priority
cannot starve its level).

## Why that blocks a `[board.priority_plan]`

RFC-0079's `reserved.transport` is a range in the SAME units the tiers are
written in — that is the whole point of it being checkable. On FreeRTOS, NuttX
and ThreadX both sides are raw kernel priorities, so the band is a fact you can
read off the port. On Zephyr the two sides are:

| | created by | units | direction |
| --- | --- | --- | --- |
| tiers | `k_thread_create` | raw `k_thread`, signed (negative = coop) | smaller = more urgent |
| transport | `pthread_create` | normalised 0–31 → `[0, NUM_PREEMPT-1]`, `SCHED_RR` | larger normalised = more urgent |

Stating a band in either unit without converting is precisely issue 0623 — two
scales meeting in one scheduler — re-authored inside the mechanism built to
prevent it. The conversion exists (Zephyr's POSIX layer maps a pthread priority
onto a `k_thread` priority), but **it is Zephyr's, not ours**, and it has to be
read out of Zephyr's `lib/posix` rather than guessed, because guessing the
direction is the failure mode.

## A second problem underneath: on a default image the priority is not set at all

`zpico_set_task_config`'s Zephyr arm is doubly gated:

```c
#if defined(CONFIG_POSIX_PRIORITY_SCHEDULING)
    zpico_posix_set_priority(&g_default_read_task_attr, read_priority);
#else
    /* No POSIX scheduling option in this image … Stack size only. */
#endif
```

and `zpico_posix_set_priority` itself opens with

```c
#if !defined(CONFIG_PREEMPT_ENABLED)
    /* No preemptive priorities to place a task on. */
    return;
#endif
```

`CONFIG_POSIX_PRIORITY_SCHEDULING` is an EXPERIMENTAL Zephyr symbol and is off
by default — the same fact issue 0626 records as the reason
`sched_get_priority_{min,max}` could not be called there. So on a stock image
neither gate passes, nothing is applied, and the transport threads inherit
whichever thread created them. That is the NuttX situation before issue 0736,
one kernel over: a band that exists by inheritance and that no image can state.

Which means a Zephyr plan must first answer *"is the priority applied in this
image at all?"* — a per-image Kconfig question, not a per-port constant. None of
the other four ports has that property.

## Not verifiable here

This host has no Zephyr workspace (`just zephyr setup` never run), so every
Zephyr fixture SKIPs and the `zephyr/*` rows of `realtime_tiers` and
`sched_dims_applied_e2e` report nothing. A change to this seam could be written
but not measured, and a seam change justified by reading rather than by
measurement is what issue 0636 spent three sessions correcting.

## What it blocks, and what it does not

Blocks: RFC-0079 §4 for `zephyr` — 8 of the 19 remaining `UNPLANNED` pins.

Does NOT block the rest of RFC-0079. FreeRTOS, NuttX and ThreadX declare plans
and are enforced; Zephyr's pins are reported as unchecked rather than assumed
correct, which is the honest state and is visible in every run of
`check-tier-priority-plan`.

## The order of work, when someone takes it

1. Read Zephyr's POSIX→native priority conversion out of Zephyr's own source
   and record it, with the file it came from, the way each band in a
   `[board.priority_plan]` already cites its source.
2. Decide what a plan says for an image where neither Kconfig gate is on. The
   NuttX precedent is to record the INHERITED value and say it was inherited
   rather than chosen.
3. Only then declare the band — and measure it on a host that can build Zephyr.
