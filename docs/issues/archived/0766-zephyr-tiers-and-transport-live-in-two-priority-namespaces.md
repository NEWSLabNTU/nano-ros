---
id: 766
title: "Zephyr tiers are native `k_thread` priorities and its transport is a POSIX pthread on a normalised band — two namespaces, so no reserved band can be stated"
status: resolved
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

## The conversion, READ rather than guessed — 2026-08-23

A Zephyr workspace is now provisioned on this host (`just zephyr setup`, 3.7,
`zephyr-workspace/`), so step 1 of the plan above is done from the source
rather than from memory.

**The whole chain, with the file each link came from:**

| step | value | source |
| --- | --- | --- |
| Kconfig | `CONFIG_NROS_ZENOH_READ_PRIORITY` = 16 | `zephyr/cmake/nros_rmw_zenoh.cmake:184` |
| band | `ZPICO_READ_TASK_PRIORITY` = 16 (default also 16) | `zpico.c:196`, applied at `zpico.c:1414` |
| band → POSIX | `mapped = lo + (span·n·2 + 31) / 62`, `lo=0`, `hi=NUM_PREEMPT-1` | `zpico_posix_set_priority`, `zpico.c` |
| POSIX → native | `SCHED_RR` ⇒ `NUM_PREEMPT - prio - 1` | `zephyr/lib/posix/options/pthread.c:25` (`POSIX_TO_ZEPHYR_PRIORITY`) |

Evaluated for the `ws-rs-realtime-entry-zenoh` image, whose `.config` has
`CONFIG_NUM_PREEMPT_PRIORITIES=15`, `CONFIG_NUM_COOP_PRIORITIES=16`,
`CONFIG_PREEMPT_ENABLED=y`, `CONFIG_POSIX_PRIORITY_SCHEDULING=y`:

```
band 16 → posix 7 (SCHED_RR) → k_thread 7
```

So the transport lands at **`k_thread` priority 7**, against tiers declared at
`[tiers.high.zephyr] priority = 5` and `[tiers.low.zephyr] priority = 10`.
Smaller is more urgent, so:

* `high` (5) is MORE urgent than the transport (7) — it PREEMPTS it.
* `low` (10) is less urgent — correctly below.

That is the same arrangement every other port had before RFC-0079's plans
landed, and nobody chose it here either.

**Both Kconfig gates are ON in this image**, so the earlier worry that a stock
image applies nothing does not hold for the images we actually build. It
remains true for an image that turns either off, and the plan still has to say
what happens then.

### The finding that changes the design: this band cannot be a constant

Every other port's `reserved.transport` is a literal read off the port —
FreeRTOS 4, NuttX 100, ThreadX 14. Zephyr's is **computed from two
image-specific Kconfig values**: `CONFIG_NROS_ZENOH_READ_PRIORITY` (the band)
and `CONFIG_NUM_PREEMPT_PRIORITIES` (the map's range). Change either and the
reserved priority moves. `NUM_PREEMPT_PRIORITIES` in particular is a per-board
Kconfig, not a per-port constant.

So a literal `reserved.transport = [7, 7]` in `[board.priority_plan]` would be
true for exactly one image and quietly wrong for the next — the same shape as
the pins RFC-0079 exists to eliminate, one level up. Zephyr needs either a
DERIVED band (the descriptor states the formula and its inputs, resolved at
configure time when `.config` is known) or a build-time check that reads the
generated `.config` and verifies the pins against it.

That is a real addition to RFC-0079 §4, and Zephyr is the port that forces it.

### Newly visible, and NOT caused by the priority work

With the fixture built, `realtime_tiers`'s `zephyr/rust` row runs here for the
first time — and fails:

```
zephyr/rust: [zephyr rust] low-tier /telem never reached 5 deliveries
             — the low tier was not scheduled
```

`[tiers.*.zephyr]` was never touched by the RFC-0079 work (5/10 before and
after, verified against `a7c1bbf8a~1`), so this is pre-existing and was simply
invisible on a host with no Zephyr workspace. Whether it is the same
tier-vs-transport story the computation above describes is not established —
`low` at 10 sits correctly BELOW the transport at 7, so the obvious explanation
does not fit, and it should be investigated on its own evidence rather than
assumed to be this issue.


## Resolved 2026-08-25 — the deferral is discharged, automatically

The three steps this issue set out are done, and the last one was the gap:

1. **Read the conversion** — done 2026-08-23, out of Zephyr's own
   `lib/posix/options/pthread.c`, not guessed.
2. **Say what a plan means when the Kconfig gates are off** — the resolver
   returns `unapplied` as its own verdict rather than a number, and the checker
   prints `[NO BAND]`. An image that applies no priority has nothing for a pin
   to collide with, and that is neither a pass nor a failure.
3. **Declare the band and measure it** — `[board.priority_plan]` with
   `derived = "zephyr"` (RFC-0079 §4.1), and the four `tiers.high.zephyr = 5`
   violations moved into the resolved pool.

### What was still wrong, and it was mine

`check-tier-priority-plan` reported Zephyr's 8 pins as DEFERRED and told the
reader to run `just check-tier-priority-plan-image` — **a recipe that did not
exist**. A deferral nobody can discharge is an unchecked pin with better
wording, which is precisely the state this issue exists to end.

Fixed:

* The recipe exists (`just check-tier-priority-plan-image [config]`).
* With no argument it checks **every** built image rather than one the caller
  names. A derived band is a property of an image, so "the plan holds" is a
  claim about all of them; letting the caller choose which to prove is how a
  green comes to mean less than it looks.
* It runs at the end of `just zephyr build-fixtures` — the only place `.config`
  files are known to exist, so the deferral is discharged by the lane instead of
  by remembering.
* On a host with no built Zephyr image it SKIPS loudly and repeats that the
  deferred pins stay unchecked, rather than passing (issue 0599's rule).

### Measured

```
check-tier-priority-plan-image: 4 built image(s)
  [ok]   build-ws-c-realtime-entry-smp:      transport [7, 7], pool [8, 14] — 8 pin(s)
  [ok]   build-ws-c-realtime-entry-zenoh:    transport [7, 7], pool [8, 14] — 8 pin(s)
  [ok]   build-ws-cpp-realtime-entry-zenoh:  transport [7, 7], pool [8, 14] — 8 pin(s)
  [ok]   build-ws-rs-realtime-entry-zenoh:   transport [7, 7], pool [8, 14] — 8 pin(s)

tier-priority-plan-image: OK (32 pin-check(s) over 4 image(s))
```

Not vacuous: setting `tiers.high.zephyr = 7` — onto the reserved band — makes it
FAIL, naming the file, the band and both remedies, exit 1; restoring gives exit
0 again.

### What this does NOT close

The two namespaces are still two. Nothing here merges Zephyr's raw `k_thread`
priorities with the transport's normalised-POSIX band; it makes the conversion
between them explicit, resolvable, and CHECKED per image. Whether zenoh-pico's
Zephyr platform should create native `k_thread`s instead of pthreads — which
would remove the conversion rather than describe it — is a larger question and
belongs to whoever takes that on.
