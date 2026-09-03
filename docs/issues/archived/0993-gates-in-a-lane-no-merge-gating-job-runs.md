---
id: 993
title: "Forty gates sat in a lane no pull request runs, and the lane's own
  comment claimed otherwise"
status: resolved
type: bug
area: ci
severity: medium
found: 2026-09-02
related: [issue-0872, issue-0871, issue-0981, issue-0952, phase-395, phase-396]
---

## Symptom

Issue 0981's `codegen_golden` sat red on `main` for a day while the required
`CI` context stayed green, and two separate changes reached a green pull request
while failing `just ci gate` locally. Both were in `check::build`.

## Measured

`just check build` is invoked in exactly one place:

```
.github/workflows/gate.yml:678
  - name: just check build + no_std (nightly / manual only)
    if: ${{ contains(fromJSON('["schedule","workflow_dispatch"]'), github.event_name) ... }}
```

So **no pull request and no merge group runs any of its 40 gates.** A red there
is invisible until somebody reads a nightly.

And the lane's own comment said the opposite:

> Minutes + source/CLI prereqs; runs on PR + nightly (`default.yml` non-push),
> not on every direct push to main.

That is the shape this repo keeps paying for — a claim nobody checks is a claim
that is eventually false — and here it actively misdirected: anyone reading
`check.just` to decide where a new gate belongs was told the build tier gates a
pull request.

## What actually belongs there

Not a judgement call. `check-lane-contracts.py` already draws the line for
affordability tiers (compile-stage artifacts a job can produce, versus runtime
fixtures it cannot), and the same test works here: **run the gate in a pristine
worktree.**

```
$ git worktree add --detach <tmp> HEAD
$ cd <tmp> && just check <gate>
```

Of 21 candidates that looked pure, 20 pass with no build artifacts at all, in
47–1360 ms each. They moved to the fast line: measured 189 gates at -P32,
slowest still 11.6 s — the additions cost no measurable wall clock.

The 21st, `borrowed-e2e`, fails there:

```
borrowed-e2e: building nros-c (platform-posix)…
FAIL: nros-c config header missing at <tmp>/target/nros-c-generated/nros/nros_config_generated.h
```

It compiles `nros-c` and reads what that generates. That is a real reason to be
in the build tier, and the pristine run is what distinguishes it from "nobody
got round to moving it". The heuristic that preceded the test — grep the recipe
for `cargo`/`cmake` — called 28 gates pure and would have moved eight that
delegate their building to a script.

## Fix

* 20 gates moved to `fast`, so they gate a merge.
* The lane comment corrected, and it now says where the lane runs and why.
* `just check gate-visibility` → `scripts/check-gate-visibility.py`, fast line,
  static. The remaining ungated set is written down in
  `.config/ungated-gates.txt` and may only SHRINK — same ratchet as the
  selftest baseline. Each of the 19 entries carries the reason it fails a
  pristine run, grouped by kind (compiles a crate; needs the CLI; needs an RMW
  backend's sources; needs a linked image).

Adding a gate to a non-merge-gating lane now requires adding a line there,
which is the moment to ask whether it needs to be there.

### This gate's first version was vacuous

It reported `0 gate(s) run by no merge-gating job` — i.e. it thought everything
was gated. The guard it exists to read is

```
if: ${{ contains(fromJSON('["schedule","workflow_dispatch"]'), github.event_name) ... }}
```

where the event names are DOUBLE-quoted inside a single-quoted string, and the
event regex matched only `'schedule'`. Seeing no events, it concluded the step
was unguarded. Fixed to accept both quote styles, with that exact guard kept as
a self-test case.

## The pristine-worktree test is necessary and NOT sufficient

Nineteen of the twenty moved gates are green in CI. The twentieth,
`sched-dim-arms`, passed the pristine worktree and then failed CI's container:

```
== freertos core-pin arm (vTaskCoreAffinitySet) ==
  FAIL: the accept arm does not compile — this IS the call site.
      .../freertos_run_tiers.c:20:10: fatal error: FreeRTOS.h: No such file or directory
```

It compiles a probe against the FreeRTOS submodule sources. A pristine WORKTREE
has no build artifacts, but it still runs on a HOST with the submodules checked
out, the SDK store populated and `activate.sh` on PATH — so the test proves
"needs nothing BUILT" and says nothing about "needs nothing INSTALLED". CI's
bare container is the oracle for the second question, and it is the one that
matters for a gate that must run there.

`sched-dim-arms` is back in the build tier with that reason recorded in the
baseline. The remaining nineteen are confirmed by a green CI run rather than by
my host.

## The rest: tried, measured, reverted

The first pass left 20 gates ungated and called closing that a latency decision.
Measuring it changed the answer.

`gate.yml`'s `check` job runs in `ghcr.io/newslabntu/nano-ros-ci:humble` — the
SAME ROS container on a pull request and on a nightly — and `ci-ok` (`CI`, the
one required context) already has `check` in its `needs`. The build tier's two
prerequisites, `generate-bindings` and the compile-check fixtures, are steps in
that same job, merely guarded `schedule`/`workflow_dispatch`. So gating the
lane is widening three `if:` guards inside an existing job. Nothing is added to
the required set, which is the constraint that froze this repo four times.

The lane also gained a parallel runner, which it should have had anyway:
`build` was a plain serial dependency list while `fast` fanned out.
`NROS_GATE_LANE` picks which dependency line to run, the list moved to
`build-serial` exactly as `fast`'s did, and the runner's summary label is
derived from the lane instead of the literal "check-fast". **Those parts stay.**

**The gating itself was reverted, on the measurement.** Locally the parallel
lane was 190 s wall — on a 32-core box. CI fans out at **-P4**, where the same
lane took ~37 minutes (16:27 → 17:05, run 33654481082), `source-gates` alone
496 s. The `check` job went from 18 minutes to 42. That is ~25 minutes added to
every pull request, not the ~3 the local number implied.

**A 32-core dev box says nothing useful about a four-core runner**, and the
estimate that justified the change came from one. The number that decides a CI
cost has to be measured at CI's parallelism.

The attempt also surfaced three gates that FAIL in the container —
`borrowed-e2e`, `sched-dim-arms`, `source-gates` — and the nightly had been red
on `borrowed-e2e` since at least 2026-09-01 with nobody noticing. That is this
issue's own thesis arriving on schedule, and it is filed as issue 0995. A lane
that is already red cannot start gating pull requests regardless of cost.

So `.config/ungated-gates.txt` keeps its 20 entries, and the ratchet keeps doing
the one thing that is unambiguously right: making the invisible set visible.

## Prior art I should have found first

Issue **0872** already states this issue's central claim, from 2026-08-28:

> an arm nothing executes accumulates gaps at roughly one per gate

It reached it from the opposite end — PR #7's required check failing seven times,
each on a different gate, each further into the job — where this one reached it
by asking which lanes a merge-gating job runs at all. Two convergent diagnoses
of one property, which is itself evidence for the property.

I filed this without checking for an existing issue, which CLAUDE.md says to do.
What is genuinely new here is the RATCHET (`.config/ungated-gates.txt` plus
`check-gate-visibility`) and the 20 gates moved after testing each in a pristine
worktree; the diagnosis is 0872's.

## Acceptance

* [x] Every gate that can run without build artifacts gates a merge.
* [x] The lane's documented trigger matches the workflows.
* [x] The ungated set is explicit, reasoned, and may only shrink.
* [x] The build lane runs in parallel rather than serially.
* [ ] Every gate runs on a pull request — attempted, measured at ~25 min added
      per PR at CI's -P4, and reverted. Blocked on issue 0995 (three gates red)
      and on `source-gates`' 496 s.
