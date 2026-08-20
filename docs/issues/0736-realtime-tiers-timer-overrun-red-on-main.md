---
id: 736
title: "`realtime_tiers` fails on main — the runtime timer contract reports
  overruns the declaration says are impossible (`measured=2 declared=0`)"
status: open
type: bug
area: testing, core
related: [phase-359, issue-0623, issue-0636]
---

## Symptom

```
cargo nextest run -p nros-tests --test realtime_tiers_e2e --retries 0
thread 'realtime_tiers' panicked at packages/testing/nros-tests/tests/realtime_tiers_e2e.rs:594:5:
    [WARN] nros: contract violation: timer-overrun-runtime timer measured=1 declared=0
    [WARN] nros: contract violation: timer-overrun-runtime timer measured=2 declared=0
    [WARN] nros: contract violation: timer-overrun-runtime timer measured=2 declared=0
```

Reproduced 3 of 3 SOLO on an otherwise idle lane, ~47 s per run (the passing
time earlier in the day was ~22 s), so this is not the load flake the QEMU
cells have.

## Not caused by the change that found it

Found while landing phase-359 W10's clock ruling (the `std` clock fallbacks
deleted in favour of the platform API). That change is NOT responsible, and the
check is worth recording because attributing it wrongly would have buried it:

```
git stash push -- packages/core/nros-core packages/core/nros-node packages/api/nros
just setup-cli && just build-test-fixtures lane=native
cargo nextest run -p nros-tests --test realtime_tiers_e2e --retries 0   # FAILS
```

The baseline — upstream `main`, fixtures rebuilt from it, W10's edits stashed
out — fails the same way. Whatever introduced this is in the ~19 commits pulled
on 2026-08-20 (phase-370's FreeRTOS POSIX simulator board, #0726's build-pool
work, #0719's panic-policy applier, #0712/#0713), or it predates them and the
cell has not been run solo in a while.

## What the assertion means

`timer-overrun-runtime` compares a timer's MEASURED runtime against the
`declared` budget from the tier table. `declared=0` means the entry baked no
runtime for that timer, so ANY measurement over zero is a violation — the
monitor is reporting "you told me this takes no time and it took 2".

So there are two candidate readings and they need separating before anyone
"fixes" the number:

1. the declaration is genuinely missing (a bake/codegen regression — the tier
   carries a timer whose runtime the model no longer emits), or
2. the declaration is present but the monitor reads the wrong field, in which
   case `declared=0` is the defect and the measurement is fine.

The distinction is visible in the generated model: check whether the entry's
`nros-plan.json` still carries a runtime for that timer.

## Why it matters beyond one red cell

`realtime_tiers` is the cell that covers the RFC-0052 scheduling dims end to
end on native. While it is red, the tier/deadline/budget surface has no runtime
gate on the fast path — which is exactly the surface issues 0623 and 0636 are
currently arguing about.

## Reproduce

```
just build-test-fixtures lane=native
cargo nextest run -p nros-tests --test realtime_tiers_e2e --retries 0
```
