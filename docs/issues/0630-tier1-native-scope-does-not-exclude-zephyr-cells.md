---
id: 630
title: "Tier 1 cannot go green on a host with no Zephyr workspace: `NROS_TEST_SCOPE=native` narrows by test NAME, and one test's cells span every platform"
status: open
type: bug
severity: medium
area: testing, build
related: [issue-0599, issue-0588, issue-0482, issue-0445, phase-329, phase-340]
---

## Symptom

`just ci` — tier 1, the tier CLAUDE.md calls "the default", "host only" — fails
on a host that has never run `just zephyr setup`:

```
TRY 3 FAIL [0.580s] nros-tests::sched_dims_applied_e2e sched_dims_applied

  EdfDeadline/zephyr/c: Test fixture binary MISSING for an in-lane coordinate:
  build/zephyr-workspace-builds/build-ws-c-realtime-entry-zenoh/zephyr/zephyr.exe
  A gated run already asserted this lane's fixtures are built and fresh,
  so this is a broken promise, not an environment skip.
```

Four cells of nine, all Zephyr:

```
CorePin/zephyr/rust
EdfDeadline/zephyr/{rust,cpp,c}
```

Deterministic, not a flake. Minimal reproduction — no `just ci`, no parallel
load, no other test:

```sh
NROS_TEST_SCOPE=native cargo nextest run -p nros-tests --test sched_dims_applied_e2e
#   -> 1 failed
cargo nextest run       -p nros-tests --test sched_dims_applied_e2e
#   -> 1 passed   (ungated, the same cells degrade to skips)
```

## Why it happens

Tier 1 narrows its run by **test name**: `CiLane::Tier1 => RunScope::Native`,
which exports `NROS_TEST_SCOPE=native`. Tier 2 and nightly narrow by
**coordinate** instead (`NROS_TEST_COORDS` → `nros_tests::fixtures::lane`,
phase-340 W3), and that is the path where a fixture outside the lane SKIPS
rather than failing.

`sched_dims_applied` is ONE test over `matrix::SCHED_CELLS`, and that table
spans zephyr / nuttx / threadx / freertos / posix by construction — phase-329 W2
consolidated ten hand-written `*_applied.rs` files into it precisely so a new
row could not be forgotten. So a name-scoped selection either takes the whole
test, every platform included, or none of it. There is no name that means "the
native cells of `sched_dims_applied`".

With `NROS_TEST_SCOPE` set and no `NROS_TEST_COORDS`, the resolver has no
coordinate to test the cell against, so it falls through to the gated-run branch:
a missing fixture is a broken promise and a hard failure. That branch is right —
it is issue 0445's rule, that a run which asserted freshness must not silently
skip — and it is being asked a question it cannot answer here.

This is the exact converse of the tension CLAUDE.md already records for the other
direction (issues 0357/0482): *"Name filtering cannot express tier 2 — it is
1-wise over platform, so every platform is in it."* Name filtering cannot
express tier 1 either, for the same reason and from the other side.

## Why it matters

CLAUDE.md documents tier 1 as the tier anyone can afford to run per task:

> `just ci` — **tier 1**, minutes, host only. The default. Gates and runs only
> native fixtures, so a stale ThreadX fixture cannot block it.

A Zephyr fixture blocking it is that promise not holding. And the failure mode is
the one the tier system exists to prevent: an instruction nobody can follow
honestly gets followed selectively. On this host tier 1 has no green to compare
against, so every subsequent run has to be read by hand — which is how a real
regression gets waved through as "the usual two".

`just check-tier-preconditions` does WARN that "no Zephyr workspace, so the
zephyr fixture lane will SKIP", and then says **"Tier 1 does not need it"**. That
sentence is wrong, and it is the sentence someone reads before deciding not to
run `just zephyr setup`.

## Candidate fixes, unmeasured

Not yet chosen — the shape wants deciding before code:

1. **Give tier 1 a coordinate filter too.** `RunScope::Native` becomes a
   coordinate predicate rather than a name token, so the zephyr cells skip
   through the same `fixtures::lane` path tier 2 already uses. One mechanism
   instead of two, which is the CLAUDE.md preference; the cost is that tier 1's
   scope stops being expressible as a nextest filter string.
2. **Let a per-CELL resolver declare its platform** and skip out-of-scope cells
   before resolving a fixture. Smaller, but adds a second place where "is this
   cell in this run" is decided — the thing issue 0442 warns about.
3. **Exclude the test from tier 1 by name.** Cheapest and worst: it silently
   drops the native cells too (posix core-pin, the ones that DO run here), and
   nothing would say so.

Option 1 looks right and is the most invasive; it should be measured against
what else keys on `test_scope()` before being written.

## Not this issue

The other red in the same run — `workspace_features::case_06_c_lifecycle`,
`ros2 lifecycle nodes` → `ConnectionRefusedError: [Errno 111]` from the `ros2cli`
daemon — is a load flake. It passes solo. Recorded here only so a future reader
of that run's log does not conflate the two.
