---
id: 1158
title: "The tier-2 lane has produced no runtime verdict in six consecutive scheduled runs, and no pre-merge event can produce one"
status: open
type: bug
area: ci, testing
severity: high
related: [0968, 1016, 1029, 1043, 1075, 1098, 1104, 1114, 1127, 1353, 1457, 1458, 1476, 1477, 1492, 1497, 1501, RFC-0061, phase-416]
found: 2026-09-06
---

# The lane whose job is the runtime verdict has not delivered one since 2026-09-01

## Measured

`run-matrix.yml` is the **only** automated path to tier 2's runtime cells.
Its last eight runs:

| when | event | outcome | died at |
| --- | --- | --- | --- |
| 09-06 06:25 | schedule | `pending` | never started |
| 09-06 02:44 | dispatch | `queued` | never started (5 h+) |
| 09-06 00:11 | dispatch | failure | `just build tier2` |
| 09-05 06:24 | schedule | failure | `just setup tier2` |
| 09-04 06:28 | schedule | failure | `just setup tier2` |
| 09-03 06:27 | schedule | failure | `Verify this runner's labels are true` |
| 09-02 06:33 | schedule | failure | `Verify this runner's labels are true` |
| 09-01 06:29 | schedule | failure | `Verify this runner's labels are true` |

**Six consecutive failures, four distinct causes, and not one of them is a test
result.** Every failure is upstream of the cells: runner labels, provisioning,
the build. Two runs are now stacked unstarted.

## Why no other lane covers it

| lane | event | depth |
| --- | --- | --- |
| `run-matrix.yml` | `schedule 0 6 * * *`, dispatch | `just ci matrix` — build **+ tests** |
| `queue.yml` | `merge_group` | `just ci matrix build` — **no tests** |
| `build-wide.yml` | dispatch only | `just ci matrix build` — **no tests** |
| `nightly.yml` | schedule, dispatch | `ci-matrix-nightly` (a different, wider lane) |

**No `pull_request` event runs any matrix lane**, and the merge queue runs only
the BUILD depth. So a runtime regression cannot be caught before merge by
design, and the one lane that would catch it after merge has been failing for
six days.

`post-submit.yml` and `gate.yml` mention `ci-matrix` in COMMENTS only — neither
invokes it. A grep that counts hits says otherwise; read the `run:` lines.

## The structural half — it is not just bad luck

One self-hosted runner exists (`newslab-175-26-server-nros-qemu-nros-sdk`), and
`run-matrix.yml` declares `concurrency: group: run-matrix,
cancel-in-progress: false`. The merge queue's `gate` jobs run on the SAME
runner. Observed while writing this: `busy=true`, occupied by `2x gate`, with
both matrix runs queued behind it.

So the lane is starved by the very PRs it exists to validate: every merge
lengthens the wait, and a busy day can starve it indefinitely. `cancel-in-progress:
false` then stacks the scheduled runs rather than superseding them, which is
correct for a verdict lane and makes the backlog visible rather than hidden.

## Why this matters more than one red lane

CLAUDE.md states the rule this violates:

> A red CI lane answers one of two questions and they look identical — the lane
> RAN and the code is broken (a verdict), or it never ran (no verdict). A
> uniformly-red lane has NO signal capacity.

Issue 0968 recorded ~12 tier-2 runtime failures as unreproduced and named the
cause: `post-submit`'s tier-2 job has never run (interlocked on
`vars.NROS_SELF_HOSTED_READY`, unset) and `host-tests` was red for 20
consecutive runs. That diagnosis is now six days older and unchanged.

**Four real regressions rode in behind this during 2026-09-05/06 alone** — 1075
(a link failure), 1098 (a compile poison with no migrated consumer), 1104 (a red
unit test), 1114 (a gate demanding an artifact no lane builds). Each was found
by hand, each hid the next, and each would have been a tier-2 verdict on any day
the lane had produced one.

## What would close this

Not "fix today's failure" — that has been done four times and produced no
verdict. The lane needs to reach the cells ONCE and then keep reaching them:

1. **A verdict, once.** Get one green-or-red RUN of `just ci matrix` on `main`,
   so 0968's ~12 failures become reproduced-or-retracted rather than folklore.
2. **Contention.** The verdict lane and the merge queue share one runner. Either
   give the lane its own runner/label, or schedule it where the queue is idle,
   or accept and DOCUMENT that it yields — the current state is neither chosen
   nor written down.
3. ~~**Distinguish "did not run" from "ran and failed"** at the summary level, the
   way issue 1043 did for `check-submodule-pins`. Six runs that all say
   "failure" hide four different stories; a lane that never reached its cells
   should not report the same word as one that did.~~ **DONE — see below.**

## Item 3, fixed (2026-09-07)

`scripts/ci/lane-stage.py` gives the lane the third outcome it was missing:

    VERDICT      the cells RAN. Green or red, the answer is about the CODE.
    NO VERDICT   the lane stopped before its cells, naming the stage
                 (provisioning / build). The answer is about the LANE.
    DID NOT START  the interlocked job never ran at all.

It reaches a reader in three places, in decreasing order of effort:

* **The `coverage` job's NAME.** It was `coverage report (did tier 2 run?)` — a
  question the job could not answer, because it read `needs.matrix.result`,
  which is `failure` for a run that tested the whole matrix and for a run that
  died in `runner-doctor` having built nothing. It is now
  `tier 2 — NO VERDICT: stopped in the build`. A check-run name is the last
  thing legible without opening a log (`gh run view`, the checks list), which is
  the level the issue asked for.
* **An annotation + a step summary** on the matrix job itself, saying which
  stage it reached and, when it reached no cells, that the run answers nothing
  about the code.
* **`just matrix-triage`**, the run-list-scale view — the sibling of
  `just nightly-triage` and `just queue-triage`, extended rather than replaced.
  It reads `gh run list`/`gh run view` and classifies runs that PREDATE the
  change, because a step's `conclusion` from `gh` and a step's `outcome` inside
  the workflow are the same four-word vocabulary and go through the same
  function.

**Measured, against the eight real runs above** (`just matrix-triage 8`,
2026-09-07): `0 of 8 run(s) reached the cells and produced a VERDICT`, split
`5 provisioning / 3 build` under one flat `failure`. Those eight runs are
recorded in the tool's self-test, so the classification is re-checkable offline
and the two counterfactuals (cells ran and failed / cells ran and passed) are
asserted to differ. Three mutations red it: adding `run` to the cells markers
(the `runner` substring trap `nightly-triage.py` records), treating a `skipped`
cells step as reached, and renaming a workflow step without updating the map.

Gate: `check-lane-stage-reporting` (fast line) — the step→stage map is AUTHORED,
so it drifts when a step is renamed, and it drifts in the safe-looking direction.

**What this deliberately does NOT do.** It cannot make a red green or a green
red — `lane-stage.py` exits 0 unconditionally. A reporter that can redden the
lane it describes is the defect one layer up. And it produces no verdict: it
makes the ABSENCE of one legible, which is the whole of item 3 and none of
item 1.

**Why the other three self-hosted lanes did not get this.** `build-wide.yml` and
`queue.yml`'s L3 have no cells at all — they are build-depth lanes, so "did it
reach the cells" has no referent there. `nightly.yml`'s `matrix-nightly` does
have cells and is already covered by `just nightly-triage`, which classifies its
per-cell reds by failing step. `run-matrix` was the one lane with cells and no
per-stage report.

## Still open, and why this issue is NOT resolved

The substance of this issue is the MISSING VERDICT, and item 3 does not deliver
one. It makes the absence legible; the lane still has not reached its cells.

1. **A verdict, once — OPEN.** Nothing here runs the cells. It needs the shared
   self-hosted runner and hours of fixture build, and until it happens issue
   0968's ~12 tier-2 runtime failures stay unreproduced. The one thing that has
   changed is that when the lane next fails, the run says whether that failure
   was one.
2. **Contention — OPEN, and deliberately not touched.** One runner serves both
   this lane and the merge queue's `gate` jobs, and choosing between "give the
   verdict lane its own runner", "schedule it where the queue is idle" and
   "accept that it yields" is an infrastructure decision with a cost the
   maintainer owns, not a defect an agent should decide by editing a label. No
   `runs-on`, no required-check set and no path filter was changed here.

Item 3's own limits, stated so they are not mistaken for coverage: the stage
model is `run-matrix`'s pipeline only; a cells step that fails having SKIPPED
every test still reads as a verdict (only `check-skip-budget` knows that
difference, the same known limitation `nightly-triage.py` records); and the
`coverage` job's dynamic name has not been observed on a real run, because
dispatching one is item 1's cost — its inputs are asserted in the gate and the
expression uses `needs`, which is an available context for `jobs.<job_id>.name`.

## Not covered

* Whether the runner-label failures (09-01..09-03) share a cause with the
  provisioning failures (09-04/05). Not investigated — they are simply
  different steps.
* `vars.NROS_SELF_HOSTED_READY`, still unset, which is what keeps
  `post-submit`'s tier-2 job from ever running (0968). Setting it is a decision
  about whether that runner is trusted for merge-gating, not a fix to make here.
* Issue 1127 (17 declared interop cells, no sweep runs any) is the same class one
  lane over and is filed separately.

## 2026-09-23 — seventeen days, and the two stops behind the count

`just matrix-triage` over the last eight `run-matrix` runs, 09-19 to 09-23:

```
  0 of 8 run(s) reached the cells and produced a VERDICT.
  8 of 8 run(s) produced NO VERDICT — the lane stopped before its cells.
```

every one `NO VERDICT: stopped in the build / at: just build tier2`. Item 3's
stage reporting works and is what made this readable: the answer arrives from
the run list without opening a log. **What it does not answer is the next
question** — which MODULE of the build, and what the first error was. The build
step fans out per module and the fixture runner already prints `== <module> ==
FAILED (rc=N)` plus a quoted first error, so the triage tool could carry one
more line. Recorded here rather than built; it is the difference between eight
identical rows and eight rows you can act on.

Behind those rows were two DIFFERENT stops, one per tier-2 variant, and neither
had anything to do with the cells:

* **1-wise (`run-matrix`)** — issue 1458, now RESOLVED. Four of five west
  fixtures aborted at Kconfig on `CONFIG_NROS` being undefined, because
  `scripts/build/west-fixtures.sh` never passed `-DZEPHYR_EXTRA_MODULES`
  after phase-449 W1 unbound the workspace's manifest project.
* **pairwise (`nightly`)** — issue 1457, host provisioning. The vendored
  rosidl clone's python deps are not installed on the self-hosted runner, so
  every cyclonedds leaf dies in `msg2idl.py` on `No module named 'catkin_pkg'`.

Both are upstream of a cell, so this issue's framing holds: seventeen days of
`failure` carrying no information about the code.

### A second fault behind the first, confirmed

Fixing 1458 does not by itself deliver a verdict. The 1-wise lane's west module
now gets past Kconfig; the pairwise lane still stops at 1457, which is not
fixable in this repository. Expect the next `run-matrix` to reach FURTHER into
`just build tier2` and quite possibly stop again somewhere new — that is what
seventeen dark days buys, and it is the 1025/1070 pattern this issue already
names. Read `matrix-triage` plus the module's first error each time; do not
read "still `NO VERDICT`" as "the fix did not work".

### The reporting half, still open

Both 1457 and 1458 were filed saying the cause was not in the job log. In both
cases it WAS — in the module's `log tail` block, not in the quoted "first error
line(s)", which matches `error:` and never the lines above it. The tail carries
the LAST leaf's output, so which of five failures you learn about is luck. A
runner that reports "there was an error" and not the error turns every instance
of this class into a manual reproduction; quoting the lines ABOVE the first
`error:` is worth doing regardless of cause.

### The third fault, 2026-09-25 — and it was foreseen above

The 2026-09-25 06:29Z `run-matrix` (run **36103083615**) is the first with both
phase-466 fixes in it, and both held: `check-fast (parallel): 354 gate(s) ran,
4 SKIPPED` with no gate failure (1476 gone), and `catkin_pkg` appears ZERO
times in the 1-wise job (1457 is the pairwise lane's, not this one). The lane
went FURTHER into `just build tier2` and stopped somewhere new, exactly as the
paragraph above predicted:

```
== zephyr == FAILED (rc=1)
CMake Error at .../zephyr/cmake/nros_system_generate.cmake:316 (message):
  nros codegen-system failed (rc=1):
  Error: codegen-system: .../zephyr_self_pkg/sibling/alpha_pkg/system.toml
  declares system semantics but no SystemModel was found.
west fixtures: 3/5 ok (0 reused, 3 built).
west-fixtures: 2 of 5 fixture(s) FAILED to build.
```

Filed as issue **1497**, root-caused and fixed as issue **1501**: the two
`zephyr_self_pkg` fixture leaves carry no `package.xml`, so the `nros sync`
this lane has run per bringup since issue 0533 could never scan them — sync
takes `src/<pkg>/package.xml` or a root `package.xml` and rejects a dir with
neither. phase-445 W5 gave these leaves a `system.toml` (which MAKES a model
mandatory) without the manifest that makes one obtainable, on 2026-09-11; the
1458 Kconfig stop hid it from that day to this one. Gate:
`check-self-pkg-package-xml`.

**Stage reached, measured, not inferred**: `just setup tier2` green, `just
build tier2` red at the zephyr module, 3 of 5 west fixtures built. Still NO
cell. Three faults have now been cleared in sequence, each revealed by the one
before it; the honest expectation for the next run is a fourth stop rather than
a verdict, and the two sibling reds in the same run (`host-tests` with no step
attributed, `probe` now running and failing on its own work) are further
candidates.

The reporting half above earns another mention here: the cause of THIS stop was
again in the module's `log tail`, and the quoted "first error line(s)" gave the
cmake frame without the `Error:` line under it that names the file.

## 2026-09-25 — the whole life of the lane: 31 runs, 0 verdicts

The sections above measure one run, or a window of eight.
`just matrix-triage 31` covers **every run `run-matrix.yml` has ever had** —
the workflow landed 2026-08-31 in phase-410 W3 (`5bc355411`), and
`gh run list --limit 100` returns 31 rows:

```
  0 of 31 run(s) reached the cells and produced a VERDICT.
  31 of 31 run(s) produced NO VERDICT — the lane stopped before its cells.
```

split **8 provisioning / 23 build / 0 cells**, every row `failure` or
`cancelled`. So "when was the last green run" has an answer nobody had asked
for: **there has never been one.** Item 1 is not overdue — it has never once
been met, and the tier-2 runtime coverage the rest of the repo reasons about
(1-wise over platform x language x rmw x kind, the RUN depth `queue.yml` and
`build-wide.yml` deliberately do not pay for) has never existed at all.

That reframes the three serial stops this issue has been tracking. They are not
what broke a working lane; they are the first three faults anyone could SEE,
because item 3's stage reporting is what made the stopping point legible:

| runs | stop | issue |
| --- | --- | --- |
| 09-01 .. 09-05 (5) | runner labels, then provisioning | — |
| 09-14 .. 09-16 (3) | cancelled in provisioning | — |
| 09-06 .. 09-23 (23 build, of which 09-17..09-23) | `== zephyr == FAILED`, `1/5 ok` | **1458**, resolved |
| 09-24 | `check fast` inside `just build tier2` | **1476**, resolved |
| 09-25 | `2 of 5 fixture(s) FAILED`, `no SystemModel was found` | **1497** / **1501** |

### Three negative results, so nobody re-measures them

Recorded because each is a plausible-sounding candidate that the run's own
output rules out:

* **NOT issue 1492** (a job that wedges and holds its concurrency group). That
  is `host-tests.yml`, whose jobs run on GitHub-hosted `ubuntu-22.04`.
  `run-matrix` runs on the self-hosted `nros-qemu, nros-sdk-zephyr, nros-big`
  set and **all 31 of its runs ran to completion** — the 09-25 one in 48
  minutes, through its own sweep and stage report. No wedge, no held group,
  different runner, different mechanism.
* **NOT issue 1353** (disk). The 09-25 run's own `runner-sweep.sh`:
  `fixtures: 1.3 GiB used / 60.0 GiB budget (high-water 1.3 GiB)`,
  `filesystem: 633G free on the checkout's volume`,
  `runner-sweep: done — nothing to sweep.`
* **NOT `provision-zenohd`.** `error: recipe 'provision-zenohd' failed with
  exit code 78` appears inside `just setup tier2` on nearly every run and reads
  like a provisioning failure. It is the lane-skip protocol (sysexits
  `EX_CONFIG`), and the caller that reads it says so three lines later —
  issue 1477. Provisioning is green on every run since 09-06.

### Item 3's gate: verified, not trusted

The step->stage map is AUTHORED, so it drifts in the safe-looking direction.
`python3 scripts/ci/lane-stage.py --selftest` — **83 passed, 0 failed**: the map
still matches `run-matrix.yml`'s real steps in both directions. And the 09-25
run's own in-job report classified correctly
(`[ok] provisioning / [FAIL] build / [----] cells`), which is the first time
that dynamic `coverage` job name has been observed on a real run — the one
thing the 2026-09-07 entry above listed as unobserved.

### The reporting half: the module logs are now an artifact

The section directly above, and issue 1497, both say the cause was in the module's `log tail` rather
than in the quoted "first error line(s)", which matches `error:` and so never
the lines above it. The real evidence is `<module>.log` in
`tmp/build-test-fixtures-<stamp>/`, and **nothing uploaded that directory** —
which is why 1457, 1458 and 1497 were each filed saying the cause was not in
the log when it was, one file over. `run-matrix.yml`'s artifact step now
carries `tmp/build-test-fixtures-*/…/*.log` plus the joblog, so the next stop
in this lane is diagnosable by download instead of by reproduction.

Still open from that paragraph: quoting the lines ABOVE the first `error:` in
the fixture runner's own summary. The uploaded log makes that less urgent, not
unnecessary.

### Items 1 and 2

**Item 1 (a verdict, once)** — open, and now known to have been open since the
lane's first run. **Item 2 (contention)** — untouched, for the reason already
given. No `runs-on`, required-check set or path filter was changed here.

## Item 2 has no ceiling, and issue 1492 already named the rule (2026-09-29)

Contention was written above as a scheduling question: which lane gets the
runner. It has a second half nobody had measured here — **how long one holder
may keep it** — and that half is not an open question, because issue **1492**
answered it for the other shared lane and its remedy was never swept to this
one.

1492's own words, in the comment it left on `host-tests.yml`: *"Without this
the next one has GitHub's six-hour default to run to, and the group is held for
all of it."* It set `timeout-minutes: 150` on the tier-1 integration job, with
the number derived from that job's measured durations. Neither self-hosted job
got one:

| workflow | job | `runs-on` | `timeout-minutes` |
| --- | --- | --- | --- |
| `host-tests.yml` | `nros-tests integration (host)` | `ubuntu-22.04` | **150** (issue 1492) |
| `run-matrix.yml` | `tier 2 (1-wise matrix)` | `[self-hosted, linux, nros-qemu, nros-sdk-zephyr, nros-big]` | **none → 360** |
| `nightly.yml` | `matrix-nightly` (`tier 2 nightly`) | the same four labels | **none → 360** |

That is the 0196 shape: the rule is *a job that holds a shared group needs a
ceiling this repository owns*, and the fix landed only where the symptom had
been seen. Today the unswept half is being exercised.

## What today measured

Run **36531385810** (schedule, 06:30), job **109285625689**, step 6
`just build tier2`: started 06:45:34Z, still `in_progress` at 11:22Z — **4 h
37 m in one step**, against every prior scheduled run of this workflow
finishing, end to end, in 25–80 minutes:

| run | outcome | duration |
| --- | --- | --- |
| 09-29 06:30 | in_progress | **4 h 52 m and counting** |
| 09-28 06:40 | failure (`just build tier2`) | 1 h 19 m |
| 09-27 06:27 | failure (`just build tier2`) | 1 h 09 m |
| 09-26 06:26 | failure | 46 m |
| 09-25 06:29 | failure | 48 m |
| 09-24 06:29 | failure | 45 m |
| 09-23 06:29 | failure | 32 m |
| 09-22 06:28 | failure | 24 m |

Downstream, exactly as item 2 predicts: nightly **36535897637** (07:18) has its
`tier 2 nightly (pairwise cover)` job `queued` since 07:18 — over four hours —
because it wants the same four labels. Its `bootstrap-probe` sibling completed
(and failed, issue 1439); only the self-hosted job is waiting. The 05:13
nightly's own tier-2 job held the runner from 05:35 to 06:38, so the sequence
is one holder after another with nothing between them.

**What is NOT claimed.** Whether this run is hung or merely slow is not
determinable from outside: per-job logs return nothing while the parent run is
in progress, and the runner is not on the host this triage runs from. It may
yet reach the cells, which would be item 1 finally satisfied. Both readings
lead to the same gap — a lane with no bound can hold the only self-hosted
runner for six hours, and until those six hours are up nothing distinguishes
that from progress.

**What would close this half.** A `timeout-minutes` on both self-hosted jobs.
Choosing the number is still the maintainer's call for the reason item 2 gives,
and 1492's correction is the warning: it priced the two terminal modes and not
the HEALTHY duration, *"the one a timeout can destroy"*, and had to be raised
to 150 for exactly that. The 25–80 m table above is a distribution of runs that
mostly died early, so it is a floor on the answer and not the answer — the same
position 1492 was in before its lane answered. Unlike tier 1, this lane also
has no green run to measure a complete one against, which is item 1.

## That run finished: five hours of real work, still no verdict (2026-09-29, later)

Run **36531385810** completed at 11:44:32Z — `failure`, **5 h 14 m** end to
end, with job 109285625689 spending **4 h 58 m 46 s** inside step 6
`just build tier2` (06:45:34 → 11:44:20). The `coverage` job named it
correctly: *tier 2 — NO VERDICT: stopped in the build.*

**It was not wedged.** The section above declined to say whether the run was
hung or merely slow, because nothing observable from outside could tell them
apart. Now it can: the step's log advances throughout, and the longest silences
are per-family builds, not a stall.

| longest gaps in step 6 | after |
| --- | --- |
| 55 min | `== zephyr ==` |
| 44 min | `== threadx_riscv64 ==` |
| 17 min | the first `build-fixture-extras` failure |
| 12 min | an `nros sync` |

So the answer is *slow, and progressing further than any previous day* — prior
runs died in the same step at 24–80 minutes. What ended it, at 11:38, was the
last fixture leg:

```
FAILED: [code=1] cyclonedds-ts/_idlroot/builtin_interfaces/msg/Duration.idl
error: rosidl_adapter is not importable by this build's interpreter.
make[1]: *** [.../fixture-make-driver/...mk:11: fixture-linux-c-cyclonedds] Error 1
error: recipe `build-fixture-extras` failed with exit code 2
```

That is issue **1457** — the same cause that ended the 05:13 nightly's tier-2
leg six hours earlier. Both of today's tier-2 attempts died of one defect, one
after 63 minutes and one after nearly five hours.

### What this buys item 2

A floor for the ceiling. The whole reason 1492's `timeout-minutes` had to be
raised to 150 was that its first number was priced from the failures rather
than from a healthy run, and this lane had no healthy run to price from. It
still has none — but it now has a run that reached the LAST fixture family
before failing, so **the build step alone costs at least 4 h 53 m** when it
gets that far. Any ceiling under about five hours would cut a run that was
still making progress, which is the mistake 1492 already made once. The
six-hour default this run consumed 87 % of is not that ceiling either; it is
just the absence of one.

### What this buys item 1

Nothing yet. Zero cells, again. What it does establish is that the remaining
distance to a verdict is one issue and not a mystery: fix 1457 and this lane
reaches `just ci matrix` for the first time since 2026-09-01.

## A third distinct stop in three nights (2026-09-30)

Nightly **36672407797** (schedule 05:13), job **109750040694**
(`tier 2 nightly (pairwise cover)`), step `just build tier2-nightly`. The
`== zephyr ==` module failed again, and again with a message the previous two
nights did not produce:

```
first error line(s) in tmp/build-test-fixtures-20260930-052445-451987/zephyr.log:
  126:  5:CMake Error: The source directory
        ".../examples/workspaces/rust/build/zephyr-zenoh/zephyr_entry" does not exist.
  127:  9:ninja: error: rebuilding 'build.ninja': subcommand failed
```

Four occurrences, all the same path. The lane's three consecutive stops, same
module, same job name:

| night | message |
| --- | --- |
| 09-29 05:13 | `rosidl_adapter is not importable by this build's interpreter` |
| 09-29 13:12 (same sha) | `2 <depend> name(s) resolve to nothing: rosidl_default_generators, rosidl_default_runtime` |
| 09-30 05:13 | `The source directory .../build/zephyr-zenoh/zephyr_entry does not exist` |

That is the point this issue keeps making, now with a third instance: the lane
reports `failure` every night and the word carries no information about what
failed.

## What the third one is, and is not

The path is under **`build/`**, not `src/` — `examples/workspaces/rust/src/zephyr_entry`
is the hand-written west application issue **1288** is about, and it exists. What
does not exist is the staged copy the build dir's `build.ninja` names, and ninja's
own line says it was **re-running cmake on an existing manifest**
(`rebuilding 'build.ninja'`). That is the shape `scripts/lib/ninja_stale_refs.py`
and `just reconfigure-stale` exist for (issue **1406**: a manifest that LOADS and
names a missing input), reached here on a runner whose workspace outlives the
checkout that configured it (the mechanism issue **1360** measured).

**Not** claimed: that 1406's scan would have caught this one, or that the
remedy is a reconfigure. Neither was tested here, and the uploaded
`zephyr.log` (1158 item 3's artifact) is where someone reproducing it should
start.

**Not** 1457 or the `<depend>` failure either — both messages are absent from
this job's log, which is the same counting check the 09-29 entry used.

## A fourth distinct stop (2026-09-30) — and the tier-2 nightly's is now the emitted west app

Nightly run **36672407797** (05:13), job **109750040694**, step 5
`just build tier2-nightly`. The lane reaches its fixture build and the zephyr
module fails on:

```
CMake Error: The source directory
".../examples/workspaces/rust/build/zephyr-zenoh/zephyr_entry" does not exist.
ninja: error: rebuilding 'build.ninja': subcommand failed
```

That directory is issue **1288**'s generated west application, which W5.b moved
out of the tracked tree and into `build/`. The evidence and the analysis are
filed there; recorded here because this lane is what triage reads, and because
it is the fourth different reason the build stage has stopped in four nights —
after `rosidl_adapter is not importable` (1457), the `zephyr_self_pkg`
SystemModel (1497/1501) and the run this issue's previous entry describes.

Also in the same log, and NOT a failure: `error: recipe provision-zenohd failed
with exit code 78`, which the caller immediately annotates as the lane-skip
protocol naming issue 1477. A grep for `error:` finds it first, above the real
stop — worth knowing for anyone triaging this lane by pattern.

The point this issue keeps making holds: four nights, one summary string, four
causes. Nothing here can be attributed without re-reading the error text.

## The lane RAN — and the build stopped on THREE causes, two of them new (2026-10-01)

Run **36825211926** (schedule, 06:31:16Z), job **110249254587**. After waiting 1 h
52 m the self-hosted runner claimed it at 08:23:14Z (issue 1365's window closing),
and for the first time in this episode the lane executed. It still produced no cell
verdict — its companion job is named `tier 2 — NO VERDICT: stopped in the build` —
but the stop is now legible, and it is not one stop:

| module | first error |
| --- | --- |
| `native` (rc=2) | `Error: 2 <depend> name(s) resolve to nothing: rosidl_default_generators, rosidl_default_runtime — declared by examples/workspaces/features/src/custom_msgs/package.xml` → **issue 1592** |
| `esp32` (rc=1) | `check-stack-floor: no RISC-V `nm` found … Refusing to report a verdict without one — a missing tool is not a pass` → **issue 1591** |
| `threadx_riscv64` (rc=1) | six × `rust-lld: error: undefined symbol: nros_rmw_cyclonedds_register_descriptor`, `6/12 ok` → **issue 1590** |

**Two of the three were not in the triage table this lane is read with**, and
neither is a cause this issue has recorded before. The `zephyr` module, which was
the stop on 2026-09-30 (the emitted west application, issue 1288), **did not fail
this time**.

### Two things about the evidence, for whoever reads this lane next

**The job log does not contain the decisive lines.** For `native` and `esp32` the
job log carries only `error: recipe … failed` plus a 120-line tail, and both tails
end before the error — `native`'s stops at `Locking 0 packages to latest Rust
1.98.1 compatible versions`. The real text is in the per-module logs
(`tmp/build-test-fixtures-<stamp>/{native,esp32}.log`), which the lane uploads as
the **`post-submit-junit`** artifact. A triage that reads only the job log will
conclude these modules failed without a reason.

**`error: recipe provision-zenohd failed with exit code 78` appears above all of
it and is NOT a failure** — it is the lane-skip protocol, which the caller
annotates in its own words naming issue 1477. A grep for `error:` finds it first,
as the 2026-09-30 entry also warned.

### What this does to this issue

Five distinct build-stage stops are now on record (1457, 1497/1501, 1288, and
today's 1591 + 1592), on a lane whose summary string has not changed. The claim
this issue keeps making is unaltered and this run is its strongest instance: the
lane finally ran, and what it found was three independent reasons, two previously
unknown. Fixing any one of them does not give this lane a verdict.
