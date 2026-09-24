---
id: 1487
title: "`just ci gate` guards the CLI's freshness and not the launch resolver's,
  though both are build inputs `nros sync` hard-fails on — the one check that
  knows only WARNS, and is not in that lane"
status: resolved
type: bug
area: [ci, build, tooling]
severity: medium
found: 2026-09-25
related: [0363, 0409, 0596, 0466, 1450]
---

## What happens

`just ci gate` — the lane CLAUDE.md tells everyone to run before every push —
has six steps:

```
steps=(check::cli-fresh check::fast check::build check::api-parity
       test-unit test-lane-contracts)
```

`check::cli-fresh` fails fast when the in-tree `nros` binary no longer matches
its sources. **There is no equivalent for `nros-launch-resolve`**, and the two
are built by SEPARATE recipes from overlapping inputs: both compile against the
`play_launch` submodule, so a pin move stales both.

The consequence, observed while finishing issue 1450:

1. `git submodule update packages/cli/third-party/play_launch` moved the pin
   forward (`07f0461e` → `9a610488`), because `main` had advanced it;
2. `just setup-cli` rebuilt the CLI, which now embeds `9a610488`;
3. `just check fast` — **354 gates green**;
4. `just ci gate` — red at step 3, `check::build`, in
   `check-template-copy-out`, with:

```
Error: sync: `.../nros-launch-resolve` was built from play_launch 07f0461e
       but this `nros` was built from 9a610488.
```

That is issue 0409's guard doing its job, at the worst available moment: eleven
minutes into a lane, inside a gate about templates, naming neither the recipe
that would fix it nor the step that should have caught it.

## The check exists, knows the answer, and declines to enforce it

`scripts/check-tier-preconditions.sh:285` already runs the right predicate:

```sh
. "$(dirname "$0")/build/launch-resolve-stale.sh"
if nros_launch_resolve_stale "."; then
    echo "check-tier-preconditions: WARNING — nros-launch-resolve is older than its
      own SOURCES. It and the in-tree CLI must agree on an argument list
      (issue 0363 C); a skew surfaces deep in a fixture build, not here."
    echo "  Remedy: just setup-launch-resolve"
fi
```

**Its own comment predicts exactly what happened** — *"a skew surfaces deep in a
fixture build, not here"* — and then prints a warning and continues. Two things
make that the defect rather than a deliberate softness:

* **The reason for warning-instead-of-failing was removed and nobody upgraded
  it.** Issue 0596 rewrote this from `cli -nt resolver` (an mtime test the
  printed remedy could not clear, so failing on it would have been a wall) to a
  CONTENT stamp, and says so in the comment right above: *"Source staleness is
  the real invariant … and it IS clearable."* A clearable precondition with a
  one-line remedy is the kind that should fail.
* **`just ci gate` does not run `tier-preconditions` at all.** It runs at the
  head of `just ci` (tier 1). So on the push lane the warning is not merely
  soft — it never prints. Nothing between the pin move and the eleven-minute
  failure asks the question.

## Why it is the 0363 shape, one mechanism over

`scripts/check-cli-fresh.sh` states the argument for its own existence:

> Issue 0363 — fail FAST when the in-tree `nros` binary no longer matches its
> sources, instead of failing deep inside a lane that happens to use it. …
> **What this file contributes is POSITION.**

The resolver has the same invariant (a binary vs the sources it compiled), the
same remedy (`just setup-launch-resolve`), an existing single-source predicate
(`nros_launch_resolve_stale`, the 0596 consolidation) — and no position. The
asymmetry is the whole bug: one of two binaries built from one submodule is
guarded.

## What it is NOT

* **Not a defect in the stamp.** `nros_launch_resolve_stamp` hashes the tracked
  `.rs`/`Cargo.toml`/`Cargo.lock` of both trees AND records
  `git -C <play_launch> rev-parse HEAD` as `play_launch@pin`, so a pin move does
  invalidate it. Verified by reading the helper; the predicate is correct.
* **Not a defect in the 0409 guard.** Refusing to sync with a mismatched
  resolver is right: a resolver from a different layer-2 checkout does not fail,
  it writes models that are MISSING DATA silently.
* **Not `cli-fresh` being wrong.** It is right, and it is the model.

## What closing it looks like

1. A `check::launch-resolve-fresh` step beside `check::cli-fresh` in `ci gate`,
   implemented over the EXISTING `nros_launch_resolve_stale` — no second
   predicate, which is the thing 0596 consolidated away.
2. It must SKIP, not fail, when no resolver binary exists. A fresh clone before
   `just setup-launch-resolve` is a normal state, and `cli-fresh` already takes
   that position for the same reason (gating on it would make the lane
   unrunnable on a fresh clone, and CI jobs that build no resolver are
   unaffected).
3. Leave `tier-preconditions`' warning where it is. It is a REPORT — "every
   unmet precondition at once instead of one per attempt" — and making one row
   fatal would change that contract. The lane gets the hard check; the report
   stays a report.

## How it was found

Hit three times in one session while landing issues 1436, 1429 and 1450: each
`git submodule update` on `play_launch` re-staled both binaries, `just setup-cli`
was treated as sufficient, and the resolver half surfaced minutes later. The
third time the failure was a 1.6-second red where the same gate had previously
taken 683 seconds, which is what made the cause legible.


---

# RESOLVED — the asymmetry is closed, and the check is where `cli-fresh` is

## What landed

`check::launch-resolve-fresh` (`scripts/check-launch-resolve-fresh.sh`), added
to `ci gate`'s steps immediately after `check::cli-fresh`:

```
steps=(check::cli-fresh check::launch-resolve-fresh check::fast check::build
       check::api-parity test-unit test-lane-contracts)
```

It **implements nothing**. It sources `nros_launch_resolve_stale` — the single
predicate issue 0596 consolidated — exactly as `check-cli-fresh.sh` defers to
`nros source-stamp`. A second implementation of "is the resolver stale" is what
0596 removed, and adding one here would have re-created it.

## Two positions taken deliberately

* **SKIP, not fail, when there is no resolver binary**, and likewise when the
  `play_launch` submodule is not initialised. A fresh clone before
  `just setup-launch-resolve` is a normal state, and a CI job that builds no
  resolver must not go red for it. `cli-fresh` takes the same position for the
  same reason.
* **`check-tier-preconditions`' warning stays a warning.** That script is a
  REPORT — every unmet precondition at once rather than one per attempt — and
  making one row fatal would change its contract. The lane gets the hard check;
  the report stays a report.

## The diagnostic names the CAUSE, not just the state

A stale resolver has two causes needing different remedies in the reader's head,
so the message splits:

* the pin moved (`git submodule update` advanced play_launch) — prints both shas;
* the pin is unchanged and the SOURCES differ — prints one sha and says so.

The first draft printed both shas unconditionally, which for a sources-only
change emitted the same sha twice and read as a contradiction ("built from X,
tree at X, therefore stale?"), sending the reader after the submodule instead of
after their own edit.

It also states the fact that cost three repetitions this session: **a
`git submodule update` on play_launch stales BOTH binaries, and
`just setup-cli` alone is not enough.**

## Verified

* **Negative control on the normal path** (issue 1167), driving BOTH verdicts
  over a throwaway `CARGO_TARGET_DIR`: a stamp matching the tree must read
  fresh, one that does not must read stale. The repo is only READ, so it cannot
  disturb the real resolver, its stamp or any git state.
* **The self-test itself was mutation-tested**: making
  `nros_launch_resolve_stale` return "fresh" unconditionally makes the gate fail
  with *"a stamp that does not match the tree was reported FRESH"*. A self-test
  that cannot fail proves nothing.
* `check-gate-selftests` — which caught the FIRST version of this gate for
  having no self-test at all — now passes with it counted.
* `just check fast` — 354 green, 1 skipped (unprovisioned NVIDIA SPE tree).
* `check-gate-lists` and `check-default-gates-run-somewhere` both OK; the new
  gate is reached by a merge-gating event.

## What this does not claim

The 1.6-second red that exposed this was a resolver already rebuilt by the time
the gate was re-run by hand, so the exact interleaving inside `check::build` was
never reproduced and is not what this closes. What is closed is the reason it
could happen at all: the push lane asked about one of two binaries built from
one submodule.
