---
id: 447
title: Multi-tier registration races on the shared RMW session, binding topics non-deterministically
status: resolved  # fixed 2026-08-06
type: bug
area: runtime
related: [issue-0422, issue-0438, issue-0458, phase-263, rfc-0032, rfc-0015]
---

## Symptom

`realtime_tiers_e2e`, cell `native/rust`, intermittent (2 of 3 runs):

```
[native rust] high-tier /ctrl counter 0 is not ≥3× the low-tier /telem counter 4
[native rust] low-tier /telem never reached 5 deliveries — the low tier was not scheduled
```

Both spellings appeared across runs, which was the first hint it was not a dead
tier.

## Root cause

`LinuxBoard::run_tiers` spawns every non-boot tier, and each spawned tier
immediately runs the shared `setup` closure on its own thread — while the boot
tier runs the SAME closure right after the spawn loop. Both declare entities on
the ONE shared RMW session, with no synchronization.

The board's `SharedSession` comment asserted this was sound because "the RMW
backend serializes concurrent access through its own locks". That holds for
publish/receive; it does NOT hold for entity DECLARATION.

Five runs of the same binary (`scratchpad/repro-447.sh` — zenohd + two
`int32-sink` observers + the prebuilt entry, 10 s):

```
ctrl 0     telem 0        <- neither tier delivers
ctrl 0     telem 1098     <- the 10 ms stream lands on /telem
ctrl 0     telem 1098
ctrl 0     telem 1098
ctrl 997   telem 98       <- correct
```

The crossed runs are the diagnostic one: `/telem` carried 1095 samples with
`distinct=998` and duplicates from 10 upward — TWO publishers, one at the 10 ms
cadence (only `[tiers.high]` declares 10 ms) and one at 100 ms, both on
`/telem`. The high tier was always scheduled and publishing ~1000 msgs/10 s; its
output was landing on the wrong topic. It also explains why the harness's
low-tier anchor passed so easily in the crossed runs — `/telem` was flooded at
10× its declared rate.

## Fix

Serialize the per-tier `setup` behind a mutex held across registration:
`run_one_tier` and `run_boot_tier` take the same lock, wired through
`run_tiers`. Registration is once per boot and off the hot path; the spin loops
stay fully concurrent.

## Verified

5/5 clean manual runs (`ctrl ~999` @10 ms, `telem ~99` @100 ms) against 3/4
crossed-or-empty before, then `realtime_tiers_e2e` 5/5 green (all 16 rows,
~9.5 s each).

## Ruled out along the way

Each from source, one at a time — recorded so they are not re-run: posix tier
priority is advisory (`run_tiers` prints so and never calls
`sched_setscheduler`); `class = "real_time"` without `budget_us` + `period_us`
(both `[tiers.high.nuttx]`-scoped) makes `from_tier_policy` return `None`, so no
sporadic gating; `deadline_us` is zephyr-scoped; the `groups`/`active_groups`
filter is correct (`resolve_tiers` pushes the BARE group id, giving
`high.groups = ["ctrl"]`); the resolved model binds `/control_node/ctrl: high`;
both node sources are symmetric; and register- and publish-side synthesise the
entity id identically via `EntityId::new(topic)`.

## Process note — worth keeping

One rebuild happened to produce a correct run, and that single observation was
briefly taken as proof the whole thing had been a stale fixture. It was not: the
next four runs of that same binary were crossed. **A race is never cleared by
one green run.** Repeat before concluding, in BOTH directions — the same trap
would have hidden this bug indefinitely.
