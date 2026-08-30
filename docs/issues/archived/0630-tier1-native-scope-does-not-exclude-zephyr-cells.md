---
id: 630
title: "Tier 1 cannot go green on a host with no Zephyr workspace: `NROS_TEST_SCOPE=native` narrows by test NAME, and one test's cells span every platform"
status: resolved
type: bug
severity: medium
area: testing, build
related: [issue-0571, issue-0599, issue-0588, issue-0482, issue-0445, issue-0328, phase-329, phase-340]
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

`just check tier-preconditions` does WARN that "no Zephyr workspace, so the
zephyr fixture lane will SKIP", and then says **"Tier 1 does not need it"**. That
sentence is wrong, and it is the sentence someone reads before deciding not to
run `just zephyr setup`.

## Cause, once found: this is issue 0571 at a fifth site

The three fixes weighed below were all wrong, because the mechanism already
exists. `nros_tests::lane_scope::admits(platform)` was added by **issue 0571**
for exactly this, with a comment that states the problem verbatim:

```rust
// issue 0571 — narrow by LANE here, because no name filter can reach
// inside one test.
if !nros_tests::lane_scope::admits(c.platform) { … continue }
```

0571 found FOUR consolidated matrix consumers that escape both halves of
`lane-filter.sh native` and fixed each: `entry_matrix`, `realtime_tiers`,
`multihost`, `roundtrip_xprocess`. `sched_dims_applied` is a fifth — phase-329
W2's consolidation of ten `*_applied.rs` files, the same shape produced by the
same move — and it was named in neither the fix nor the list.

That is the issue-0328 shape, which is why the two siblings behaved so
differently in one run: `realtime_tiers_e2e` passed in 21.7 s while
`sched_dims_applied_e2e` hard-failed, over the same realtime fixtures, from
adjacent files. The only difference was four lines.

## Fixed (2026-08-16)

`sched_dims_applied` narrows by lane in its cell loop, and reports what it
dropped — 0571's other half, and the half that matters more, since a cell that
vanishes into a green verdict is what let 0572 sit unseen behind a 12-second
PASS:

```
gated:    sched_dims: 1 cell(s) ran, 0 skipped, 11 out of lane
ungated:  sched_dims: 12 cell(s) ran, 11 skipped, 0 out of lane
```

Tier 2/3 are untouched — `NROS_TEST_SCOPE` unset admits everything.

The skip is keyed on the CELL'S PLATFORM, never on "the artifact is missing"
(issue 0445): an admitted platform whose fixture is absent still fails exactly
as hard as before.

### The gate, because the list was the defect

A fifth site existed because the four were a sentence in a doc comment.
`lane_scope::CONSUMERS` and `lane_scope::EXEMPT` are now data, and
`lane_scope::tests::every_cell_iterating_test_is_classified` **recomputes the
candidate set from the sources** — a file that reads a cell list and reads a
cell's `.platform` — and refuses anything in neither list. Each `EXEMPT` entry
carries its reason; today there are four (two coverage gates that boot nothing,
two `native_example_*` tests that filter to `PlatformId::Linux` first).

The predicate is deliberately generous, catching the coverage gates too. A gate
whose own candidate set were hand-maintained would be the defect it checks for
(issue 0196).

Mutation-tested both directions: dropping `sched_dims_applied_e2e.rs` from both
lists fails with *"in neither CONSUMERS nor EXEMPT"*; neutering its `admits`
call while leaving it listed fails with *"listed … but never call
lane_scope::admits"*. It also asserts the predicate matched something at all —
a gate that matches nothing passes forever.

### Still true, and not fixed here

`check-tier-preconditions` says "Tier 1 does not need it" of the Zephyr lane.
That sentence is now correct for `sched_dims_applied`, and it was the sentence
that made this issue's symptom read as an environment problem rather than a bug.
Left alone deliberately: it is right about tier 1 and the wording is upstream's.

## Candidate fixes considered before the cause was found

Recorded because all three were plausible and all three were wrong — the
mechanism existed and none of them was it:

1. give tier 1 a coordinate filter (`RunScope::Native` as a predicate rather
   than a name token). Would not have worked: the missing artifacts are Zephyr
   WEST leaves, which have no `fixtures.toml` row, and `fixtures::lane`
   deliberately fails closed on an unattributable path.
2. a per-cell platform declaration — which is what `lane_scope` already is.
3. exclude the test from tier 1 by name — would silently drop its host cell too.

## Not this issue

The other red in the same run — `workspace_features::case_06_c_lifecycle`,
`ros2 lifecycle nodes` → `ConnectionRefusedError: [Errno 111]` from the `ros2cli`
daemon — is a load flake. It passes solo. Recorded here only so a future reader
of that run's log does not conflate the two.
