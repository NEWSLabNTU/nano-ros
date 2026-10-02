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

## 2026-09-25, second finishing run — the 128 G is ATTRIBUTED, and the tier's own 30 G is the exhaustion

The previous append asked the next finishing run WHAT holds the 128 G. Run
**36171483132**, job **108191821508** (18:32:56 → 20:52:56, all 23 steps) answers
it: `disk-report.sh` runs a `du` breakdown either side of the tier, and both
survived.

**Before `just ci tier1`** (19:48:04Z), `/dev/root 146G 128G 18G 89%`:

```
42G   examples/            ← 37G examples/workspaces, 5.3G examples/templates
14G   build/
1.1G  packages/
409M  packages/cli/target
110M  .git
```

**After** (20:52:27Z), `/dev/root 146G 146G 264K 100%`:

```
42G   examples/      (unchanged)
19G   build/         (+5G)
16G   target/        (did not exist before — 13G target/debug, 3.0G nros-relwithdebinfo)
15G   packages/      (+14G, of which packages/cli/target is 14G, was 409M)
~3.6G target-check-{c,cpp,cpp-clippy-zenoh,cpp-cyclone-embedded,census-hooks}/ + target-embedded + target-excluded-tests
589M  tmp/
```

So the split is clean, and neither half is the other's fault:

* **~57 G is standing state** the tier inherits — `examples/` 42 G and `build/`
  14 G, left by the fixture build that runs before it. Unchanged across the
  tier, and identical to the byte in the first finishing run.
* **~30 G is what `just ci tier1` CREATES** — a 16 G repo-root `target/` plus
  `packages/cli/target` growing 409 M → 14 G, against **18 G** of headroom.
  That is the exhaustion, and it is arithmetic rather than inference.

The remaining ~70 G of the 128 G is outside the checkout (image, toolchains,
`/usr/local/cargo` at 1.3 G, and whatever else the runner image carries); this
report does not break that down.

### The `before` figure is DETERMINISTIC, which makes it a budget

Both finishing runs report `146G 128G 18G 89%` before the tier — the same
numbers, hours apart, on different commits. So 18 G is not a fluctuating
margin, it is the budget the tier has, and the tier wants about 30 G.

### The copy-out FAIL now has a well-supported cause, still not proven

Both runs fail the same check with the disk at 100 %:

```
  multi-package-workspace: FAIL — the copy does not build
```

It copies a template to `/tmp/nros-template-copy-out.XXXXXX` and builds it, and
`/` is the same 146 G filesystem that the report shows at **264 K free**
(`Free space left: 7 MB` mid-step; 31 MB in the first run). A build with no
space is the obvious reading and two-for-two is not a coincidence — but the
error line is still truncated mid-word, so this is not proven from the log.
What would prove it: the same check passing on a run that does not reach 100 %,
or an `ENOSPC` visible in an untruncated tail.

## 2026-09-25, third finishing run — the reclaim frees 33 G and the fixtures eat it

Run **36177357218**, job **108211089668** (2 h 19 m 55 s, 23 steps) reproduces the
previous two to the gigabyte, and its check-run annotations carry the figures
without opening the log at all:

```
freed 33286 MB; 52100356 KB free
89% used, 18G free — 42G examples; 14G build; 409M packages/cli/target     (before the tier)
100% used, 196K free — 42G examples; 19G build; 16G target                  (after)
```

Three runs, three identical `before` breakdowns. The new number is the first one:
**#1313's reclaim frees 33.3 G and leaves ~49.7 G free** — and by the time the
tier starts, only **18 G** remains. So about **31 G disappears between the reclaim
and the tier**, which is the fixture build, and that is on top of the ~30 G the
tier itself then wants.

Stated as a budget on a 146 G runner:

| stage | free after |
| --- | --- |
| after `reclaim-disk.sh` | ~49.7 G |
| before `just ci tier1` (fixtures built) | 18 G |
| after `just ci tier1` | 196 K |

That reframes the remedy space. Reclaiming harder buys at most the 33 G it
already finds; the two consumers that matter are the **fixture build's ~31 G**
and the **tier's ~30 G**, against a 146 G disk that arrives with ~70 G already
spoken for outside the checkout. Any fix that does not reduce one of those two,
or move the lane to a bigger disk, is arithmetic that does not close.

Issue **1492** (the wedge this evidence used to arrive through) is resolved as of
this run; tier-1 jobs now finish and report, so this issue's transcripts will keep
arriving.

## The `gate` numbers exist now, and they are not the numbers this issue argued from (2026-09-27)

The transcript uploads added on 2026-09-24 worked. Scheduled `gate` run
**36287553068** (02:06 UTC), job **108531095406**, produced
`disk-transcript-before-check-build` and `disk-transcript-after-check-build`,
and they are the first disk figures any scheduled `gate` has ever emitted.

The step table reproduced the 2026-09-23 shape exactly — 23 report, 24 reclaim,
25 upload, **26 `just check build` failure**, 27 after-report success, 28 upload,
**29 `just check no-std` in_progress, where the runner died** — so nothing about
the failure moved. What moved is that it is now readable.

| moment | df | biggest three |
| --- | --- | --- |
| 02:47, before the tier | **86% used, 21G free** | `packages/cli/target` **30G**; `examples` 19G; `build` 3.5G |
| after `reclaim-disk.sh` | ~27.3G free | freed **6,703 MB** (`/__t` 5.2G + `build/metadata-probe` 1.5G) |
| 03:32, after the tier | **100% used, 272K free** | `packages/cli/target` 31G; `examples` 19G; `target` **15G** |

So the compile tier's own spend, measured on `gate` rather than inferred from
`host-tests`, is about **24G**: `target` 1.1G → 15G, `build` 3.5G → 8.8G, plus
~4.6G of eight `target-check-*` directories that did not exist before the step
(`target-embedded` 956M, `target-check-cpp-cyclone-embedded` 816M,
`target-check-c` 753M, `target-check-census-hooks` 687M, `target-check-cpp`
589M, `target-check-cpp-clippy-zenoh` 471M, `target-excluded-tests` 331M).
Against 27.3G available that does not fit with any margin, and the disk was
**full inside step 26** — which is what makes step 26's failure attributable to
the disk rather than to a gate verdict, for the first time.

Two corrections to what the sections above assume:

- **The 42G/22G split is a `host-tests` shape and does not describe `gate`.**
  Here `examples` is 19G, not 42G, and the single biggest consumer on the disk
  is **`packages/cli/target` at 30G**, already present at 02:47 — i.e. spent by
  earlier steps of this same job, not by the tier. Whatever prunes usefully
  here, it is not first of all `examples/workspaces` (14G).
- **The job log cannot be read at all.** The annotation on job 108531095406 is
  the runner's own crash:

  ```
  Unhandled exception. System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20260927-020659-utc.log'
  ```

  The worker process died writing its diagnostic log, so it never uploaded the
  step log and `actions/jobs/108531095406/logs` returns `BlobNotFound`. For this
  failure mode the annotations and the two transcript artifacts are the only
  evidence there is; that is why the uploads are placed per-step rather than
  behind one `always()` at the end.

Note when reading the artifacts: the transcript is **cumulative**, so the
`after` artifact contains the `before` report and reclaim as well. The two
uploads are two snapshots of one growing file, not two independent measurements.

Acceptance is unchanged — a scheduled run reaching a VERDICT on `check build`
and `check no-std`, three nights running — and the remedy list is unchanged,
but it can now be priced: the tier needs ~24G and the reclaim currently hands it
~27.3G on a disk that arrives 86% full.

## One night later the headroom is GONE, and the growth is all in one directory (2026-09-28)

The 2026-09-27 section closed by pricing the remedy: "the tier needs ~24G and
the reclaim currently hands it ~27.3G on a disk that arrives 86% full." The next
scheduled `gate` — run **36368803015** (02:10 UTC), job **108760546805**,
failing at step 26 `just check build` — measured all three of those numbers
again, and every one moved the wrong way.

| | 2026-09-27 (36287553068) | 2026-09-28 (36368803015) |
| --- | --- | --- |
| before the tier | 86% used, **21G** free | **94% used, 9.7G** free |
| `packages/cli/target` | 30G | **41G** |
| `examples` | 19G | 19G |
| `build` | 3.5G | 3.5G |
| reclaim frees | 6,703 MB | 6,704 MB |
| post-reclaim headroom | **~27.3G** | **~16.2G** |
| after the tier | 100% used, 272K free | 100% used, **264K** free |

Two things this says that one measurement could not.

**The growth is not spread across the checkout — it is `packages/cli/target`,
+11G in a day**, while `examples` (19G) and `build` (3.5G) are unchanged to the
gigabyte. The 2026-09-27 section already corrected the earlier premise that
`examples/workspaces` was the thing to prune; this narrows it further. Whatever
prunes usefully here is the CLI's own target directory, and it is *growing*,
which the 42G in the after-report confirms is not a one-off.

**The tier now dies EARLIER, and the after-report proves it rather than
suggesting it.** Yesterday `check build` got `target` to 15G and `build` to
8.8G before the disk filled. Tonight, starting with 11G less, it reached
`target` **5.7G** and `build` **7.2G**. Same step, same tier, less than half the
`target` output — so the ~24G figure is the tier's *appetite*, not what it gets
to spend, and the reclaim's fixed 6.7G no longer covers the gap it was sized
against.

### The tier-1 PUSH lane is the same failure, four runs running

Not scheduled `gate` — `host-tests` on the **push** event, which emits the
runner's disk warning but no transcript:

| run | head | died at | warning |
| --- | --- | --- | --- |
| 36322794885 | `6e1a356e0` | 2h39m | `Free space left: 84 MB` |
| 36331789788 | `4ac8244d4` | 2h32m | `Free space left: 76 MB` |
| 36354992627 | `205cfc959` | 2h06m | `Free space left: 95 MB` |
| 36359825546 | `1f8b1d442` | 2h16m | `Free space left: 2 MB` |

All four end the same way: the warning, then a log **truncated mid-line**
(`… /pkg_rust_publiserror: recipe 'build' failed on line 233`, two lines mashed
together), then `error: recipe 'tier1' failed with exit code 1`. Each ran past
the ~2h20m envelope issue 1492's resolution measured for a clean tier-1 finish,
so these are 1353 on a second lane and **not** 1492 recurring — 1492's own
acceptance was three clean 23-step finishes, which these are not.

What is still missing is a `host-tests` disk transcript: that lane uploads none,
so its four deaths have a warning and no figures behind them. Pairing them
against `gate`'s numbers is inference, stated as such.

Acceptance is unchanged. The remedy list is unchanged, and now has a direction:
the arrival state is what moved, `packages/cli/target` is what moved it, and a
fixed 6.7G reclaim against a disk arriving 94% full cannot hold.

## The `live-peer` lane dies of this too, and its workspace mount had 65 G free (2026-09-28)

Every section above measures `/__w`, because that is what `disk-report.sh`
looks at and what the reclaim can act on. Two consecutive `live-peer
regression` nights show a failure with this issue's signature where that
measurement says there was nothing wrong.

| run | job | step 9 `Build the fixtures those rows resolve` | job ended |
| --- | --- | --- | --- |
| 36293862264 (2026-09-27 04:17) | 108548946263 | started 04:24:54Z, **never completed** | 05:40:28Z |
| 36377215634 (2026-09-28 04:19) | 108785391163 | started 04:27:13Z, **never completed** | 05:49:16Z |

Both jobs conclude `failure` with **no failing step** — steps 10-13 and the
post-checkout/stop-containers steps all carry a null conclusion — and both logs
end mid-line inside `idlc` output with no `##[error]`. The verdict is only in
the check-run annotation, and on both nights it is the same one:

```
Unhandled exception. System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20260928-041930-utc.log'
```

That is the runner AGENT failing to write its own diagnostic log, not a build
step failing to write an artifact — which is why there is no step to attribute
it to and why an `always()` upload cannot rescue it, the same reason the
2026-09-24 section gives for splitting `gate`'s two uploads.

The same annotation, with `Worker_20260928-021055-utc.log`, is what job
**108760546805** carries — the scheduled `gate` run the section above measures.
So the two lanes fail identically at the agent, whatever their build was doing.

### What the live-peer transcript actually reports, and why it does not settle this

`disk-transcript-live-peer-board` for run 36377215634 is three lines:

```
reclaiming    5.2G	/__t
freed 5251 MB; 68816360 KB free
```

**65.6 GiB free**, taken before the SDK setup — roughly four times the 16.2 G
the `gate` job had post-reclaim, on the lane that then died. Two readings fit:
`/home/runner/actions-runner` is a different device from the `/__w` mount the
transcript measures, or step 9 consumed all 65.6 G in the 82 minutes it ran.
The transcript cannot separate them, because it reports once and only on the
mounts inside the container; on the `gate` job `/dev/root` backs both `/__w`
and `/`, which is evidence for neither reading here.

That is the measurement this arm needs: a `df` of the runner host's own volume,
or simply a second live-peer report after step 9, which would say whether the
65.6 G survived. Until one exists, "reclaim more inside `/__w`" is not known to
help this lane, and the direction the section above derived — prune
`packages/cli/target` — is a claim about `gate`, not about this.

What this is NOT: not issue 1364 (`tomllib`), and not issue 1474 — 1474 is real
in the same run, but it fails the OTHER job (**108785696260**, "rows whose board
is NOT this runner") with a verdict, `cargo-clippy ... is not applicable to the
'stable-x86_64-unknown-linux-gnu' toolchain` under `run_rust_clippy`. A lane
with one job dying at the agent and one failing on a named gate reports a single
`failure`, and reading that as one cause is how the second one hides.

Acceptance for this arm: a `live-peer` run whose step 9 reaches a conclusion,
with a disk figure recorded after it.

## Correction: `host-tests` DID upload its transcripts, and tier 1 needs >56 G (2026-09-28)

The section above ends "What is still missing is a `host-tests` disk
transcript: that lane uploads none, so its four deaths have a warning and no
figures behind them. Pairing them against `gate`'s numbers is inference, stated
as such."

**That was wrong when it was written.** All four runs in that table — 36322794885,
36331789788, 36354992627, 36359825546 — carry `disk-transcript-before-tier1`
**and** `disk-transcript-after-tier1` as artifacts, and the job also emits the
figures as check-run annotation notices. The numbers this issue said it did not
have were in the runs it was citing. What follows is what they say, plus a fifth
death, run **36419496642** (12:04 UTC), job **108918531568**, step 15
`just ci tier1`, 2 h 07 m.

| | 36359825546 (2026-09-27) | 36419496642 (2026-09-28) |
| --- | --- | --- |
| before the tier | 89% used, 18G free | 84% used, 25G free |
| `examples` / `build` / `packages/cli/target` | 42G / 14G / **447M** | 35G / 14G / **447M** |
| reclaim frees | 33,286 MB | 33,286 MB |
| post-reclaim headroom | **~49.6G** | **~56.6G** |
| after the tier | 100% used, 264K free | 100% used, 260K free |
| after: `examples` / `build` / `target` | 42G / 19G / 16G | **42G** / **22G** / **16G** |

Three things follow, and none of them carries over from the `gate` sections.

**The host-tooling road is live and it is where the 33 G comes from.** The
reclaim on this lane deletes `/__host-reclaim/android` (11G),
`/__host-reclaim/dotnet` (5.8G), `ghcup` (3.7G), `swift` (3.5G), `powershell`
(1.4G) alongside `/__t` (5.2G) — that is the 2026-09-25 "a `container:` job CAN
delete the host's preinstalled tooling" section in production, and it frees five
times what `gate`'s fixed 6.7 G does.

**Tier 1 still exhausts it, so its appetite is >56 G, not the ~24 G the `gate`
sections price.** Both runs end at 100% with a quarter of a megabyte left. Any
remedy sized against the `gate` figure is sized against the wrong lane.

**The consumers are the exact inverse of `gate`'s.** Within run 36419496642:
`examples` 35G → **42G** (`examples/workspaces` 30G → 36G), `build` 14G → 22G,
`target` → 16G of which 13G is `target/debug` — while `packages/cli/target`
stays **447M**. On `gate`, `packages/cli/target` is 41-42G and `examples` is 19G
and static. So "prune `packages/cli/target`", the direction the 2026-09-27 and
2026-09-28 `gate` sections derived, reclaims **nothing** here, and the 42G
`examples`/36G `workspaces` figure the earliest sections argued from is this
lane's, not `gate`'s. Two lanes, two different directories, one issue.

### The decisive error, which no host-tests section had

Earlier host-tests deaths were a `Free space left: N MB` warning and a log
truncated mid-line. This one names the write that failed:

```
rustc-LLVM ERROR: IO failure on output stream: No space left on device
error: failed to write to `/__w/nano-ros/nano-ros/examples/mps2-an385-freertos/rust/talker/
  target-link-check/nros-minsizerel/deps/rmetamUfMmZ/full.rmeta`: No space left on device (os error 28)
error: recipe `rust-rtos-link-check` failed with exit code 101
error: recipe `tier1` failed with exit code 101
```

So the step that tips tier 1 over is **`rust-rtos-link-check`**, writing a
per-leaf `target-link-check/` directory under `examples/` — which is part of the
same `examples` figure that grew 7 G during this run. That is a specific
candidate the four warning-only deaths could not have identified, and it is
where a tier-1 prune should be priced first.

What this is NOT: not issue 1492. That issue's envelope for a clean tier-1
finish is ~2 h 20 m and this died at 2 h 07 m with an ENOSPC verdict, not a
timeout.

Acceptance for this arm: a `host-tests` push run reaching a tier-1 verdict, with
the after-report below 100%.

## Third `host-tests` point: the arrival state is degrading and the death is earlier (2026-09-28)

Run **36451336301**, job **108974046015**'s successor in the same lane (job id
from `actions/runs/36451336301/jobs`), step `just ci tier1`, integration job
**16:51:39 → 18:51:04 = 1 h 59 m 25 s**. Three points now, all `host-tests` on
the push event:

| | 36359825546 | 36419496642 | 36451336301 |
| --- | --- | --- | --- |
| before the tier | 89% used, 18G free | 84% used, **25G** free | 89% used, **18G** free |
| `examples` at arrival | 42G | **35G** | **42G** |
| reclaim frees | 33,286 MB | 33,286 MB | 33,288 MB |
| post-reclaim headroom | ~49.6G | ~56.6G | **~49.5G** |
| died after | — | 2 h 07 m | **1 h 59 m** |
| after the tier | 100%, 264K free | 100%, 260K free | 100%, **232K** free |

The reclaim is a constant 33.3 G, so the headroom the tier gets is decided
entirely by the arrival state, and that is what moves: `examples` arrives at 42 G
on two of three runs and the headroom is ~7 G lower than the best case. The
death time tracks it — **1 h 59 m is earlier than the 2 h 06 m–2 h 39 m band**
the earlier sections recorded, so that band is not a floor.

### RETRACTED: that run is not this issue — it is `check-template-copy-out`

The paragraph that stood here said the log contained no failing write and that
the attribution rested on the disk annotation plus the truncation. **Both claims
were wrong, and the second was wrong because of the first.** The failing gate is
in the log, twenty lines above the tail I grepped:

```
===== FAIL (template-copy-out, rc=1, 1555109ms) =====
  multi-package-workspace: FAIL — the copy does not build
```

Run **36466773816** (18:39 push, integration job 18:51:07 → 21:15:08) is the
same failure, `template-copy-out rc=1` with `multi-package-workspace: FAIL`, so
it is two consecutive `host-tests` runs at one named gate on one template, not a
disk death. That belongs to **issue 1453**, where it is now recorded.

What went wrong in the reading: the disk annotation said `100% used, 232K free`,
which is true and is this issue's signature, so I stopped there. But tier 1
builds the world, so reaching 100 % is what a tier-1 run DOES on this runner —
it is a consequence of having run, not evidence of what failed. The grep that
missed it looked for `FAILED:` and `error:`; this gate prints `===== FAIL (` and
`FAIL —`. Two of this issue's three signatures were present and the cause was
still something else, which is the whole point of reading past the step name.

The `1 h 59 m` timing claim in the table above also does not support what I drew
from it. Run 36466773816 arrived in the SAME state (89 % used, 42 G `examples`,
17 G free — marginally worse) and lasted **2 h 24 m**, back inside the
2 h 06 m–2 h 39 m band. So arrival state does not predict death time, and the
sentence claiming the death time tracks it is withdrawn. What survives is the
narrower fact: the reclaim is a constant ~33.3 G, so the headroom the tier gets
is set by the arrival state, and `examples` now arrives at 42 G.

Acceptance unchanged.

## FIRST GREEN: the scheduled `gate` reached a verdict, and the reason is the arrival state (2026-09-29)

Run **36511105083** (scheduled `gate`, 02:07:19Z) is **`success`** — 6 jobs green,
1 skipped. Every step this issue has been waiting on produced a verdict:

| step | |
| --- | --- |
| `just check build` (nightly / manual only) | success |
| `just check no-std` (nightly / manual only) | success |
| `just test-unit` (completes L1) | success |
| `just check workspace-all` (host + embedded clippy) | success |

That is this issue's acceptance for the `gate` arm — *"a scheduled run reaching a
VERDICT on `check build` and `check no-std`"* — met for the first time since the
issue was filed. Acceptance asks for **three nights running**, so this is night
one of three, not closure.

### Why it fit, measured rather than inferred

```
before: 75% used, 38G free — 19G examples; 14G packages/cli/target; 3.5G build
reclaim: freed 6711 MB; 46077760 KB free          (~43.9 G)
after:  89% used, 17G free — 19G examples; 16G target; 15G packages/cli/target
```

The tier spent ~27 G (43.9 → 17) and **finished with 17 G to spare**. Against the
two nights this issue measured before:

| | 2026-09-27 | 2026-09-28 | **2026-09-29** |
| --- | --- | --- | --- |
| before the tier | 86% used, 21G free | 94% used, 9.7G free | **75% used, 38G free** |
| `packages/cli/target` | 30G | 41G | **14G** |
| reclaim frees | 6,703 MB | 6,704 MB | **6,711 MB** |
| post-reclaim headroom | ~27.3G | ~16.2G | **~43.9G** |
| after the tier | 100%, 272K free | 100%, 264K free | **89%, 17G free** |

**The reclaim did not change** — 6,711 MB, within 8 MB of both earlier nights. What
changed is the **arrival state**, and within it one directory:
`packages/cli/target` **41 G → 14 G**, which is what took the starting point from
94 % to 75 % and the headroom from 16.2 G to 43.9 G.

That is the direction the 2026-09-28 section derived from two points — *"the
growth is not spread across the checkout, it is `packages/cli/target`, +11 G in a
day"*, and *"the headroom the tier gets is decided entirely by the arrival
state"*. This night is the first datum where the arrival state moved the other
way, and the tier fit. So the remedy this issue has been waiting for is not a
bigger reclaim: it is whatever keeps that directory from growing, and the
evidence now runs in both directions.

### What this does NOT say

- Nothing about `host-tests`. Its own acceptance (a push run reaching a tier-1
  verdict below 100 %) is untouched, and the five deaths recorded above stand —
  the fifth, run 36496442918, was `check-template-copy-out` and belongs to issue
  1453, not here.
- Nothing about `live-peer`, whose arm still waits on a run whose fixture-build
  step reaches a conclusion.
- Why `packages/cli/target` shrank is NOT established here. A runner with a
  different cache state, a cache eviction, and a deliberate change would all look
  like this from one transcript. Two more green nights would distinguish a fix
  from a lucky arrival; one would not.

## 2026-09-30 — three more, and one of them is the runner's OWN log file

Four failing jobs on main that night, three of them this:

| run | job | evidence |
| --- | --- | --- |
| 36679366972 `host-tests` (push 06:39) | 109771243653 | `Free space left: 0 MB`, one second before the gate's verdict; the failing gate's captured output ends mid-build with no compiler diagnostic |
| 36672424283 `host-tests` (push 05:13) | 109750091198 | `Free space left: 33 MB`; a DIFFERENT first failure (`workspace-features`, not `template-copy-out`) |
| 36668247728 `live-peer regression` (04:18) | 109737476632 | no failing step at all; the job has five steps with a `null` conclusion and an annotation only |

The third is the sharpest instance this issue has. The annotation is:

```
System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_...-utc.log'
  at GitHub.Runner.Worker.Worker.RunAsync(String pipeIn, String pipeOut)
Unhandled exception. System.IO.IOException: No space left on device
  … at GitHub.Runner.Common.HostContext.Dispose()
```

The runner process died writing its own diagnostic log, and then died again in
the handler that was reporting the first death. There is no step log to read —
`.../actions/jobs/<id>/logs` is the two-line `BlobNotFound`, because nothing was
ever uploaded. A job that ends this way reports `failure` with no failing step,
which reads exactly like a cancelled job and nothing like a full disk.

The wider cost is the one this issue exists for, and it is visible in the table:
the two `host-tests` reds have different first failures, so on a disk-starved
runner the lane names whichever gate happened to be running when the space ran
out. Neither red is evidence about the gate it names (see the 2026-09-30 entry
on issue **1453** for that argument in full). A lane in this state has no signal
capacity regardless of what it prints.

## The streak ended at one night, and the fourth datum confirms which variable decides (2026-10-01)

Run **36804891948** (scheduled `gate`, 02:14:31Z, head `e61d7dfd2`), job
**110186978795**. Steps 27 `just check build` and 30 `just check no-std` both
**failed**, steps 31–34 never started, and the job's log is the two-line
`BlobNotFound` because the runner process died:

```
Unhandled exception. System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20261001-021449-utc.log'
```

So the `gate` arm's acceptance — *"a scheduled run reaching a VERDICT on
`check build` and `check no-std`"*, three nights running — stands at **one of
three**, and the streak is broken.

### The transcripts, which both uploaded

```
before: 89% used, 17G free — 33G packages/cli/target; 20G examples; 4.2G build
reclaim: freed 6740 MB; 24597392 KB free          (~23.5 G)
after:  100% used, 256K free — 34G packages/cli/target; 20G examples; 11G target
```

Added to the table this issue has been keeping:

| | 2026-09-27 | 2026-09-28 | 2026-09-29 | **2026-10-01** |
| --- | --- | --- | --- | --- |
| before the tier | 86%, 21G free | 94%, 9.7G free | 75%, 38G free | **89%, 17G free** |
| `packages/cli/target` | 30G | 41G | **14G** | **33G** |
| reclaim frees | 6,703 MB | 6,704 MB | 6,711 MB | **6,740 MB** |
| post-reclaim headroom | ~27.3G | ~16.2G | ~43.9G | **~23.5G** |
| after the tier | 100%, 272K | 100%, 264K | **89%, 17G** | **100%, 256K** |
| verdict | died | died | **success** | **died** |

Two things this fourth point settles, which three points could not.

**The reclaim is a constant.** 6,740 MB, within 37 MB of all three earlier
nights. Four measurements, one number — it is not the variable and no tuning of
it is the remedy.

**`packages/cli/target` is the variable, and it predicts the outcome in both
directions.** 30G → died, 41G → died, **14G → the one success**, 33G → died.
The 2026-09-29 entry derived that direction from a single favourable night and
said explicitly that *"a runner with a different cache state, a cache eviction,
and a deliberate change would all look like this from one transcript"*. This
night is the confirming datum on the other side: the directory came back to 33G
and the tier died again, at the same reclaim, on the same workflow. Nothing was
changed between them, so the green night was an arrival state and not a fix.

**The tier's demand is stable too** — ~23.3G spent here (23.5 → 0.25) against
~27G on the green night. It is the supply that moves.

### What `check build` actually spends it on

The after-transcript names the growth, which no earlier entry here has:

| | before | after |
| --- | ---: | ---: |
| `target/` | 1.1G | **11G** (`target/debug` 1.1→7.2G, `target/nros-relwithdebinfo` 0→3.1G) |
| `build/` | 4.2G | **11G** |
| nine `target-*` siblings | absent | **~5.2G** (`target-embedded` 975M, `target-check-cpp-cyclone-embedded` 829M, `target-check-c` 767M, `target-check-census-hooks` 701M, `target-check-cpp` 602M, `target-check-cpp-clippy-zenoh` 483M, `target-excluded-tests` 332M, …) |

Those per-lane `target-*` directories exist because a cargo `--target-dir`
serves exactly one workspace root (issue 0616), so they are not waste to be
deleted — but they are ~5.2G of the ~23.3G, and they are the part this issue has
never counted.

### What this does NOT say

- Not that `packages/cli/target` is *stale residue*. This is a GitHub-hosted
  `ubuntu-22.04` runner, which arrives clean, so those 33G were built by steps
  7–23 of this same job. It is the lane's own legitimate output, which is why
  the reclaim step does not touch it and should not.
- Nothing about whether step 30 `check no-std` has a defect of its own. Both 27
  and 30 are marked failed and the log is gone, and with the volume at 100% the
  parsimonious reading is one cause; that reading is not a measurement, and only
  a run with headroom can separate them.
- Nothing about `host-tests` or `live-peer`, whose arms are unchanged.

## Tier 1, third measured point: the headroom is the lowest yet and the tipping step MOVED (2026-10-01)

Run **36810256910** (scheduled `host-tests`, 03:23:07Z, head `e61d7dfd2`), job
**110203524815**, step 15 `just ci tier1`, 2 h 21 m. `workspace unit tests`
passed; the integration job died.

```
before: 90% used, 16G free — 43G examples; 14G build; 635M packages/cli/target
reclaim: freed 33336 MB; 50203112 KB free          (~47.9 G)
after:  100% used, 276K free — 43G examples; 20G build; 15G target
```

Against the two points in the 2026-09-28 correction above:

| | 36359825546 (09-27) | 36419496642 (09-28) | **36810256910 (10-01)** |
| --- | --- | --- | --- |
| before the tier | 89%, 18G free | 84%, 25G free | **90%, 16G free** |
| `examples` | 42G | 35G | **43G** |
| `examples/workspaces` | — | 30G → 36G | **38G, unchanged by the run** |
| `packages/cli/target` | 447M | 447M | **635M** |
| reclaim frees | 33,286 MB | 33,286 MB | **33,336 MB** |
| post-reclaim headroom | ~49.6G | ~56.6G | **~47.9G** |
| after the tier | 100%, 264K | 100%, 260K | **100%, 276K** |

Three things this point adds.

**The reclaim is maxed out on this arm, and the headroom is still falling.**
33,336 MB is within 50 MB of both earlier runs, and it is already deleting every
host-tooling directory there is — `/__host-reclaim/android` (11G), `dotnet`
(5.8G), `ghcup` (3.7G), `swift` (3.5G), `powershell` (1.4G), plus `/__t` (5.2G)
and `build/metadata-probe` (2.2G). So unlike the `gate` arm, where the fixed
6.7 G leaves obvious room, **there is nothing left to add to this reclaim**; the
only movable quantity on this lane is what the checkout brings. And that is
moving the wrong way: 47.9 G is the lowest post-reclaim headroom of the three,
below the 09-28 point's 56.6 G, which is the degradation this issue's "the
arrival state is degrading" section named — now with a third point behind it.

**The tipping step is NOT `rust-rtos-link-check` any more.** The 2026-09-28
correction identified it as "where a tier-1 prune should be priced first", from a
run that reached it. In this run the string `rust-rtos-link-check` occurs **zero
times**: the tier died ~6 minutes in, at

```
===== FAIL (workspace-features, rc=101, 361740ms) =====
```

So the lane now runs out before it gets to the step the prune was being priced
against. That does not retract the 09-28 finding — `target-link-check/` really
did grow 7 G in that run — but it does mean the first thing to exhaust the volume
is no longer that step, and a prune aimed only there would not have saved this
run.

**It is this issue and not a clippy defect, by the truncation signature.** The
`workspace-features` gate exits 101 with no diagnostic; its last output line is a
command echo cut off mid-string:

```
cargo clippy --quiet -p nros --no-default-features --features "rmw-cferror: recipe `build` failed on line 233 with exit code 1
```

`--features "rmw-cf` and then the recipe's own failure, spliced together. The job
log contains **zero** occurrences of `error[E`, `could not compile` or
`No space left on device`, and one `Free space left: 82 MB` immediately before,
with the after-report at 276 K. Truncated mid-write with no diagnostic is this
issue's signature, not a lint.

**`packages/cli/target` stays negligible** — 635 M against the `gate` arm's 33 G
the same morning — so the inverse-consumer finding holds: the two arms are
limited by different directories, and `examples/workspaces` at 38 G is this one's.

Acceptance for this arm is unchanged and still unmet: a `host-tests` run reaching
a tier-1 verdict with the after-report below 100%.

### The same morning's `live-peer`, for completeness

Run **36814478655** (04:18): `rows whose board IS this runner` (job
**110216447223**) died with **no failing step**, annotation only —
`Unhandled exception. System.IO.IOException: No space left on device :
'/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20261001-041819-utc.log'`
— the second consecutive night of the runner-process death recorded in the
2026-09-30 section. Its sibling `rows whose board is NOT this runner` failed on
`No SOURCES given to target: app` / `west fixtures: 4/5 ok`, which is issue 1536
and not this one. The `live-peer` arm's acceptance (a run whose fixture-build
step reaches a conclusion) is unmet for a second night.

## Tier 1, fourth point — and a correction to the third (2026-10-01, later)

Run **36920672143** (push `host-tests`, 20:19:01Z, head `d986c270a`), job
**110565630403**, step 15 `just ci tier1`. It ran **2 h 1 m** before finishing,
which is why the 2026-10-01 entry above could not include it.

```
before: 91% used, 14G free — 43G examples; 14G build; 553M packages/cli/target
reclaim: freed 33353 MB; 48745388 KB free          (~46.5 G)
after:  100% used, 272K free — 43G examples; 20G build; 15G packages/cli/target
```

`workspace unit tests` passed; the integration job died on
`===== FAIL (workspace-features, rc=101, 342582ms)` with `Free space left: 28 MB`
one line earlier, and the captured `ci-logs` carry **no compiler diagnostic** —
only `error: recipe tier1 failed with exit code 1`. Same truncation signature,
same tipping gate as the third point.

| | 09-27 | 09-28 | 10-01 (3rd) | **10-01 (4th)** |
| --- | --- | --- | --- | --- |
| before the tier | 89%, 18G | 84%, 25G | 90%, 16G | **91%, 14G** |
| reclaim frees | 33,286 MB | 33,286 MB | 33,336 MB | **33,353 MB** |
| post-reclaim headroom | ~49.6G | ~56.6G | ~47.9G | **~46.5G** |
| after the tier | 100%, 264K | 100%, 260K | 100%, 276K | **100%, 272K** |

The reclaim is a constant across **four** runs now (33,286–33,353 MB, a 67 MB
spread), and the post-reclaim headroom is the lowest of the four — the
degradation the "arrival state is degrading" section named, with a fourth point.

### Correction to the third point

That entry says **"`packages/cli/target` stays negligible — 635 M against the
`gate` arm's 33 G"**. The word *stays* is wrong: 635 M was the **arrival** value,
and this run shows the same directory at **553 M before and 15 G after**. So it
does not stay small on this arm; it starts small and grows ~14 G **during** the
tier. (The third point's own after-summary reads `15G target`, which this run
disambiguates as `15G packages/cli/target` — the two arms were not as cleanly
separated as that entry claimed.)

What survives of the inverse-consumer finding is narrower and still useful: on
the `gate` arm the directory is **already** 30–41 G on arrival and the reclaim
cannot touch it, whereas here it arrives negligible. The growth during the tier
is common to both. A prune aimed at the arrival state therefore helps the `gate`
arm and does nothing for this one.

### What this does not change

Acceptance for this arm is unchanged and still unmet: a `host-tests` run reaching
a tier-1 verdict with the after-report below 100%. Four runs, four times 100%.

## Tier 1, fifth point — the first ENOSPC that names the write, and it is in `/tmp` (2026-10-02)

Run **36933780739** (push `host-tests`, 2026-10-01T22:13:56Z, head `0183a0434`),
job step 15 `just ci tier1`. `workspace unit tests` passed; the integration job
died on `workspace-features` for the third consecutive point.

```
before: 91% used, 14G free — 43G examples; 14G build; 549M packages/cli/target
reclaim: freed 33358 MB; 48573712 KB free          (~46.3 G)
after:  100% used, 560K free — 43G examples; 20G build; 15G packages/cli/target
```

**What is new: the failure is diagnostic rather than truncated.** Every earlier
point in this arm ended in a cut-off log or a bare gate name. This one names the
write, the temp file and the compiland:

```
running: cd ".../target/debug/build/cyclonedds-sys-d6a24355a5931f7a/out/build" && … cmake …
third-party/dds/cyclonedds/src/tools/idlc/src/types.c:696:1:
  fatal error: error writing to /tmp/ccWQgwjh.s: No space left on device
```

Three things follow that the four previous points could not show.

**It is gcc's assembler temporary in `/tmp`, not a cargo or cmake output.** The
transcripts only ever measure `/__w` and the checkout; `/tmp` has never appeared
in them. On this runner the two share one filesystem — `df` reports a single
`/dev/root` at 146 G for both `/__w` and `/` — so filling the workspace starves
the compiler's scratch space as well. Any remedy that counts only build
directories is counting the wrong total.

**The compiland is `cyclonedds-sys`'s build script**, reached from
`workspace-features`, so the tier dies inside the DDS backend's own cmake build
rather than in nano-ros code.

**The reclaim is a constant over five runs** — 33,286 / 33,286 / 33,336 /
33,353 / **33,358 MB**, a 72 MB spread — and all five end at 100%: 264K, 260K,
276K, 272K, **560K** free.

### What this does not change

Acceptance for this arm is unchanged and unmet: a `host-tests` run reaching a
tier-1 verdict with the after-report below 100%. Five runs, five times 100%.

## The runner-log death recurred at the SAME schedule slot, one night later (2026-10-02)

Not a new shape — the 2026-09-30 entry above already measured this one, on this
same lane. What is new is that it happened **again at the same slot**, which
makes it the only failure in this issue with two instances an exact 24 hours
apart:

| date | run | job | lane / slot |
| --- | --- | --- | --- |
| 2026-09-30 | 36668247728 | 109737476632 | `live-peer regression`, 04:18 |
| 2026-10-01 | **36814478655** | **110216447223** | `live-peer regression`, 04:18 |

The 2026-10-01 annotation (head `e61d7dfd2`, created 04:18:17Z):

```
Unhandled exception. System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20261001-041819-utc.log'
```

One number the earlier entry did not have: the filename's timestamp is
`041819`, **two seconds after the run was created**. The worker died writing its
first diagnostic line, before any step of ours ran — so "no failing step and no
log to read" is not an artefact of where it got to, it is because it never
started.

### The known failing writes, consolidated

Worth having in one place now that a third target has an instance, because it
bears on the remedy rather than on the diagnosis:

| where the write failed | first recorded | shape |
| --- | --- | --- |
| build dirs / workspace | the original entry | truncation, `Free space left: N MB` |
| `/home/runner/actions-runner/…/_diag/Worker_*.log` | 2026-09-30 (above) | runner death, annotation only |
| `/tmp/ccWQgwjh.s` (gcc assembler temp) | the tier-1 fifth point, above | named ENOSPC mid-compile |

Only the first is somewhere the reclaim and the before/after transcripts can
see. **A remedy that counts only build output is counting the wrong total** —
the point the fifth point made, now with two targets outside that total rather
than one.

### One run, two findings

The same run's sibling job, `rows whose board is NOT this runner`
(110216752409), also failed, and for an unrelated reason — a west fixture's
configure, filed on issue **1536**. A dead runner and a broken fixture in one
run are two findings; attributing the whole run to this issue would have hidden
the other.

## Tier 1, sixth point — two gates red in ONE job, and only one of them is disk (2026-10-02)

Run **36938229060** (push `host-tests`, 2026-10-01T22:58:53Z, head `2eb977f6f`),
job **110623429320**, step 15 `just ci tier1`. `freed 33358 MB` — the same
reclaim constant as the fifth point, to the megabyte — and then
`Free space left: 0 MB`.

What makes this point worth recording is not the disk number. It is that the
tier failed **two** gates and they have **different causes**:

```
===== FAIL (mem-report, rc=1, 2944ms) =====
AssertionError: STORAGE_ROLES (component storage): …/entry/cpp/entry.cpp.jinja
  no longer contains 'static unsigned char __nros_comp_buf_{{ s.index }}['
===== FAIL (template-copy-out, rc=1, 744378ms) =====
```

`mem-report` is **not** this issue. It answered in **2.9 seconds** with a Python
`AssertionError`, which no amount of free space changes; that is issue 1147's
drift tripwire firing on a template rename, since fixed on `main` by PR #1541.
`template-copy-out` ran for **744 seconds** and died beside `Free space left:
0 MB`, which is this issue.

### Why that matters for reading this lane

The 2026-09-30 entry argued that on a disk-starved runner the lane names
whichever gate happened to be running when the space ran out, and showed it
with two runs naming two different gates. This is the sharper version of the
same point, inside a single job: **a red from this lane is not one finding.**
Here one of the two reds is attributable to a real source defect with a
one-line fix, the other is the disk, and nothing in the step name or the
recipe's exit code separates them — only the elapsed time and the error text
do.

So the triage rule this issue implies is concrete: for every FAIL in a
disk-starved tier-1 job, read its own duration and message before attributing
it here. A gate that answered in seconds with a diagnostic of its own was not
starved of anything.

### What this does not change

Acceptance is unchanged and unmet. Six runs, six times at or near 100 %; the
reclaim has now been within 72 MB of 33.3 G on six consecutive measurements,
and the arrival state is still what decides.

## The scheduled `gate` arm is ATTRIBUTED, and `packages/cli/target` is not the consumer (2026-10-02)

Two complete before/after pairs for the scheduled `gate` lane, both failing at
step 27 `just check build`, both with the runner-death log shape. Earlier
entries called this lane's red undiagnosed because its step log is the two-line
`BlobNotFound`. It was not undiagnosed; it was unread — **the annotation was on
the `check` job all along**, and a previous pass read the aggregator `CI` job's
annotations instead, which say only `Process completed with exit code 1`.

| | 2026-10-01 run **36804891948** job **110186978795** | 2026-10-02 run **36954274069** job **110673704948** |
| --- | --- | --- |
| before | 89 % used, 17 G free | 84 % used, 25 G free |
| `packages/cli/target` | 33 G | 24 G |
| `target/` | 1.1 G | 1.1 G |
| reclaim | freed 6,740 MB → **23.46 G** | freed 6,761 MB → **30.77 G** |
| step 27 | failure | failure (29 min) |
| after | **100 % used, 256K free** | **100 % used, 256K free** |
| `target/` after | **11 G** | **17 G** |
| `packages/cli/target` after | 34 G | 25 G |

Both annotations are the same ENOSPC on the runner's own diagnostic log:

```
Unhandled exception. System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/cached/2.337.0/_diag/Worker_20261002-020914-utc.log'
```

### Three things these pairs settle

**The missing log is a SYMPTOM, not a second fault.** The worker cannot write
its diagnostic log because the filesystem is full, so nothing is uploaded and
the step log reads `BlobNotFound`. Anyone chasing runner infrastructure from
that two-line log is chasing this issue. Note the job did **not** die: steps 28
and 29 ran and uploaded the after-transcript, and step 30 was still going — so
this is a different shape from the `live-peer` instances above, where the job
had no failing step at all. Here the step fails, the job continues, and only the
log is lost.

**`packages/cli/target` is the arrival ballast, not the consumer.** This issue's
gate-arm table has been pairing survival against that directory's size. Across
these two runs it moves by **1 G** (33→34, 24→25) while **`target/` goes 1.1 G →
11 G and 1.1 G → 17 G**. The compile tier's growth is `target/`, `build/`
(4.2 G → 11 G on 10-02) and eight new `target-check-*` directories; the
`packages/cli/target` number predicts how much room is left to start with, which
is not the same claim.

**More headroom did not buy survival — it bought more growth.** 23.46 G
available produced an 11 G `target/`; 30.77 G available produced a 17 G
`target/`. Both ended at exactly `100 % used, 256K free`. Stated as an
observation on n=2, not a law: the alternative is that the two runs simply had
different work to do (the fast line grew from 379 to 382 gates between them, and
`check build` is per-feature and per-example). But it is the first evidence in
this issue that the compile tier may expand to fill what it is given, and that
matters because "raise the headroom" has been the implicit remedy throughout.

### What would close this arm

Unchanged in kind, sharper in target: a scheduled `gate` run whose after-report
is below 100 %. What these pairs add is that the remedy has to bound
`target/`'s growth — a cargo target dir per check lane is the shape that
produced eight extra directories here — rather than only reclaiming more before
the tier starts.

### The tier-1 half of the same distinction: ballast decides, and here it was 458 M

Measured the same day, on the other lane, and it is the first tier-1 run in this
window that did **not** exhaust the disk.

`host-tests` run **36950039916** (push, head `0db0eeb18`), job
**110660733210**, step 15 `just ci tier1`, 02:33:34 → 03:19:12 (**45.6 min**):

```
before: 90% used, 15G free — 43G examples; 13G build; 458M packages/cli/target
reclaim: freed 32532 MB; 48681576 KB free
after:  92% used, 13G free — 43G examples; 19G build; 13G target
```

**13 G still free at the end.** Every other tier-1 point in this issue ends at
0–560 K. The one variable that is different at arrival is the one the section
above says is ballast rather than consumer: `packages/cli/target` is **458 M**
here, against 15 G, 24 G and 33 G in the runs that died. Consumption itself was
ordinary — `build/` 13 G → 19 G and a 13 G `target/`, about 33 G in all, the same
order as the runs that ran out.

So the two halves fit together: **`packages/cli/target` does not consume the
space, it decides how much there is to consume**, and this is the first
measurement where it was small enough for the tier's ordinary appetite to fit.

### What this point does NOT establish

It is not proof that the full tier fits. This run **aborted at 45.6 minutes on a
single gate** — `===== FAIL (mem-report, rc=1, 1586ms)`, which is issue 1147's
drift tripwire, on a head that predates its fix: #1541 merged at 01:21:52 and
this run's head `0db0eeb18` was created at 01:15:15, so it still carries the old
`__nros_comp_buf_{{ s.index }}[` literal. The remaining work — fixtures and the
rest of the matrix — never ran, and that work is where the earlier points spent
their last gigabytes.

What it does establish is narrower and still useful: with negligible arrival
ballast the tier gets **45 minutes and 33 G in without exhausting anything**,
where the 15–33 G arrivals were at 100 % well before finishing. The next tier-1
run that both starts clean and runs to completion is the measurement that would
settle it.
