---
id: 1514
title: "`gate.yml`'s `check` job was named \"full on PR\" and its header said the
  compile tier runs on pull requests, while the `just check build` step is
  `schedule`/`workflow_dispatch` only — coverage advertised, not delivered"
status: resolved
type: bug
area: [ci, docs]
severity: medium
found: 2026-09-27
related: [1226, 0952, 1487, 1513]
---

## What was wrong

Three statements in `.github/workflows/gate.yml`, and the first contradicted the
other two:

* the step: `- name: just check build (nightly / manual only)` with
  `if: contains(fromJSON('["schedule","workflow_dispatch"]'), github.event_name)`
  — honest, and it does not run on a pull request;
* the JOB it lives in: `name: check (fast on push; full on PR/nightly)`;
* the file header: *"the compile tier runs on PR + nightly only"*.

So the lane advertised full compile-tier coverage on a pull request and delivered
a compile SMOKE (`check compile-smoke`, which really is PR-only). A real
`check::build` break in any PR lands unseen until a nightly.

## Why it is worth an issue rather than a typo fix

This is issue 1226's rule — *"a gate that WORKS is not a gate that RUNS"* — in its
REPORTING form, and the cost was measured the day it was found. Two agent
sessions each hit a red `check::build` in a worktree, and neither could tell from
the workflow whether that lane was supposed to cover their branch:

* one spent its verification budget re-running 23 of 24 build gates individually
  to prove its three reds were an unprovisioned `cyclonedds` and not its diff;
* the other re-ran four, one of which was its own mid-run edit tripping the
  stale-CLI guard correctly.

Both were right to chase them — precisely BECAUSE the PR lane does not run those
gates, a break there would land unseen, which is what one of them said. But the
label is how a contributor decides whether a red is theirs, and this one pointed
the wrong way. `check::build`'s own failure text already warns that "a red count
is not a coverage report"; the job name was making the same mistake one level up.

## What landed

**The labels now say what is delivered.** The job is
`check (fast + PR source gates; full compile tier on nightly/manual)`, and the
header spells out that `just check build` is `schedule`/`workflow_dispatch` only
and that a PR gets a smoke, not the tier.

**A gate so it cannot drift back:** `check-lane-coverage-labels`
(`just check lane-coverage-labels`, fast line, buildless). The EVENTS are
measured from the step's own `if:` and the CLAIM is read from the file, so there
is no authored table to go stale — the failure mode that produced 1513 the same
day.

Scope is deliberately narrow: it catches the specific lie that cost the sessions
— a label claiming the full tier on an event where only a smoke runs. It does NOT
require every step's `if:` to be described in prose; a workflow that documented
each one would drift worse than one that documents none.

Self-tests three verdicts on the normal path (issue 1167): a job name claiming
"full on PR" is caught, a header claiming PR compile-tier coverage is caught, and
— the arm that keeps it honest — a claim of PR coverage is ACCEPTED when the step
really does run on a pull request.

## One thing found by the gate, on its own author

The first corrected header QUOTED the old wrong phrase while explaining it, and
the gate fired on the quotation: it matches the claim as PROSE and cannot tell a
quotation from an assertion. The history is now described rather than quoted,
with that reason written beside it. A prose-matching gate reading its own
changelog is a shape worth remembering.
