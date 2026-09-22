---
id: 1453
title: "`check-template-copy-out` sat red on `main` unseen because it runs on
  `schedule` only — the defect's class is gated cheaply now, the lane's
  reachability is an open cost decision"
status: open
type: tech-debt
area: testing, build
severity: medium
found: 2026-09-22
related: [issue-1108, issue-1429, issue-1226, issue-0319, issue-1040, issue-1412, phase-452]
---

## What was measured

`just check template-copy-out` was **RED on clean `origin/main`, on three of its
six buildable templates** — `pure-c-workspace`, `c-and-cpp-mixed-workspace`,
`multi-node-workspace-cpp`, i.e. every template a user copies out which builds a
C or C++ workspace. Every one of them died the same way:

```
thread 'main' panicked at packages/rmw/zenoh/nros-zpico-build/src/runner.rs:304:23:
NROS_DECLARED_TL_PUBLISHERS="" is neither a count nor `refused`.
```

That defect is archived issue 1429 and was fixed in PR #1151: an UNSET cmake
property leaves `get_property`'s output variable **undefined**, so an unquoted
`if(NOT _tl STREQUAL "")` compares the literal string `"_tl"` against `""`,
which is never equal — the guard that exists to SKIP an absent fact takes its
branch and emits a carrier with no value.

**Nobody saw the red**, and the reason is placement, not attention.
`template-copy-out` is a member of the `check build` gate list
(`just/check/fixtures.just`), and `check build` runs on `schedule` /
`workflow_dispatch` only — the step in `.github/workflows/gate.yml` is named
"just check build (nightly / manual only)". No `pull_request` and no
`merge_group` event asks the question, so the three templates a stranger copies
were unbuildable on `main` and every merge-gating lane stayed green over it.

This is issue 1226's shape exactly (a gate that WORKS is not a gate that RUNS)
and issue 0319's before it (a backend's own suite that `just check` never
reached, red on main for two days).

## Why it matters more than an ordinary red lane

`examples/templates/` is the one part of the tree whose contents are **copied by
a stranger**. Being wrong there is replicated rather than merely observed: the
user's first build of their own project fails, in code they now own, with a
panic naming an environment variable they have never heard of. That is the
argument `scripts/check-template-copy-out.sh` was written on (issue 1108), and
it is undiminished by the gate existing — an unreachable gate protects nobody.

It also has no signal capacity while red. A uniformly-red lane cannot report a
regression, because the new failure looks exactly like yesterday's; that is the
mechanism by which issue 0876 rode into the nightly unnoticed.

## The class is gated now — cheaply, and that is the good half

PR #1151 did not stop at the site. `check-declared-fact-carriers` gained **rule
4 (VALUED)**: a `get_property` output variable may not be compared against `""`
unquoted. Checked as the IDIOM rather than as a name, with five self-test cases
including the two-character difference between the broken and the correct
spelling.

That rule runs on the **fast line**, statically, in milliseconds, and it is
merge-gating. So the specific defect cannot recur silently, and it cannot recur
at any of its siblings either — the sweep found three sites sharing the idiom,
two of which were correct only by accident (one because every road emits a node
count, one because an abstaining road sets a `..._UNKNOWN` companion and the
`AND` short-circuits).

**This is the right shape of answer and it is already taken.** It is recorded
here so that nobody re-opens this issue believing the 1429 class is unguarded.

## The residual question — defects that are NOT in a checkable class

Rule 4 covers one cmake idiom. What `check-template-copy-out` catches is the
category "a template a user copies does not build", and the members of that
category found so far were each unreachable by any static predicate:

* an unresolvable `<depend>nano-ros</depend>` in two `package.xml` files — a
  name that is not even a legal ROS package name, over which two static gates
  and a colcon-parity job had been green for the template's whole life (issue
  1108);
* a Rust manifest path-depping five levels up into this checkout, so the
  template built in place and nowhere else (same first run);
* the sibling lane's first real finding: `check-scaffold-builds` caught the Rust
  component template still built on the `Component*` trait family retired in
  212.N.12, **broken for three and a half months** (#1412).

Those are what an end-to-end build finds and a source-reading gate does not.
Nothing in the tree currently asks that question on any event that gates a
merge, so the exposure window for the next one is a nightly's latency at best
and "until somebody reads the nightly" in practice.

## Why it cannot simply be made merge-gating

Two constraints, and they are not the same constraint.

**1. Cost.** The gate really builds: it copies each template out of the TRACKED
file set into a temp dir and runs `nros sync` + `nros build --workspace` on it,
six times. It is costed two ways in the tree and they disagree, which is itself
worth knowing: `just/check/fixtures.just` records **~10 min per template**,
while the run behind this filing put the whole set nearer **~20 min warm**.
Either way the unit is tens of minutes, and that is before provisioning — a C or
C++ workspace template needs the in-tree `nros` CLI built, the launch resolver
built, and the vendored `-sys` sources present.

**2. Lane contracts.** `check-lane-contracts` enforces the rule that came out of
the 2026-08-28 incident: **a gate in an affordability tier may only resolve
artifacts the JOB ITSELF builds.** `check-build` was on the merge group once and
could never pass there, because it resolved generated bindings and prebuilt
`.compile-ok` stamps that no CI job built — which left the required check red
for EVERY pull request for a day. A required check that cannot produce a pass is
a deadlock, not a slow gate.

Here the two constraints meet in a way worth stating precisely, because the
tree's two notes about it read as contradictory and are not:

* `just/check/fixtures.just` says the gate "resolves nothing it did not build
  itself, so it satisfies `check-lane-contracts` on any lane that can afford
  it";
* PR #1151's own note says it "cannot join an affordability tier".

Both are true. The gate is lane-contract-CLEAN *provided the job pays for the
CLI, the resolver and the submodule sources*, and that payment is exactly what
an affordability tier is defined as not doing. So the blocker is not a rule
forbidding the placement — it is that the placement is only legal at a price,
and nobody has decided whether the price is worth paying.

## The candidate that was suggested and not implemented

A **`merge_group`-only SUBSET: one template, `pure-c-workspace`** — not all six.

The case for it: the three reds were ONE defect with one cause, and
`pure-c-workspace` exhibits it. A single C workspace template is the cheapest
carrier of the "a copy-out template does not build" question, so one sixth of
the cost buys most of the class. The mechanism already exists and costs nothing
to build — `scripts/check-template-copy-out.sh` takes template names as
positional arguments (`check-template-copy-out.sh [--list | --self-test]
[template-name ...]`), so a subset lane is an argument, not a new code path.

The case against, stated so it is not lost:

* A subset is a **reach narrower than the rule** (issue 0196), and this
  neighbourhood has been bitten by that repeatedly. Whichever five templates are
  left off the merge lane keep exactly today's exposure, and the next defect has
  five places to land where the nightly is still the only reader.
* It still costs the provisioning, which is most of the fixed overhead. One
  template does not cost one sixth of six templates — the CLI build, the
  resolver build and the submodule checkout are paid once either way.
* A partially-covered class invites the reading that the class is covered. The
  honest alternative is "not merge-gating, and we know it" rather than
  "merge-gating for one of six".

Other roads deliberately not costed here, so the decision has a shape rather
than a single option: run the full set on `merge_group` and accept the batch
latency (the queue's whole economic argument is that expensive verification runs
once per batch rather than once per push, and the measured L1 figure in
`.github/workflows/gate.yml` — 587 s of an 878 s gate — is precedent for
tolerating minutes there); or keep it nightly and make the nightly's verdict
*reach* someone, which is what `just nightly-triage` exists for and is a
different repair to a different defect.

## This is a cost decision, and it is not taken here

No recommendation, on purpose. The inputs are: the exposure is a template a
stranger copies; the specific 1429 defect is already gated statically at no
cost; the unguarded remainder is the end-to-end class, whose three known members
were each invisible to static analysis; and the price of merge-gating any part
of it is tens of minutes plus provisioning per batch.

Whoever takes it should also decide the reporting half, because a nightly gate
that nobody reads and a merge gate that everyone waits on are the two ends of
the same trade, and there is a middle (a `schedule` lane whose red is surfaced
the way `just queue-triage` surfaces an ejection) that costs no build time at
all.
