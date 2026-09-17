---
id: 1382
title: "`check-compile-smoke` builds two feature shapes that write ONE shared
  sizes header, so whichever runs second aborts — and deleting the header only
  swaps which one loses"
status: open
type: bug
area: [build, ci]
severity: medium
found: 2026-09-17
related: [issue-0834, issue-0088, issue-1260, issue-0245, issue-0268]
---

## What happens

`just check compile-smoke` — a REQUIRED `CI` context on every pull request —
aborts in `nros-cpp`'s build script:

```
nros-cpp: .../target/nros-c-generated/nros/nros_config_generated.h was written
by another crate with DIFFERENT probed sizes.
  on disk: .../build/sizes-probe/rustc-.../935b8b7de9912b03/.../libnros.rlib
  current: .../build/sizes-probe/rustc-.../6481b7fa738ff254/.../libnros.rlib
Disagreeing defines:
  EXECUTOR_OPAQUE_U64S:         on-disk=11306 vs would-write=11301
  NROS_EXECUTOR_MAIN_STACK_MIN: on-disk=3792  vs would-write=3712
  NROS_EXECUTOR_SIZE:           on-disk=90448 vs would-write=90408
  NROS_EXECUTOR_STORAGE_SIZE:   on-disk=90448 vs would-write=90408
  NROS_EXECUTOR_VALUE_SIZE:     on-disk=1896  vs would-write=1856
```

## Why it cannot converge

The lane's own success line says what it builds: *"nros-c + nros-cpp check
clean in the shipped shape (std,rmw-cffi,platform-posix,ros-humble), **and with
param-services** (issue 1260)"*. Two feature shapes. `nros-sizes-build` keys
its probe directory by `(rustc, target, features)` — correctly, so the two
shapes get two probe dirs, `935b8b7de9912b03` and `6481b7fa738ff254`, both
created within the SAME run. They legitimately probe different executor
layouts, 40 bytes apart.

But the header they emit is **one shared path**,
`target/nros-c-generated/nros/nros_config_generated.h`, and the guard at
`packages/tooling/nros-build-helpers/src/shared.rs:662` refuses a second write
that disagrees. So the first shape writes and the second aborts, whichever
order they run in.

**Measured, and this is the part that rules out a poisoned cache:** delete only
the contested header and its `.stamp` and re-run, and the numbers SWAP —

```
EXECUTOR_OPAQUE_U64S: on-disk=11301 vs would-write=11306   (was 11306 vs 11301)
```

— i.e. the other shape got there first this time and the first one now loses.
Neither ordering converges. That is one step past issue 0834: 0834's mirror
reached a state no re-run repaired because of a stamp; this one is two
producers with genuinely different correct answers pointed at one file.

## What is NOT established

Why it is not red on every PR. It passed once in this same worktree earlier the
same evening and then failed on every subsequent run, so something about cargo
freshness decides whether the second shape's build script re-runs at all — a
shape whose script does not re-run does not write, and the lane is green. That
makes this an INTERMITTENT required check, which is the worse failure mode:
a green run does not mean the two shapes agree, only that one of them was
cached.

Reproduced at `origin/main` (f62c17359) with nothing else checked out, so it is
not a property of any branch.

## Direction

The header path has to carry the same key the probe directory does — one
generated header per `(rustc, target, features)` shape, with the consumer
picking the one matching its own features — or the lane has to stop building
two shapes into one `--target-dir`. The second is the smaller change and
matches issue 0616's rule one level down: *a cargo `--target-dir` serves
exactly ONE workspace root*, and by the same argument a generated-header mirror
serves exactly one feature shape.

Whatever the fix, the acceptance is the SWAP above: delete the header, run
twice, and neither run may abort.

Found while running the required PR gates for the phase-392 amendment B
measurement wave.
