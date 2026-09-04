---
id: 1040
title: "`check-api-parity` runs in NO workflow and `check-build` only on
  schedule/dispatch, so reds there accumulate until someone happens to run the
  full tier locally — five landed on main in one day"
status: open
type: task
area: [ci, process]
related: [1035, 1021, 0952]
---

## The measurement

**2026-09-04. Five reds were sitting on `main`, found in one day, all by the
same mechanism: a person ran `just ci gate` locally.**

| red | landed with | lane that sees it |
| --- | --- | --- |
| `error: this function has too many arguments (8/7)` (`nros-cli-core`) | phase-397 depend ladder + phase-413 W2 insertion | `check::build` |
| `check-api-parity`: `cpp:declared_depth` unclassified | phase-403 step 2 (`3d852da49`) | `check::api-parity` |
| `error: items after a test module` (`board_facts.rs`) | `e453399de` (phase-400 W1) | `check::build` |
| `check-api-parity`: 3 release-jitter items unclassified | `55324cc33`, landed 02:37 the same day | `check::api-parity` |
| zenoh-pico 1.8.0 does not build on NuttX ([#1035](1035-zenoh-pico-1-8-0-true-in-preprocessor-breaks-nuttx.md)) | PR #299 | **no lane at all** |

Plus a sixth of a different kind, in the same window: `other.json` carried
`cpp:declared_depth` **twice** — two sessions classified one symbol
independently, git merged both (different regions, no textual conflict), and
JSON parsers keep the last silently. Now gated in `api-parity.py`.

## The two facts, verified in `.github/workflows/`

1. **`check-api-parity` runs in NO workflow.** `grep -rl api-parity
   .github/workflows/` returns nothing. Not on `pull_request`, not on
   `merge_group`, not nightly, not on dispatch. It exists only as a local
   recipe.
2. **`check-build` runs on `schedule` / `workflow_dispatch` only**
   (`gate.yml:740`). That is deliberate and correct — CLAUDE.md records why:
   it needs generated bindings and prebuilt `.compile-ok` that no CI job
   builds, so it was red for every pull request for a day when it was
   required.

So the *required* `CI` context is `check-fast` + `test-unit` + `check-cli-tests`,
and everything the compile tier and the parity ledger see is invisible to it.

## Why this is not "just run the tier"

`just ci gate` being stronger than the merge gate is the DESIGN, and it is a good
one: the queue stays cheap and always-satisfiable, and the person making a change
catches compile-tier breakage before the queue does. CLAUDE.md says so directly.

The gap is narrower than "CI is too weak": **nothing REPORTS on these lanes
between local runs.** A red there is not blocked, not announced, and not visible
in any dashboard — so its cost is paid by whoever next runs the full tier, who
then finds five unrelated failures with five unrelated owners and has to fix or
route all of them before their own work can be gated. That is exactly what
happened here, and it is a poor trade: the person who broke it pays nothing, the
person who runs the tier pays everything, and the delay means the author has
moved on.

A red lane also loses signal capacity, which CLAUDE.md already records for the
nightly (issue 0878): once a lane is habitually red, a NEW regression in it looks
exactly like yesterday's.

## What would fix it, in rough order of cost

1. **Report, do not gate.** Add `check-api-parity` (and `check-build`, which
   already has a nightly step) to a scheduled run whose only job is to
   ANNOUNCE — `just nightly-triage` already exists for classifying nightly
   failures, and `queue-triage` for merge-queue ejections. Neither covers this.
   Cheapest, and it does not risk deadlocking the queue the way a required
   check with unbuildable prerequisites did.
2. **Make `check-api-parity` affordable enough to gate.** It re-extracts our
   surface with clang + nightly rustdoc, which is why it is not on the fast
   line. Worth measuring whether the LEDGER half alone (the part that catches
   an unclassified row) can run buildless — that is the half that caught four
   of the five.
3. **A NuttX compile on some lane.** #1035 is the one red above that no lane
   would have caught at any frequency, and it is the most expensive kind: a
   platform that does not build at all, merged and unnoticed. It need not be a
   full fixture build — `cargo build -p zpico-sys --target armv7a-nuttx-eabihf`
   would have caught both of its breaks.

## Not proposed

Making these required checks. That is what put `check-build` on the merge group
and made it red for every PR for a day, because it resolves artifacts no CI job
builds — the failure mode `check-lane-contracts` now gates against. The problem
here is reporting, not enforcement.
