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
