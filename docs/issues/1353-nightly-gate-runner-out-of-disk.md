---
id: 1353
title: "The scheduled `gate` run dies at `just check build` with `No space left on
  device` on the GitHub-hosted runner — 8 of the last 10 nights, so the compile
  tier's only unattended lane has produced almost no verdict for a week"
status: open
type: bug
area: ci
severity: medium
found: 2026-09-12
related: [1350, 0996, 1177, 1158]
---

## What happens

`gate.yml` runs on `schedule` at 02:0x UTC, and that event is the only one that
reaches the compile tier: `just check build (nightly / manual only)` and
`just check no-std (nightly / manual only)` are both gated to
`schedule`/`workflow_dispatch`. On the hosted runner, the job reaches that step
and then the **runner process itself** runs out of disk:

```
System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20260912-020326-utc.log'
   at System.IO.RandomAccess.WriteAtOffset(SafeFileHandle handle, ...)
```

The exception is on the runner's own `_diag` log, not on anything the build
writes, so by the time it surfaces there is no room left even to record why.
The job's own log blob is unavailable afterwards (`BlobNotFound` from the log
API), which is consistent with the upload having nowhere to write.

## Evidence

| run | date | job | failing step |
| --- | --- | --- | --- |
| 34666582331 | 2026-09-12T02:03 | 103479596699 | 22 `just check build (nightly / manual only)` |
| 34301642480 | 2026-09-09T02:03 | 102309634092 | same step |
| 34178686534 | 2026-09-08T02:02 | 101913144010 | same step |

All three carry the identical `No space left on device` annotation on
`_diag/Worker_<date>-utc.log`. The scheduled `gate` history is:

```
34666582331 failure 09-12    34075012860 failure 09-07
34553102010 SUCCESS 09-11    34005439803 failure 09-06
34427875667 failure 09-10    33937933817 failure 09-05
34301642480 failure 09-09    33828029043 failure 09-04
34178686534 failure 09-08    33706473516 failure 09-03
```

**One success in ten nights** (2026-09-11).

## Why it matters

This is the "a red lane answers one of two questions and they look identical"
class the pitfall index already names: the lane has no signal capacity while it
is uniformly red, so a real compile-tier regression landing tonight is
indistinguishable from the disk failure. And the compile tier is exactly where
the `pre-push` `check-fast` hook deliberately does *not* look — `check build`
caught two reds on `main` in its first two runs for that reason. Since
2026-09-03 it has answered once.

It also weakens two claims made elsewhere. CLAUDE.md says `check-build` is
`schedule`/`workflow_dispatch` only *because* it cannot pass on the merge group;
that is a sound argument for where it runs, but it assumes the schedule
actually runs it. And `check-default-gates-run-somewhere` (issue 1040) asks
whether a gate is *reachable* from some lane, not whether that lane *completes*
— so a gate whose only lane dies before reaching it still counts as covered.

## What this is NOT

Issue **1350** is the same error string on a different machine. That one is the
local dev host: orphaned `-P 32` gate fan-out kept building after its lane was
killed and took `/home` to 0 bytes free twice. This issue is the **GitHub-hosted
runner**, where no orphan from this repo can exist across jobs — the runner is
fresh per job. The two share a symptom and nothing else, so fixing 1350 will not
fix this.

## It is not one lane — a second hosted lane hit it (2026-09-12)

The `live-peer regression` workflow, run **34672525328** (schedule, 04:15), job
**103496470349** (`rows whose board IS this runner`), carries the same
annotation on the same file:

```
Unhandled exception. System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20260912-041530-utc.log'
```

Its sibling job **103496661226** (`rows whose board is NOT this runner`) failed
at step 11, `Build the fixtures those rows resolve`, reporting only `Process
completed with exit code 1` with no compiler diagnostic in the log — the same
shape the `host-tests` job showed on 2026-09-12T02:27, where the stream is
truncated mid-compile and the run ends. That truncation is itself a symptom:
when the disk is gone the log has nowhere to land, so the lane that failed
cannot say why.

So the title's "the scheduled `gate` run" understates it. At least three hosted
lanes are affected — `gate` (schedule), `live-peer regression` (schedule) and
`host-tests` (push) — which rules out anything specific to `just check build`
and points at the job image's free space against what a full provision plus
compile-tier build now costs. The disk report the section below asks for should
therefore be added to the shared setup the lanes have in common, not only around
the compile-tier steps.

## The truncation is confirmed as disk, from cargo itself (2026-09-12)

Until now `host-tests` was attributed to this issue by the SHAPE of its
failure — a log truncated mid-compile with no diagnostic. Run **34679397895**
(push, 06:55), job **103515049240**, step `just ci tier1`, says it outright:

```
===== FAIL (test-targets, rc=101, 62936ms) =====
error: failed to write to `target/debug/deps/rmetaYBvbcx/full.rmeta`:
  No space left on device (os error 28)
error: could not compile `zerocopy` (lib) due to 1 previous error
error: failed to run custom build command for `zpico-sys v0.5.0`
```

So the inference was right, and this replaces it with a measurement. It also
narrows where the space goes: the failure is a write into the HOST workspace's
`target/debug`, during `check test-targets`, which is a workspace-wide plus
per-crate clippy — i.e. the tier-1 lane fills the same disk the compile tier
does, without touching the compile tier at all.

## Still live six days on, and the push lane's verdict rate is now measurable (2026-09-17/18)

Re-measured because a lane that is red every cycle has no signal capacity, so
"same as last time" is not a finding. It is the same cause, stated by cargo
again rather than inferred — run **35282943543** (push, 22:37), job
**105408801047**, step `just ci tier1`:

```
##[warning]You are running out of disk space. … Free space left: 84 MB
error: failed to write to `/__w/nano-ros/nano-ros/target/debug/deps/rmetaia4cmc/full.rmeta`:
  No space left on device (os error 28)
error: recipe `test-targets` failed with exit code 101
```

Same step and same write target as the 2026-09-12 measurement above, so nothing
about the cause has moved.

What is new is the **rate**, which the earlier sections give for the scheduled
`gate` lane but not for `host-tests` on push. Every `host-tests` run on `main`
between 19:42 and 00:25 UTC:

| run | created | outcome |
| --- | --- | --- |
| 35266432423 | 19:42 | failure — 1353 |
| 35273285355 | 20:51 | failure — 1353 (`Free space left: 99 MB`, log truncated mid-compile) |
| 35281018380 | 22:14 | cancelled by concurrency |
| 35281878661 | 22:24 | cancelled by concurrency |
| 35282943543 | 22:37 | failure — 1353 (quoted above) |
| 35286479179 | 23:21 | cancelled by concurrency |
| 35289417066 | 00:01 | cancelled by concurrency |
| 35291099866 | 00:25 | in progress |

**Three verdicts in five hours, all three this issue; four runs cancelled before
reaching one.** So the push lane is worse off than the "uniformly red" shape the
section above describes for the schedule: it is red *and* mostly pre-empted, and
because each run takes 60–100 minutes it is routinely superseded by the next
push before it can answer. A tier-1 regression landing in that window is
invisible twice over.

Two consequences worth carrying into the fix:

* The `du`/`df` reporting item below should cover `host-tests` too, not only
  `gate.yml`'s compile-tier steps — this lane fills the same disk without ever
  entering the compile tier, which the 2026-09-12 section already established
  and this confirms.
* Whatever the disk fix turns out to be, the concurrency behaviour is a separate
  and independent loss of signal on this lane. It is not part of this issue, but
  a reader measuring "how often does tier 1 answer on `main`" will hit it
  immediately and should not mistake it for this one.

## What would close it

Measurement first, because the cause is not yet established and a guessed fix
here is a guessed fix to the only unattended compile-tier lane:

1. Add a disk report to `gate.yml` around the compile-tier steps (`df -h` before
   `just check build` and after it, plus `du -sh` of the cargo target dir, the
   sccache dir and `third-party/`) so the next failure says how much was used and
   by what. Today the only artifact is an exception about a log file.
2. From that, decide between the two shapes: the tier genuinely needs more than
   a hosted runner's free space, or something is accumulating within the job
   (the compile-check fixtures, the message bindings and the provisioned
   compile-tier sources are all built in the same job, before this step).
3. If it is the former, the honest options are pruning what the tier builds,
   freeing space explicitly at the start of the job, or moving this tier to a
   runner that has the disk — not leaving a lane red and calling it known.
4. Acceptance is a scheduled run that **reaches a verdict** on `check build` and
   `check no-std` — green or red, but a verdict — on three consecutive nights.

Until then, treat a red scheduled `gate` as "no answer", not as "the compile
tier is broken", and read `just check fast` in that same job separately: it
passed in run 34666582331 (step 13), because the scheduled job provisions the
submodule sources that make `capability-conditionals` and
`xrce-vendored-versions` fail everywhere else.

## Step 1 is implemented (2026-09-22) — the report, not the remedy

`scripts/ci/disk-report.sh` prints `df -h` for the workspace, `/` and `/tmp`
(deduped to one row per filesystem), `du -sh` for the workspace `target/`, the
CLI's `packages/cli/target/`, `build/`, `third-party/`, `examples/`, the sccache
dir, `~/.nros` and `~/.cargo`, and the ten biggest children of `target/` —
which is where both quoted measurements above landed
(`target/debug/deps/…rmeta`). It is wired into:

* `gate.yml`, bracketing `just check build` on the schedule/dispatch events —
  the placement this section asks for;
* `host-tests.yml`, bracketing `just ci tier1` — the extension the 2026-09-17/18
  section argues for, since this lane fills the same disk without entering the
  compile tier.

Both "after" reports run under `always()`, not `!cancelled()`: the run that
FAILED is the one whose numbers matter, and the default `success()` would skip
exactly it. The script never fails a job — every probe tolerates its own
failure, because a report that can break the lane it reports on is worse than
no report.

**This changes nothing about the disk.** It exists so that step 2 — deciding
between "the tier needs more than a hosted runner has" and "something is
accumulating within the job" — is made from numbers instead of inference. The
acceptance in "What would close it" is unchanged: a scheduled run that reaches
a verdict on `check build` and `check no-std`, three nights running.

Rate, re-measured for this cycle: 2026-09-22 saw this cause on FIVE lanes —
scheduled `gate` (02:02, runner lost communication, 2-line log), `live-peer`
(04:16, `_diag/Worker_*.log` write), `host-tests` schedule (03:19) and
`host-tests` push twice (05:19 and 09:55, the latter quoting
`rustc-LLVM ERROR: IO failure on output stream: No space left on device`).

## The first measurement (2026-09-22) — it is BOTH shapes, and the split is 42 G / 22 G

Run **35726999134** (`host-tests`, push, `7c0791416`), job **106742973318** — the
first to carry the report added above. Both brackets fired, and the step
between them failed the same way as every other instance of this issue
(`FAIL (cpp, rc=101)`, `recipe tier1 failed`), so these are the numbers of a
real occurrence rather than a dry run.

**Before `just ci tier1`:**

```
/dev/root       146G  124G   22G  86% /__w
404M  packages/cli/target
 10G  build
 31M  third-party
 42G  examples
 50M  /github/home/.nros
1.2G  /usr/local/cargo
```

**After it:**

```
/dev/root       146G  146G  244K 100% /__w
4.1G  target            (debug 2.9G, nros-relwithdebinfo 938M, nros-c-param-services 329M)
 13G  packages/cli/target
 13G  build
 31M  third-party
 42G  examples
```

### What this settles

Step 2 above asks to decide between "the tier needs more than a hosted runner
has" and "something accumulates within the job". **It is both, and they are not
equal partners:**

* **The job arrives at the tier with 86 % of the disk already gone** — `examples/`
  alone is **42 G**, built by `Build rust core fixtures` and `Build workspace
  fixtures` in this same job, plus 10 G in `build/`. That is the accumulation
  half, and it is the larger number.
* **`just ci tier1` then wants more than the 22 G that is left.** It spent all
  of it: `packages/cli/target` **404 M → 13 G** (+12.6 G), a fresh workspace
  `target/` at **4.1 G**, `build/` +3 G. The warning
  `Free space left: 70 MB` lands seconds after the step starts, and the step
  runs another 24 minutes before `cpp` fails on the write.

So raising the runner size alone would buy roughly one more run's headroom,
and pruning the tier's own build alone would not reach the 42 G that is already
spent when it starts. The two candidate remedies in step 3 — prune what the
tier builds, free space explicitly at the start, or a bigger runner — can now be
priced against these numbers instead of guessed.

### One number the report does not yet give

`examples/` is a single 42 G total; which fixture families dominate it is not
broken down, because the report's per-child expansion only covers `target/`.
Whoever takes step 3 should add `du -sh examples/*` (or the top N) before
deciding what to prune — the same one-line change to `scripts/ci/disk-report.sh`.

Acceptance is unchanged: a scheduled run that reaches a VERDICT on `check build`
and `check no-std`, three nights running.

## The report does NOT survive this failure on the scheduled `gate` lane (2026-09-23)

First scheduled `gate` after the report landed: run **35808783184** (02:02),
job **107015560275**. It failed the usual way for this lane — annotation:

```
System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20260923-020342-utc.log'
```

and the job log is the 2-line `BlobNotFound`. The step list is the finding:

| step | state |
| --- | --- |
| 22 `Disk report (before check build)` | **success** |
| 23 `just check build` | **failure** |
| 24 `Disk report (after check build)` | **pending** — never ran |

So on this lane the measurement added above **produces numbers nobody can
read**, twice over:

* the BEFORE report ran and its output went into the job log, which the runner
  never uploaded — the same "when the disk is gone the log has nowhere to land"
  symptom this issue already describes, now applied to the diagnosis itself;
* the AFTER report is guarded by `always()`, and `always()` only binds while the
  runner is alive to honour it. This runner died inside `check build`, so the
  step stayed `pending` and the job was abandoned.

**What this does NOT change:** the `host-tests` numbers recorded above are
unaffected — that lane's job completes and uploads its log, which is why the
42 G / 22 G split is known at all. This is specifically about the scheduled
`gate`, where the failure destroys its own evidence.

**What a fix has to find:** a channel that survives a runner death. One is known
to work, because this issue is reading it right now — the ANNOTATION channel
carried the `IOException` out of a job whose log was lost. Whether a workflow
`::notice::` rides that same channel or the lost log stream is **not
established**, and guessing is how this issue got a report that cannot be read.
The cheap experiment: emit one `::notice::` from the before-report, let the next
scheduled `gate` fail, and see whether it appears in
`check-runs/<jid>/annotations`. If it does, the df summary belongs there; if it
does not, the numbers have to leave the runner another way (an artifact uploaded
before the compile tier, or a step that writes them where a later job can read).

## The 42 G is two directories, and one of them is 36 G (2026-09-23)

The breakdown `88e9f940a` added arrived on the first scheduled `host-tests`
after it — run **35813854577** (03:19), job **107031060491**, which failed the
usual way (`Free space left: 91 MB`, `FAIL (cpp, rc=101)`, `recipe tier1
failed`). Both brackets fired and the log survived, so these are a real
occurrence's numbers.

**`examples/`, before `just ci tier1` (identical after — the tier does not add
to it):**

```
 36G  examples/workspaces
5.2G  examples/templates
 20M  examples/native
8.3M  examples/rv-virt-threadx
8.1M  examples/threadx-linux
8.1M  examples/qemu-armv7a-nuttx
8.1M  examples/mps2-an385-freertos
8.0M  examples/zephyr
4.7M  examples/mps2-an385-baremetal
660K  examples/esp32-c3-baremetal
492K  examples/px4
336K  examples/rv-virt-nuttx
```

So the 42 G that is gone before the tier starts is **`workspaces` (36 G) plus
`templates` (5.2 G)** — 98 % of it — and every per-board example leaf together
is under 70 M. The rest of the run is unchanged from the 2026-09-22
measurement: 22 G free at the start, 256 K at the end, `packages/cli/target`
404 M → 13 G, a fresh `target/` at 4.1 G (2.9 G of it `target/debug`),
`build/` 10 G → 13 G.

**What step 3 can now price.** "Prune what the tier builds" means, concretely,
the four large workspace fixtures under `examples/workspaces` — they are the
disk, and the per-board leaves are rounding error. Whether they can be pruned
is a question about `fixtures.toml` coverage (`lane=native` builds them via
`Build workspace fixtures`), not about disk, and this issue does not answer it.

Still unchanged: acceptance is a scheduled run reaching a VERDICT on `check
build` and `check no-std` three nights running, and on the scheduled `gate`
lane the report itself is still unreadable (section above).

## Both `ci-base` lanes fail at the SAME gate, and three named suspects are not it (2026-09-23, phase-466 W4)

Read from fresh runs, because a lane red for eleven days cannot be attributed
from an old one.

| lane | run | job | failing step |
| --- | --- | --- | --- |
| `gate` (schedule) | 35808783184 | 107015560275 | 23 `just check build (nightly / manual only)` |
| `host-tests` (schedule) | 35813854577 | 107031060491 | `just ci tier1` |

Those look like two steps and are one. `just ci tier1` runs `just check`, which
runs the `build` tier, and the tier-1 log names the line:

```
===== FAIL (cpp, rc=101, 6880ms) =====
Checking C++ headers (build + syntax + clippy)...
  - freestanding syntax (c++14)
error: failed to run custom build command for `nros-cpp v0.5.0`
  process didn't exit successfully: … build-script-build (exit status: 101)
error: recipe `build` failed on line 233 with exit code 1
error: recipe `tier1` failed with exit code 1
```

`just/check.just` line 233 is the body of `build:`. So **`just check build` is
the single failing step on both lanes**, and `cpp` is not special — it is
wherever the `-P4` fan-out happened to be when the disk ran out, four lines
after the runner's own

```
##[warning]You are running out of disk space. … Free space left: 91 MB
```

The same `cargo build -p nros-c -p nros-cpp --no-default-features --features
std,rmw-cffi,platform-posix,ros-humble` finishes in 30.8 s on a developer host
with disk to spare, which is the control for "is this a `cpp` defect".

### Three suspects that are not the cause

* **`invalid instruction mnemonic 'bkpt'`.** Real, and in the same run — but in
  the `nros new -> sync -> resolve` job (107015560271), which **passed**
  (`scaffold-journey: PASS (baremetal)`). It is a non-fatal `sync:` line: the
  scaffolded baremetal project cannot be host-probed, so `nros sync` degrades to
  `1 component(s) are un-probeable, so pool budgets stay at the crate defaults
  (issue 1061)`. That is issue **1265**'s class — a cross-only leaf the metadata
  probe cannot build — with a third shape worth recording there: this one is not
  classified by `probe_blocker` at all, so instead of a named refusal it reaches
  the host assembler and fails on ARM inline asm. Nothing to do with these two
  lanes; it does not fail the job it appears in.
* **`recipe 'zenohd' failed`.** The only text recoverable from a `host-tests`
  run old enough (2026-08-27) to predate this issue's window. Unreachable today:
  the lane dies in `just check`, and `zenohd` is downstream of that in
  `test-all`. Issue 0774 is the right entry if it ever surfaces again, but it is
  not what is happening now.
* **The CI image.** Both lanes pull `ghcr.io/newslabntu/nano-ros-ci:humble`,
  last rebuilt 2026-09-12 (`humble-726a93d1ff05`), and every provisioning step
  in both jobs reports success — clang-format, compile-tier sources, submodule
  fetches, the message bindings, the compile-check fixtures. The W1 package
  drift (`unzip`, `python3-tomli`) is in the **Zephyr** image, which neither
  lane uses. The image's own size does sit on the same filesystem and is
  therefore part of the 124 G that is gone before `just ci tier1` starts, but
  nothing about it changed when these lanes went dark: the gate failures in the
  table at the top of this issue run from 09-03, before that image build.

### `check-lane-contracts` is not the gap either

The obvious 0196-shaped suspicion — that the affordability rule only looks at
merge-gating lanes, so a schedule-only `check build` could resolve artifacts its
job never builds — is **already closed**. `SCANNED_EVENTS` was widened from
`GATING_EVENTS` to include `push`/`schedule`/`workflow_dispatch` on 2026-09-05
(issue 1030), keeping the severity distinction in the output (`[gating]` vs
`[report]`) instead of in the scope. Run here: `18 test target(s) across 3
affordability tier(s) and 21 CI lane invocation(s) (3 merge-gating, 18
report-only); none resolves an artifact its job does not build.` The scheduled
`gate` job does build the bindings and the `.compile-ok` fixtures that
`check build` needs, in steps 20 and 21, and both succeed.

### What this commit does about it

Two changes, neither of which is the remedy this issue is still waiting for.

1. **The measurement now leaves the runner.** `disk-report.sh` writes its
   one-line summary to `$GITHUB_STEP_SUMMARY` (uploaded when the STEP completes,
   so the BEFORE report's numbers are server-side before the compile tier
   starts) and emits a `::notice::` (the experiment the section above asks for —
   if the next scheduled `gate` failure shows it under
   `check-runs/<jid>/annotations` while the log is still `BlobNotFound`, the
   annotation channel is confirmed; if it does not, the step summary is what
   carried the numbers).
2. **A reclaim, priced against the numbers this issue already has.**
   `scripts/ci/reclaim-disk.sh`, run between the two brackets on `host-tests`
   and before `check build` on `gate`, frees three things nothing downstream
   reads: the mounted hosted tool cache (`$RUNNER_TOOL_CACHE`, i.e. `/__t` — on
   the same filesystem, and never opened by a `container:` job whose toolchains
   come from the image and whose JavaScript actions run on the runner's bundled
   node), `<repo>/build/metadata-probe` (the shared sizing-probe cargo scratch
   from issue 0522 — 11 G in a developer checkout; its product is a JSON sidecar
   written elsewhere), and the apt/pip caches `live-peer.yml` was removing
   inline. `live-peer.yml` now calls the same script, so the three lanes this
   issue measured share one reclaim rather than three spellings.

**Why this is not the `rm -rf` antipattern.** That rule is about build OUTPUT,
where a wipe destroys the reproduction of a missing dependency edge. None of the
three is an artifact anything reads again, so removing them cannot change what
gets built — only how much room the next step has.

**What is NOT established.** How much it frees. `/__t` is ~9 G on an
`ubuntu-22.04` image by reputation rather than by a measurement taken here, and
`build/metadata-probe` in CI is an unmeasured fraction of the 10 G `build/`
recorded above. The reclaim prints its own delta through the same two channels,
so the next run answers it. If the answer is "not enough", the remaining roads
are unchanged and now cost-comparable: prune the four large `examples/workspaces`
fixtures (36 G, a coverage question), or move these lanes off a hosted runner.

**And expect a second fault behind this one.** These lanes have produced no
verdict for eleven days, so nothing here distinguishes "the disk was the only
problem" from "the disk was the first problem". `check build` was independently
red on this lane on `workspace-all`/`workspace-features` since 2026-09-01
according to `gate.yml`'s own comment, and issue 0981 records `codegen_golden`
red on it for a day. A green run is the only thing that would retire that
expectation; a run that merely gets further is progress, not a verdict.

Acceptance is unchanged: a scheduled run reaching a VERDICT on `check build`
and `check no-std`, three nights running.
## Three lanes in six hours, and the failure has moved into the runner itself (2026-09-23)

The section above reads `gate` and `host-tests`. There was a **third** lane in
the same window: between 02:02 and 04:17 UTC the same self-hosted runner lost
three scheduled jobs on three different lanes to this issue, and **two of the
three produced no failing step at all**:

| run | job | lane | what the API reports |
| --- | --- | --- | --- |
| 35808783184 | 107015560275 | `gate` (schedule) | step `just check build`; log is **2 lines** |
| 35813854577 | 107031060491 | `host-tests` (schedule) | step `just ci tier1`; `Free space left: 91 MB` |
| 35817736997 | 107042793783 | `live-peer` (schedule) | **no failing step** — step 9 is still `in_progress`, steps 10-13 `pending` |

For the first and third, the only diagnosis is
`gh api repos/NEWSLabNTU/nano-ros/check-runs/<jid>/annotations`, and both say
the same thing:

```
System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20260923-041733-utc.log'
   at System.IO.StreamWriter.Flush(Boolean flushStream, Boolean flushEncoder)
   at System.Diagnostics.TextWriterTraceListener.Flush()
```

That is the runner process failing to write **its own diagnostic log**, not a
build failing to write an object file. It explains the shape the live-peer job
has: its log holds 48,534 lines and stops mid-compile at 05:24:42, while the job
is marked failed at 05:40:11 — sixteen minutes during which nothing more could
be recorded. A job that dies this way has no failing step to name, so the
`nightly-triage`/`queue-triage` idiom of keying on the failing step name returns
nothing for it, and it reads as an unexplained infrastructure loss rather than
as this issue.

**Consequence for step 1's report.** The section above records that the
`disk-report.sh` "after" step does not survive on the scheduled `gate` lane.
This is why, and it is stronger than "the step was skipped": once the runner
cannot write to disk, no step output survives at all, including a step guarded
by `always()`. **Annotations are the only surviving evidence, because GitHub
writes them server-side.** Any future reporting this issue adds has to assume
the job's own output is gone.

**The trajectory, from the one job that did keep its log.** `host-tests` 03:19
prints `df -h /` around its build steps:

```
before:            overlay  146G   70G   76G  48% /
after rust-core:   overlay  146G   75G   72G  52% /
before ci tier1:   overlay  146G  124G   22G  86% /
end:               overlay  146G  146G  256K 100% /
```

70 G is already used when the job starts — that is the accumulation this issue
measured on 2026-09-22 — and `just ci tier1` alone takes it from 22 G free to
256 K. Nothing in the window reclaimed anything: the numbers at the start of
this job are where the previous job left them.

Unchanged: acceptance is a scheduled run reaching a verdict on `check build`
and `check no-std` three nights running.

## The reclaim ran, freed 7.8 G, and the tier still ran out (2026-09-23)

`ad1ac08b3` put `scripts/ci/reclaim-disk.sh` in front of the tier on `gate`,
`host-tests` and `live-peer`, and asked the next run to answer whether freeing
what is already spent is enough. **It is not**, and the run says so precisely.

`host-tests` push run **35843238815**, job **107123101951**:

```
::notice:: 86% used, 22G free — 42G examples; 10G build; 404M target
disk reclaim — before just ci tier1
  reclaiming    5.6G   /__t
  reclaiming    2.2G   /__w/nano-ros/nano-ros/build/metadata-probe
  freed 7933 MB; 30494628 KB free
…
##[warning] You are running out of disk space … Free space left: 16 MB
===== FAIL (cpp, rc=101, 8636ms) =====
error: recipe `build` failed on line 233 with exit code 1
```

So the tier entered with **29.1 G free** — 7.8 G more than the 22 G its
predecessors had — and consumed all of it, failing at the same gate, in the
same recipe, four lines after the same warning. The reclaim is not wasted (it
is 7.8 G nobody was using), it is simply smaller than the deficit.

**What this settles.** The third road in `ad1ac08b3`'s reasoning — free what is
already spent on things nothing downstream reads — has now been measured and
does not close this issue on its own. Of the roads that remain:

- **Prune what the tier builds.** The 42 G is `examples`, which the 2026-09-23
  measurement broke down as `workspaces` 36 G plus `templates` 5.2 G. This is a
  question about `fixtures.toml` coverage, not about disk, and it is the only
  road whose size is bigger than the deficit.
- **Move these lanes off a hosted runner.** Both are `container:` jobs, so a
  container cannot delete the host's preinstalled tooling, and the runner's
  146 G is the ceiling.

A fourth possibility this run does not exclude: the tier's own consumption may
itself be reducible — it wants more than 29 G to run `check build`, and nothing
here says that number is necessary rather than incidental.

Acceptance is unchanged: a scheduled run reaching a VERDICT on `check build`
and `check no-std`, three nights running.

## Reproduced on a second run, and the after-report names where it goes (2026-09-23)

`host-tests` push run **35849381577**, job **107143164777** — the second run to
carry the reclaim, and it repeats the first to the megabyte:

```
::notice:: 86% used, 22G free — 42G examples; 10G build; 404M target
  reclaiming 5.6G /__t ; reclaiming 2.2G build/metadata-probe
  freed 7933 MB; 30480904 KB free
##[warning] … Free space left: 25 MB
===== FAIL (cpp, rc=101, 8929ms) =====
```

Same reclaim, same 29.1 G entering the tier, same gate. So the previous
section's finding is a property of the lane, not a one-off.

What is new is the AFTER report, which this run reached:

```
--- du -sh of the usual suspects ---
6.9G   target
13G    packages/cli/target
13G    build
42G    examples
1.3G   /usr/local/cargo
```

Against the before-report's `42G examples; 10G build; 404M target`, the tier
grew `packages/cli/target` from **404 M to 13 G** and `build` from **10 G to
13 G**, and created a workspace `target/` at **6.9 G** — about **22 G** of
growth inside the checkout, against 29.1 G available.

Two things follow that the earlier sections could not say:

- **`examples` is not what the TIER spends.** Its 42 G is identical before and
  after; it is the cost of *arriving*, already paid by the fixture build. So
  pruning it enlarges the runway but does not shrink the tier's own appetite,
  which is the ~22 G above.
- **The three categories do not account for the whole disk.** The filesystem is
  146 G at 100 %, and everything the report measures sums to ~76 G. The rest is
  outside the checkout and outside this report's reach.

Acceptance is unchanged: a scheduled run reaching a VERDICT on `check build`
and `check no-std`, three nights running.

## The evidence-channel experiment has an answer, and it is NO for both (2026-09-24)

The section "The report does NOT survive this failure on the scheduled `gate`
lane" names the cheap experiment: emit a `::notice::`, let the next scheduled
`gate` fail, and see whether it appears under `check-runs/<jid>/annotations`.
Phase-466 W4 emitted it, and added a `$GITHUB_STEP_SUMMARY` line beside it as
"the one expected to work". The next scheduled `gate` is run **35945635228**,
job **107462875362** (02:04 UTC), and it answers both at once:

| channel | emitted by | survived? |
| --- | --- | --- |
| `::notice::` | steps 22, 23 and 25, all of which report **success** | **no** |
| `$GITHUB_STEP_SUMMARY` | the same three steps | **no** |
| the job log | — | no (`BlobNotFound`, as always on this lane) |

`gh api repos/NEWSLabNTU/nano-ros/check-runs/107462875362/annotations` returns
**exactly one** annotation, and it is the runner's own

```
System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20260924-020406-utc.log'
```

which the SERVICE writes, not the runner. The run page renders no job summary
for the job at all.

**The channel is not broken — it is runner-side.** The control is the sibling
`host-tests` job **107478998501** from the same night, whose runner lived: its
annotations carry all three of the same notices, titled and intact
(`disk before just ci tier1`, `disk reclaim before just ci tier1`,
`disk after just ci tier1`). So a workflow-emitted annotation is buffered on the
runner and flushed later; when the runner dies there is nothing to flush it, and
the step summary behaves the same way.

That leaves the other road this issue named: **an artifact uploaded before the
compile tier**. An artifact is a completed server-side transaction — once the
upload STEP finishes, the blob exists independently of the runner — and it is
the only channel left that a dead runner cannot retract.

## Two corrections to this issue's own record (2026-09-24)

Both are things a reader would otherwise carry forward wrongly.

**1. The reclaim ran, and `check build` produced a VERDICT.** The step list of
job 107462875362 is not the shape the 2026-09-23 sections describe:

| step | state |
| --- | --- |
| 22 `Disk report (before check build)` | success |
| 23 `Reclaim disk before the compile tier` | **success** |
| 24 `just check build` | **failure** |
| 25 `Disk report (after check build)` | **success** |
| 26 `just check no-std` | **in_progress** — the runner died here |
| 27-32, `Stop containers` | pending |

So the runner did NOT die inside `check build`: that step failed and the
after-report then ran to completion, which means the process was alive and
writing. The death moved one step later, into `just check no-std`. Whether step
24's failure was the disk or a real gate verdict cannot be read — that is the
whole of the problem above — but the after-report's success is evidence that the
job was still functioning when it happened.

**2. `host-tests` did not get past `just check build` either.** The scheduled
`host-tests` of the same night — run **35950894731**, job **107478998501**
(03:18) — failed at step 10, `Build rust core fixtures`:

```
error[E0432]: unresolved import `crate::parameter_services`
error: could not compile `nros-node` (lib) due to 2 previous errors
error: recipe `build-fixture-rust-core` failed with exit code 2
```

That is issue **1468**, a code defect, and step 14 `just ci tier1` was
**skipped**. Its disk report therefore describes a job that aborted before
spending anything: `50% used, 74G free`, `examples` **19 M** (not 42 G),
`build` 2.6 G, no workspace `target/` at all. The reclaim freed 6,695 MB there
(`/__t` 5.2 G + `build/metadata-probe` 1.4 G) against a disk that was not under
pressure. **None of those numbers say anything about this issue**, in either
direction, and reading that run as "the reclaim fixed host-tests" would be
reading an early abort as a success.

## What this commit does, and what it deliberately does not (2026-09-24)

`scripts/ci/disk-transcript.sh` is the one spelling of where the numbers are
written; `disk-report.sh` and `reclaim-disk.sh` append everything they print to
it, and `gate.yml`, `host-tests.yml` and `live-peer.yml` upload it in a step of
their own placed immediately after the report or reclaim that produced it.
`gate` gets **two** uploads, before and after the compile tier, rather than one
`always()` upload at the end of the job: `always()` only binds while a runner is
alive to honour it, which is exactly what this failure removes. The second one
sits between the after-report and `check no-std`, because that is where the
runner now dies.

It frees nothing new. The remedy this issue is still waiting for — prune what
the tier builds (36 G of `examples/workspaces`, a fixture-coverage question),
reclaim between `check build` and `check no-std`, shrink the tier's own ~22 G
appetite, or move the lane off a hosted runner — is unchanged and still has
**no `gate` numbers to be priced against**: no scheduled `gate` run has ever
produced a readable disk figure, and there are no `workflow_dispatch` runs
either. The 42 G/22 G split every section above argues from is `host-tests`, a
different job with a different build.

Acceptance is unchanged: a scheduled run reaching a VERDICT on `check build`
and `check no-std`, three nights running.

## The tier's own appetite, MEASURED at last (2026-09-24, `host-tests` push)

Run **36022653723**, job **107711155226**, head `ba1a6f70b`, step **`just ci
tier1`**. The job log is `BlobNotFound` and the only surviving evidence is the
annotation, which is this issue's signature:

```
Unhandled exception. System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20260924-160424-utc.log'
```

**Why this run is different from every earlier one quoted above.** The section
before this one records that the `host-tests` numbers then available came from
a job that aborted at `Build rust core fixtures` on issue 1468, never reached
`just ci tier1`, and therefore said nothing in either direction. Issue 1480 was
the next wall and it was fixed in #1261; this is the FIRST `host-tests` run to
get past `Build workspace fixtures` — that step reads `success` here — so it is
the first time the tier's own consumption has been observable at all.

**What it spent.** From `disk-transcript-before-tier1` (the artifact upload
this issue added, which is why these numbers survive a dead runner):

```
SUMMARY disk before just ci tier1 — 88% used, 18G free
  — 42G examples; 14G build; 410M packages/cli/target
reclaiming    5.2G  /__t
reclaiming    2.2G  /__w/nano-ros/nano-ros/build/metadata-probe
freed 7517 MB; 26029560 KB free
```

So the tier started with **26,029,560 KB (~24.8 GiB) free**, on a checkout
whose `examples/` was already the full 42 G (`workspaces` 37 G, `templates`
5.3 G), and `just ci tier1` consumed **all of it** before finishing. The
estimate this issue has been carrying — "the tier's own ~22 G appetite" — is
now a floor with a measurement under it rather than an inference: **≥ 24.8 GiB,
on top of a 42 G examples tree, after the reclaim has already run.**

**The reclaim is not the gap.** Steps 12 and 13 (`Disk report (before ci
tier1)`, `Reclaim disk before the tier — issue 1353`) both read `success`, the
reclaim found and freed everything it knows how to free, and the tier exhausted
the result anyway. Whatever closes this issue has to reduce what the tier
BUILDS or move the lane off a 146 G hosted runner; it is not another reclaim
site.

**What this does NOT say.** Nothing about the scheduled `gate` lane, which
still has no readable disk figure of its own — the split above is `host-tests`,
a different job with a different build, and that caveat stands unchanged. It
also does not say the tier would have PASSED with more disk: no tier-1 verdict
has been produced on `main` since 2026-06-17 (2 successes against 356 failures
on this workflow), so what the tests would have reported is still unknown.

Acceptance is unchanged.

### Reproduced, and the figure is DETERMINISTIC (second run, same day)

Run **36033675501**, job **107748375245**, head `a7f829300`, two hours after
the one above and on a `main` four commits further along. Same shape and the
same annotation (`No space left on device : '…/Worker_20260924-174242-utc.log'`),
same `steps_failed=[]`, same `Build workspace fixtures = success` then death
inside `just ci tier1`. Its transcript:

```
SUMMARY disk before just ci tier1 — 88% used, 18G free
  — 42G examples; 14G build; 409M packages/cli/target
freed 7517 MB; 26033844 KB free
```

against the first run's `26029560 KB free`. **The two agree to within 4 MB**,
so what the tier needs is not load-dependent or a one-off: it is a stable
number that a remedy can be priced against, and ~24.8 GiB of headroom after
the reclaim is reproducibly NOT enough. The earlier section's "≥ 24.8 GiB" is
therefore a measurement rather than an observation.

Nothing else about the two runs differs, so this adds no new cause — only the
confidence that the existing one is exact.

### The AFTER numbers, and they name the directory that grows

Run **36051691975**, job **107808557432**, head `95693968d`. The first run of
this lane whose post-tier steps RAN: `just ci tier1` reports `failure` and then
`Disk report (after ci tier1)` and its upload both report `success`, so the
runner survived and the after-transcript exists. Every earlier instance killed
the worker mid-step and left only an annotation.

```
SUMMARY disk before just ci tier1 — 88% used, 18G free
  — 42G examples; 14G build; 409M packages/cli/target
freed 7518 MB; 26023412 KB free

SUMMARY disk after  just ci tier1 — 100% used, 84K free
  — 42G examples; 14G packages/cli/target; 13G build
```

Two things follow, and the second is the useful one.

**Third reproduction of the entry figure.** `26023412 KB` against `26029560`
and `26033844` from the two runs above: three runs, spread over four hours and
several commits, agreeing within **10 MB**. The tier's starting headroom is a
constant.

**`packages/cli/target` grows from 409 MB to 14 GB DURING THE TIER** — about
13.6 GB, and it is the only entry that moves. `examples/` is 42 G before and
42 G after; `build/` goes 14 G to 13 G. So the tier does not overflow the disk
by building more examples: it overflows it by filling the CLI's own cargo
target directory, from a state the CLI-build steps had left at 409 MB.

That redirects the remedy this issue has been carrying. "Prune what the tier
builds (36 G of `examples/workspaces`)" is aimed at a tree that does not grow
here; the 13.6 GB that does is `packages/cli/target`, which is also the
directory `gate.yml` CACHES. Whether the right move is a `--target-dir` the
tier shares with the earlier steps, a prune between the CLI build and the tier,
or a smaller profile for the tier's own cargo invocations is a maintainer
decision — but it can now be aimed at a measured 13.6 GB rather than at an
estimate.

**What this does NOT say.** It is still not a test verdict: `just ci tier1`
failed on the disk, not on an assertion, so what tier 1 would REPORT on a
runner with room remains unknown, and the two successes since 2026-06-17 stand.
It also says nothing about the scheduled `gate` lane, whose own figures are
still absent. Adjacent but distinct: issue 1491 records 329 MB of untracked
cargo output at the REPO ROOT from a merge-gating gate, which is three orders
of magnitude smaller and a different directory.

### The `packages/cli/target` attribution reproduces (second after-transcript)

Run **36070565655**, head `547c575d5`:

```
SUMMARY disk before just ci tier1 — 89% used, 18G free
  — 42G examples; 14G build; 410M packages/cli/target
freed 7527 MB; 25841676 KB free

SUMMARY disk after  just ci tier1 — 100% used, 92K free
  — 42G examples; 14G packages/cli/target; 13G build
```

Same shape as the section above, independently: `packages/cli/target` goes
**410 MB to 14 GB** during the tier, `examples/` is 42 G either side, `build/`
goes 14 G to 13 G. Two of two runs with an after-transcript agree, so the
attribution is not a single reading.

Entry figure, fourth measurement: `25841676 KB` against `26029560`, `26033844`
and `26023412`. This one is ~180 MB lower than the other three — its BEFORE
line also reads 89 % rather than 88 %, so the runner started marginally fuller;
the reclaim freed the same 7.5 GB. The tier's requirement is still ~24.6–24.8
GiB and still not met.

## The scheduled `gate` lane's OWN numbers — the gap this issue kept naming

Every section above argues from `host-tests`, and says so: *"no scheduled
`gate` run has ever produced a readable disk figure, and there are no
`workflow_dispatch` runs either. The 42 G/22 G split every section above argues
from is `host-tests`, a different job with a different build."* That is no
longer true.

Scheduled `gate` run **36084587324**, job **107913551185**, head `27f1e8ef5`,
step **`just check build`** (the compile tier, which only the schedule event
reaches). Log is `BlobNotFound`; the annotation is this issue's signature,
`No space left on device : '…/Worker_20260925-020238-utc.log'`. Both disk
artifacts uploaded:

```
SUMMARY disk before just check build — 86% used, 21G free
  — 30G packages/cli/target; 19G examples; 3.5G build
reclaiming    5.2G  /__t
reclaiming    1.5G  /__w/nano-ros/nano-ros/build/metadata-probe
freed 6700 MB; 28690428 KB free

SUMMARY disk after  just check build — 100% used, 432K free
  — 31G packages/cli/target; 19G examples; 16G target
```

**The `gate` lane overflows through a DIFFERENT directory than `host-tests`,
and from a different starting state.** Side by side:

| | scheduled `gate` (`check build`) | `host-tests` (`ci tier1`) |
| --- | --- | --- |
| headroom after reclaim | 28,690,428 KB (~27.4 GiB) | ~24.6–24.8 GiB |
| `examples/` | 19 G, unchanged | 42 G, unchanged |
| `packages/cli/target` | **30 G going in**, 31 G after | 410 MB going in, **14 G after** |
| repo-root `target/` | **1.1 G → 16 G** | not a mover |

So two lanes, two growers: the tier fills `packages/cli/target` from nearly
nothing, while the compile tier arrives with that directory already at 30 G and
fills the repo-root `target/` instead, by ~14.9 GB. A remedy aimed at one does
not help the other, which is exactly why this issue insisted the `host-tests`
numbers could not be spent on the `gate` lane.

**What this does NOT say.** Nothing about why `packages/cli/target` is 30 G here
and 410 MB there — the two jobs restore different caches and run different
recipes, and this is one observation of each, not a characterisation. Nothing
about whether `check build` would PASS with room. And it is adjacent to but
distinct from issue **1491**, which measures 329 MB of untracked cargo output at
the repo root: same directory, three orders of magnitude apart, and 1491 is
about what is left BEHIND rather than what is needed DURING.

Acceptance is unchanged — and its first clause, "a scheduled run reaching a
VERDICT on `check build` and `check no-std`", is still unmet: this run reached
neither.

## The road this issue wrote off is open: a `container:` job CAN delete the host's preinstalled tooling (2026-09-25)

Every section above that prices a remedy ends at the same two roads — prune
what the tier builds, or move the lane off a hosted runner — because of one
sentence, in `reclaim-disk.sh`'s own header and repeated in the 2026-09-23
measurement:

> Raising the runner is not available here (both lanes are `container:` jobs,
> and a container cannot delete the host's preinstalled tooling).

**That is true of the image's filesystem and false of anything the job
MOUNTS**, and this issue already contains the counter-example. Candidate 1 of
the reclaim is `$RUNNER_TOOL_CACHE`, i.e. `/__t`, which IS the host's
`/opt/hostedtoolcache` — mounted into the container by the runner — and
deleting through it has freed a measured **5.2 G every run** since phase-466
W4. A bind mount is not an overlay layer: unlinking through one frees space on
the same `/dev/root` that `/__w` lives on, which is the filesystem every number
in this issue is measured against. The only thing special about `/__t` is that
the runner mounts it for us. A job may mount whatever else it likes.

So the third road exists and had been ruled out by a premise rather than by a
measurement.

### What landed

`host-tests.yml`'s integration job now declares:

```yaml
      volumes:
        - /usr/local/lib/android:/__host-reclaim/android
        - /usr/share/dotnet:/__host-reclaim/dotnet
        - /usr/local/.ghcup:/__host-reclaim/ghcup
        - /opt/ghc:/__host-reclaim/ghc
        - /usr/share/swift:/__host-reclaim/swift
        - /usr/local/share/powershell:/__host-reclaim/powershell
```

and `reclaim-disk.sh` gains one candidate class: the CHILDREN of
`/__host-reclaim`, emptied and reported exactly like the other two. The mount
list lives in the workflow because that is the only place a host path can be
named; the script removes what was mounted and nothing else, so a lane that
mounts nothing behaves exactly as before.

The safety argument is the one candidate 1 already makes and needs no new
premise: this job's compilers, Python and Node come from the `nano-ros-ci`
image, its JavaScript actions run on the runner's bundled node under `/__e`,
and no step in the workflow is a `setup-*` action. Nothing in any of these
lanes reads Android, .NET, GHC, Swift or PowerShell.

### What is NOT established, and how it gets answered

**How much it frees.** Those six trees sum to roughly 20 G on the published
`ubuntu-22.04` image — but this runner reports a **146 G** disk where that
image has 75 G, so it may not be that image, and nothing has priced them here.
The arm prints its own `du` per tree and the `freed N MB` delta into the
transcript artifact, which is the one channel this issue established survives a
dead runner, so the next `host-tests` run on `main` answers it. Against the
tier's reproducible ~24.8 GiB of entry headroom and the ~22 G of growth the two
after-transcripts attribute, roughly 20 G would be decisive and roughly 5 G
would not; the run says which.

**Whether the tier then PASSES.** It does not say that and cannot. No tier-1
verdict has been produced on this lane since 2026-06-17, so a run that gets
further is progress and not a verdict — the same caveat the 2026-09-23 section
attaches to the first reclaim, and the "expect a second fault behind this one"
warning stands unchanged.

### The sweep, and why it is staged

Three jobs call `reclaim-disk.sh`: `host-tests`' integration job, `gate`'s
`check` job (schedule/dispatch only), and `live-peer`'s `board` job. Only the
first got the mounts.

That is deliberate and it is a deferral, not an oversight. `gate`'s `check` job
is the source of the repository's ONE required status check, and a `volumes:`
entry that docker refuses would fail every pull request before a step ran.
`host-tests` gates nothing and reaches its container in about ninety seconds,
so it prices the mount at no risk to anyone's merge. The arm itself is already
exercised off a runner — `NROS_CI_HOST_RECLAIM_ROOT` is a test seam, and the
probe covers a populated tree, an empty one, a non-directory child and an
absent root. When a `host-tests` run has reported a delta, the same six lines
belong on the other two jobs.

Acceptance is unchanged: a scheduled run reaching a VERDICT on `check build`
and `check no-std`, three nights running.

## 2026-09-25 — the FIRST measured before/after pair, and the tier still ends at 100 %

Every disk transcript this issue wanted was lost with its job (issue 1492: nine
consecutive wedges, all `BlobNotFound`). Run **36146158187**, job
**108107735210**, is the first `nros-tests integration (host)` to finish its
steps, so its `disk-report.sh` output survived:

```
before just ci tier1:
Filesystem      Size  Used Avail Use% Mounted on
/dev/root       146G  128G   18G  89% /__w

after just ci tier1:
/dev/root       146G  146G  272K 100% /__w
```

Mid-step, GitHub's own warning: `Free space left: 31 MB`.

So on a **146 G** runner the tier arrives with the disk already **89 % full**
— 128 G consumed before `just ci tier1` starts — and consumes the remaining
**18 G**, ending at **272 K free, 100 %**. This is measurement, not inference:
the two `df` lines are from the same job, 6 minutes apart in the log.

Two things follow, and neither is a diagnosis:

* **#1313's reclaim did not stop the exhaustion.** It plausibly bought the run
  enough headroom to FINISH (this is the first job that did), which is how the
  transcript exists at all. But the tier still ends at 100 %.
* **The tier's own failure may be a consequence.** The step reported
  `multi-package-workspace: FAIL — the copy does not build`, a template
  copy-out into `/tmp`, on a filesystem that reached 272 K. The log line is
  truncated mid-word, so this cannot be settled here; it is the first thing to
  check on the next run that finishes.

What would make the 128 G attributable: the before-transcript that
`scripts/ci/disk-report.sh` writes now reaches its artifact, because the job
finishes. So the next run can be asked WHAT holds those 128 G before the tier
starts. Until one is read, the figure is a total and nothing more.
