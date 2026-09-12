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
