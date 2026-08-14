---
id: 579
title: "NuttX: the boot tier never adopts its declared priority, so a
  `[tiers.*.nuttx] priority` ordering can silently invert"
status: open
type: bug
area: platform-nuttx
related: [issue-0251, issue-0263, issue-0246, issue-0572, issue-0570, rfc-0015, rfc-0016, phase-281]
---

## Symptom

`packages/boards/nros-board-nuttx/src/lib.rs` calls `apply_tier_priority` from
exactly one place — `nuttx_run_one_tier`, the SPAWNED path:

```
$ grep -n apply_tier_priority packages/boards/nros-board-nuttx/src/lib.rs
367:fn apply_tier_priority(tier: &nros_platform::TierSpec<'_>) {   # definition
384:fn apply_tier_priority(_tier: …) {}                            # off-target no-op
755:    apply_tier_priority(tier);                                 # spawned tiers only
```

`tiers[0]` — the boot tier, which runs on the main task — therefore keeps
whatever priority the init task was started with, no matter what its
`[tiers.<name>.nuttx] priority` says. The value is parsed, baked into the
`TierSpec`, carried to the board, and dropped.

With the in-tree realtime workspace:

```toml
[tiers.high.nuttx] priority = 110     # tiers[0] — the boot tier
[tiers.low.nuttx]  priority = 100     # spawned
```

the guest runs both at 100. Confirmed twice: the tier table in issue 0570's
crash dump shows `nsh_main` and both pthreads at `PRI 100`, and applying the
boot tier's priority by hand visibly changes which tier wins the CPU.

## Why it matters

The declared numbers are an ORDERING. Dropping one of them does not make that
tier "default" — it silently reorders the set. A spawned tier that declares 105
outranks a boot tier that declared 110, which is the inverse of what the author
wrote, with no diagnostic anywhere.

It lands on the worst tier to get wrong: the boot tier is the SESSION OWNER,
whose spin drives the shared zenoh-pico flush for every other tier (the reason
issue 0246 keeps the sporadic budget off it).

## The SAME BOARD's C arm already does it

`packages/boards/nros-board-nuttx-qemu/c/nuttx_run_tiers.c:562`:

```c
    (void)pthread_setschedparam(pthread_self(), SCHED_FIFO, &bsp);
```

and its header comment states the contract outright: *"the boot tier adopts its
declared RAW priority on the caller thread via `pthread_setschedparam(…)` (the
spawned tiers get theirs at `pthread_create`)"*. So this is not an open design
question for the family — the C and Rust arms of ONE board disagree, and only
the Rust one drops the value.

## The two sibling boards already solved this, differently

* **ThreadX** applies it. `nros_threadx_set_current_priority` exists precisely
  for this, and its own comment states the failure: *"without this, a boot tier
  whose declared priority is numerically above a spawned tier's silently
  inverts"* (`packages/boards/nros-board-threadx/src/entry.rs`).
* **Zephyr** orders around it. `resolve_tiers` sorts so `tiers[0]` is the
  numerically-largest = lowest-priority tier, so the boot tier never needs to
  outrank anything (issue 0251, `entry_tiers.rs`).

NuttX does neither. Whichever answer it takes should be stated in the same
place as the other two, so the next board has a rule to copy rather than a
third invention.

## Not to be confused with issue 0246

That rule forbids giving the session-owning boot tier the kernel SPORADIC
SERVER: a spent budget drops it to `sched_ss_low_priority` and stalls the shared
flush. It is about a mechanism that CAPS CPU. A plain `pthread_setschedparam`
priority caps nothing, and `run_tiers` already computes `boot_is_budgeted` to
keep the budget off the owner independently.

## Found how, and what it is NOT

Found while investigating #572 — and it is **not** #572's cause. That was
#570's `pthread_attr_t` mirror overflow (`__PTHREAD_ATTR_SIZE__` 5 vs the
56-byte kernel struct), diagnosed from outside the guest with an execution
trace. Applying the boot priority during that investigation appeared to "fix"
`/ctrl` and break `/telem`; with the real cause known, that is explained as a
changed caller frame moving where the 36-byte smash landed, not as a scheduling
effect. Recorded because the misreading is easy to repeat: **do not treat this
issue as a lead on a delivery failure.** It is a config knob that is accepted
and discarded.

## Acceptance

* a `[tiers.*.nuttx] priority` on the boot tier either takes effect or is
  refused loudly — never accepted and dropped;
* the Rust arm agrees with the C arm of its own board
  (`nuttx_run_tiers.c:562`), which is the narrowest possible fix;
* the guest states the boot tier's EFFECTIVE priority (the console already
  prints its name, groups and knobs since #572's instrumentation);
* whichever of the ThreadX or Zephyr shapes NuttX adopts is written down as the
  rule for the family, not open-coded a third time.
