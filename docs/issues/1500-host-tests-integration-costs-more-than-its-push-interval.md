---
id: 1500
title: "The `host-tests` integration job costs two to three times the interval
  between the pushes that trigger it, so with `cancel-in-progress: false` about
  half of its runs are cancelled before they start a single step"
status: open
type: bug
area: ci
severity: medium
found: 2026-09-25
related: [1492, 1353, 1158, 1040]
---

## What this is, and what it is not

Issue **1492** is about a job that WEDGES: one run holds the group for hours
and nothing else can start. This is the quieter thing underneath it, and it
would still be true if no run ever wedged again: **the job takes longer than
the gap between the pushes that trigger it.** The lane is not blocked by a
fault; it is oversubscribed by design.

It is **not** issue 1353 either. 1353 is why the job FAILS (it runs out of
disk). This is about the runs that never get to fail, and fixing the disk does
not change the arithmetic — a job that succeeds will take at least as long as
one that dies a fifth of the way through the tier.

1353 names this and declines it, correctly, as out of its scope: "the
concurrency behaviour is a separate and independent loss of signal on this
lane. It is not part of this issue, but a reader measuring *how often does
tier 1 answer on `main`* will hit it immediately and should not mistake it for
this one." Nothing has filed it since. This is that filing.

## The arithmetic, measured 2026-09-25

**What one job costs.** Step timings from job **107808557432** (run
36051691975), the run 1492 records as the first whose post-tier steps ran:

| step | minutes |
| --- | --- |
| `Initialize containers` | 1.3 |
| `Build nros CLI from packages/cli/` | 3.0 |
| `just setup native` | 2.8 |
| `Build rust core fixtures` | **14.2** |
| `Build workspace fixtures` | **60.1** |
| disk report + reclaim + upload | 0.7 |
| `just ci tier1` | 22.5 (then died on the disk) |

So the job reaches the tier at about **82 minutes**, 74 of them in the two
fixture builds, and had spent **105 minutes** when the tier failed one fifth of
the way in. Job **107977492420** agrees within a minute (13.7 + 61.4, tier
reached at ~82 m). Whole-job durations over 1492's eight wedges run **57 m to
2 h 30 m**.

**What the trigger costs.** The `push` trigger is path-filtered on
`packages/**`, `examples/**`, `Cargo.toml`, `Cargo.lock`, `justfile`,
`just/**`, `cmake/**`, `zephyr/cmake/**`, `nros-sdk-index.toml` and the
workflow itself — which is very nearly every commit that lands. The 30 most
recent runs span **2026-09-24T09:37 → 2026-09-25T09:03**, 23 h 26 m: one run
every **47 minutes** on average.

**What that produces.** Of those 30 runs, the `nros-tests integration (host)`
job:

* **started and ran steps in 16**, every one of which failed;
* **was cancelled with ZERO steps recorded in 14** — 47 %.

A run cancelled with zero steps never had a runner assigned. With
`cancel-in-progress: false` a group keeps one pending run, so a third push
displaces the second while the first is still going; that is the reading, and
the measurement is the fourteen zero-step cancellations. 1492 saw the same
shape from the other side ("the 21:24, 21:34 and 21:39 runs show their unit
halves `cancelled` by superseding while their integration halves never ran at
all").

## Why `cancel-in-progress: true` is not the fix

It is the obvious move and it is worse. Superseding is right for the **unit**
job — that job takes 8 to 11 minutes, so a newer push's answer arrives before
the older one would have finished, and the workflow's own comment gives exactly
that reasoning. Apply it here and every run is cancelled by the next push
before its 82 minutes are up: on a 47-minute push interval, **nothing ever
completes**. `cancel-in-progress: false` is what makes the lane answer about
*some* commits rather than none, which is why the workflow chose it.

Nor is a `timeout-minutes` this. That bounds a wedge (1492 remedy 1, landed on
this issue's PR) and does not change how long healthy work takes.

## What would close it

The shapes worth pricing, none of which this issue picks:

1. **Make the job cheaper.** 74 of the 82 pre-tier minutes are the two fixture
   builds. Whether that is reducible is a `fixtures.toml` coverage question —
   the same one 1353 reaches from the disk side, where those builds are also
   the 42 G that is gone before the tier starts. One change would answer both.
2. **Trigger it less often.** The path filter as written fires on nearly every
   commit. A narrower filter, or dropping `push` for `schedule` plus dispatch,
   trades "bound a regression to one commit" — which the workflow's comment
   gives as the reason for the `push` trigger — for a lane that reliably
   answers about `main` once a day. That premise deserves re-reading against
   the measurement: the lane currently bounds a regression to one commit for
   at most half of them, and in practice to none, since **it has not produced a
   green run since 2026-06-17**.
3. **Split the job.** The fixture build and the tier are one job because the
   tier consumes the fixtures; an artifact between them would let the build be
   shared across pushes instead of repeated per push.

Acceptance: over a representative window, the fraction of `host-tests` runs
whose integration job records zero steps is small, and the lane answers about a
named majority of the commits that trigger it. Today those numbers are 47 % and
none.

## Read this with

* **1492** — the wedge, and the `timeout-minutes` ceiling that bounds it.
* **1353** — why the job fails when it does run, and the 42 G that the same
  two fixture builds spend.
* **1158** / **1040** — the standing rule this is an instance of: a lane that
  produces no verdict is not a slow lane, it is a lane with no signal capacity.

## 2026-09-29 — the job now fills a 146 GB disk, because it gets further

`host-tests` run **36516625906** failed in `just ci tier1` at
`check-template-copy-out`'s `multi-package-workspace` copy, with a build log
cut off mid-word (`…NEWSLerror: recipe`). The cause is not in that log; it is in
the disk report the job prints after the step:

```
/dev/root  146G  146G  280K 100% /__w
42G  examples            (37G examples/workspaces, 5.3G examples/templates)
19G  build
16G  target              (13G target/debug)
15G  packages            (15G packages/cli/target)
```

This is progress, not a regression. Until issue 1468's fix, this job died
before building any fixtures, so it never produced this much output; it is
the next fault behind that one. The space is the lane's OWN build output, so
`scripts/ci/reclaim-disk.sh` — which frees what nothing downstream reads —
cannot recover it, and should not be made to.

Not fixed here, deliberately: which output this lane may discard, and when,
is a capacity decision rather than a bug. The largest single term is
`examples/workspaces` at 37 GB of fixture builds, and `target/debug` at 13 GB
is where a debuginfo setting would move the most. Both want a measurement of
which lane stages still read them before anything is deleted.

## The 150-minute ceiling fired for the first time (2026-09-29)

Issue **1492** set `timeout-minutes: 150` on this job and closed with the
ceiling listed as untested: *"`timeout-minutes: 150` has never fired — 4, 30
and 10 minutes of slack"*. It has now fired.

Run **36604239335**, job **109529052988**:

| | |
| --- | --- |
| job started | 17:21:42Z |
| `just ci tier1` (step 15) | 18:46:48Z → **19:51:41Z**, `failure` |
| post-steps 16–20 (disk report, transcript, junit, tier logs) | 19:51:41Z → 19:52:04Z, all `success` |
| job completed | **19:52:13Z**, conclusion **`cancelled`** |

2 h 30 m 31 s against a 2 h 30 m ceiling. Note also that the job waited 34
minutes for its concurrency group before starting — this issue's own subject —
so the run row spans 17:20 to 19:52 while the job itself is the 150 minutes.

## What it did and did not cost

**The verdict survived.** The tier failed on its own a minute before the
ceiling, with the usual shape:

```
Free space left: 35 MB
===== FAIL (workspace-features, rc=101, 400510ms) =====
```

which is issue **1353**, not a timeout. Every artifact step ran: the disk
report, the transcript, the nextest JUnit and the fixture/tier logs all
completed by 19:52:04. Nothing was truncated.

**What it cost is the LABEL.** Because the ceiling fired during the final
cleanup, the job's conclusion is `cancelled` and so is the run's — for a run
that actually failed, at a named gate, with the evidence uploaded. Anyone
reading run- or job-level status sees a cancellation and would reasonably skip
it as superseded; only the step list says `just ci tier1 => failure`. That is
the same class of loss this issue and 1492 are about, one level up: a real red
that does not read as one.

## What this changes

Not the remedy, and not a reopening of 1492 — its fix worked as designed and
the number it chose was defensible from what it could measure. What changed is
that the lane's duration has grown into its budget: 1492 priced 150 minutes
against runs that ended 4 to 30 minutes early, and this one needed every
second of it and then some. Any future move of that number should start from
this run rather than from the three in 1492, and should note that a ceiling
which fires *after* the verdict mislabels the outcome rather than protecting
anything.

## It fired again, and this time it cost the verdict (2026-09-30)

The section above measured the ceiling's first firing and concluded that what it
cost was the LABEL — the tier had already failed on its own a minute earlier,
every artifact step ran, and only the job's `conclusion` was wrong. The second
firing is not that.

Run **36663968607** (`host-tests`, **schedule**, 03:20), job **109724560522**:

| | |
| --- | --- |
| job started | 03:20:46Z |
| job completed | 05:51:41Z |
| duration | **2 h 30 m 55 s** against a 2 h 30 m ceiling |
| step 15 `just ci tier1` | **`cancelled`** |
| job conclusion | `cancelled` |

The difference from 09-29 is the step's conclusion. There it was `failure` —
the tier reached a verdict (`workspace-features`, `Free space left: 35 MB`) and
the ceiling merely arrived during cleanup. Here step 15 is **`cancelled`**: the
ceiling killed the tier mid-run. There is no `===== FAIL` line, no gate named,
and the post-steps that upload the disk transcript, the nextest JUnit and the
tier logs never ran, so nothing about what the tier was doing survives.

## What that means for this issue

Two firings, two outcomes, and the second is the one that matters:

| firing | step 15 | verdict | artifacts |
| --- | --- | --- | --- |
| 09-29 17:21 (push) | `failure` | 1353, named | all uploaded |
| **09-30 03:20 (schedule)** | **`cancelled`** | **none** | **none** |

So the lane can now consume its entire budget without producing anything —
which is a stronger statement than this issue's original one. The original
complaint was that the job costs more than the interval between the pushes that
trigger it, so about half its runs are cancelled before starting. This run was
not cancelled before starting: it ran for two and a half hours and was killed
before it could say anything.

**Not a reopening of 1492, and not an argument for a bigger number by itself.**
1492's own correction is the relevant caution: a ceiling priced from the
failures rather than from a healthy run destroys the verdict it was meant to
bound, and that is precisely what happened here. What this adds is that the
distribution has moved far enough that 150 minutes is now inside it, not above
it — the 09-29 run needed 2 h 30 m 31 s and this one wanted more than
2 h 30 m 55 s. Any move of that number should start from these two runs.

**Also worth separating from 1353.** Nothing here says the disk was the
problem. The tier was killed before it could report, so its cause is unknown,
and reading this as another 1353 instance would be assuming exactly what the
missing artifacts would have told us.

## Re-measured 2026-10-01 — the rate got WORSE while the interval nearly doubled

Asked of this issue: does the `target-check-cpp-*` residue still appear, and
does this lane's arithmetic still hold? The second half, measured over the 30
most recent runs (**2026-09-29T08:50 → 2026-10-01T03:23**, 1 d 18 h 33 m):

| | 2026-09-25 (as filed) | 2026-10-01 |
| --- | --- | --- |
| push interval | one run per **47 min** | one run per **88 min** |
| integration job: zero-step cancellations | 14 of 30 — **47 %** | **19 of 30 — 63 %** |
| integration job: actually ran steps | 16 | 11 |
| green runs | none since 2026-06-17 | **0 in the last 200 runs** (oldest 2026-09-21) |

**The interval nearly doubled and the loss got worse, which the original model
alone does not predict.** It is not a refutation of the arithmetic: the job grew
at about the same rate the interval did. This issue's own later sections measure
it reaching **2 h 30 m** and hitting the 150-minute ceiling twice, against the
~82 minutes priced on 2026-09-25. The RATIO is roughly unchanged (82/47 ≈ 1.7;
150/88 ≈ 1.7), so the lane is losing the same fraction for a different reason
than it was — the denominator improved and the numerator ate it.

Two cautions on reading the table:

* **"cancelled" at RUN level is not this metric.** 21 of the 30 runs conclude
  `cancelled`, but two of those are the 150-minute ceiling firing on a job that
  ran its full 23 steps — this issue documents both firings. The 63 % counts
  only integration jobs with **one or zero recorded steps**, i.e. the ones that
  never had a runner, which is what this issue is about.
* **The pushes are bursty, not evenly spaced.** The 09-29 cluster runs 10:35,
  11:27, 11:43, 11:51, 12:02, 12:28, 12:38, 12:46 — eight in two hours against
  an 88-minute mean. An average interval flatters a group that only ever holds
  one pending run.

## Still not picking a remedy, and why

The three shapes priced above are unchanged and the choice between them is a
**capacity decision**, not a defect to fix: making the job cheaper means
deciding which fixture coverage to drop, triggering it less often means trading
"bound a regression to one commit" for "answer about `main` once a day", and
splitting it means an artifact boundary. Each is somebody's call about what this
lane is for.

What the re-measurement adds to that choice: **option 2 (trigger less often) is
now weaker than it looked**, because the thing it buys — per-commit attribution —
is already not being delivered for 63 % of commits and has delivered a green
verdict for none in at least 200 runs. And **option 1 (make the job cheaper) is
now the only one that also moves the ceiling problem**, since a job that no
longer takes 2 h 30 m cannot be killed by a 150-minute ceiling mid-tier.

## 2026-10-01 — the disk half fixed; the cadence half is a decision

**Disk.** Run **36810256910** (2026-10-01) shows the job 90 % full BEFORE the
tier — 130 G of 146 G — of which the checkout is ~58 G (`examples` 43 G,
`build` 14 G); the reclaim then returns ~33 G (`/__t` 5.2 G, android 11 G,
dotnet 5.8 G, ghcup 3.7 G, swift 3.5 G, powershell 1.4 G, metadata-probe
2.2 G), and the tier still ran out. Its two largest terms are dev-profile
builds with cargo's default full debuginfo: root `target/debug` (12 G) and
the CLI's test build in `packages/cli/target` (15 G).

Measured on the CLI test graph (`cargo test --workspace --no-run`, two
scratch target dirs, nothing else changed): **13 G with full debuginfo,
4.8 G with `CARGO_PROFILE_DEV_DEBUG=line-tables-only` — 63 % less**, nearly
all of it `debug/deps`. `cargo test` inherited the dev setting, so one
variable covers both profiles. Projected across both dirs, ~17 G off the
tier. Set at JOB level on `integration` — never per step, which is the rule
the two removed `RUSTFLAGS: "-C debuginfo=0"` comments state, since a profile
setting is a fingerprint input. The fixture steps build `nros-relwithdebinfo`
and are untouched. `line-tables-only` rather than `0` keeps file:line in a
failing test's backtrace; nothing in the lane reads DWARF.

**One lever ruled out by measurement.** The 60-minute workspace build takes
no lane argument, and `workspace-fixtures-build.sh` honours
`NROS_FIXTURE_COORDS` — so narrowing it looked like the "make it cheaper"
shape this issue lists first. It is not: `lane-coords tier1` is all twelve
Linux language×RMW coordinates, and **all 73 Linux workspace rows fall inside
it**. The build is genuinely tier 1's. (`fixture-lane.sh`'s header still says
tier 1 maps to the broad `native` build; its own `nros_lane_build_lane` maps
`tier1` to itself, as CLAUDE.md records.)

**What remains is the cadence half, and it is a policy choice**, which is why
this issue declined to pick one. Read it with the two ceiling sections above:
the job is no longer ~2 h but **at its 150-minute ceiling** — 2 h 30 m 31 s on
09-29, and on 09-30 the ceiling killed the tier with nothing uploaded. So two
separate things now cost this lane its verdicts: runs superseded before they
get a runner (the original subject), and runs that do get one and spend all
of it. This change addresses neither directly. It may help the second as a
side effect — less debuginfo is less to write and link — but that was not
measured, and the disk margin it buys is the only claim made here. A trigger
change or a cheaper job is still what moves the cadence, and the cheaper job
is now known not to be a lane-narrowing.

## 2026-10-02 — it is not only `host-tests`, and BOTH lanes went a full day with zero green

This issue has been written about `host-tests` throughout. The same arithmetic
now applies to `gate`, and the two together have a consequence neither of their
own issues states: **the push lane produced no successful verdict at all on
2026-10-01/02.**

Every completed push-event run on `main` in the window, by lane and conclusion
(300 runs enumerated with the `created=` range form):

| lane | success | failure | cancelled |
| --- | --- | --- | --- |
| `gate` | **0** | 7 | 9 |
| `host-tests` | **0** | 5 | 11 |
| `docs` | 8 | 1 | 0 |
| `post-submit` | 26 | 3 | 2 |

Sixteen completed `gate` runs and sixteen completed `host-tests` runs, and not
one green between them. The two columns have different causes and neither is
new on its own — `gate`'s seven failures are all issue **1345** confirmed by
text, and `host-tests`'s five are **1353**, **1147** and the Rust 1.99 event —
but the **cancelled** column is this issue, and it is the larger half in both
lanes.

The interval is the reason, and it got shorter rather than longer: the last
three pushes of the window landed at `01:42:19`, `01:48:39` and `01:49:42`,
i.e. **63 seconds apart** at the tightest. `host-tests` integration runs over
an hour.

### Why the zero matters more than either number

A lane with a failing cause still has signal capacity in principle — fix the
cause and the greens return. A lane that is *also* being superseded faster than
it can finish has none even after the fix, and you cannot tell the two apart
from the conclusion column: a cancelled run and a run that never started look
identical, and both read as "not green". So the remedy question this issue has
been deferring is now load-bearing for 1345 and 1353 as well — when those are
fixed, `gate` and `host-tests` will still answer nothing on a one-minute merge
cadence.

`docs` and `post-submit` are the control, and they behave: both are short
enough to finish inside the interval, and both report.

### What this does not change

No remedy is picked here either, for the reason the earlier section gives. The
new fact is only that the cost is now measurable as a zero rather than as a
percentage, and that it is two lanes rather than one.

## The ceiling fired again, and this time the step breakdown says where the 150 minutes go (2026-10-02)

`host-tests` run **36999487841** (push, head `5439c00f7`), job **110813666069**
`nros-tests integration (host)`, started 13:19:18, cancelled 15:50:21:

```
The job has exceeded the maximum execution time of 2h30m0s
```

Per-step durations (every step over a minute):

| step | duration |
| --- | --- |
| 10 `Build rust core fixtures` | 13 min |
| 11 `Build workspace fixtures` | **60 min** |
| 15 `just ci tier1` | 69 min → cancelled |

**Half the window went on fixtures before a single test ran.** `just ci tier1`
started with 79 minutes left and used 69 of them without producing a `FAIL`, a
panic or a `gate(s) FAILED` line — so this red is the ceiling and nothing else.

Two things it is not, checked rather than assumed:

- **Not 1353.** The job's own annotation reads `93% used, 12G free — 43G
  examples; 20G build; 13G target`: it was not near the disk.
- **Not 1628.** No `FREERTOS_DIR … missing include` panic appears anywhere in the
  log; tier 1 was still running when the clock stopped it, so this run never got
  as far as that wall.

It also queued for 2 h 10 m before starting (run created 11:09:53, job started
13:19:18), which is the oversubscription half of this issue on the same run. The
cost of the ceiling, measured here: a run that waited over two hours, spent 2 h 31
m on a runner, and produced no verdict.
