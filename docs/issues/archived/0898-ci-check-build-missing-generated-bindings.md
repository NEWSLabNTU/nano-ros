---
id: 898
title: "`check-build` asserts generated message bindings that no CI job produces
  — the second half of the tier that could never pass"
status: resolved
type: bug
area: ci, build
related: [issue-0871, phase-396]
resolved_in: "issue 0898"
---

## The half that was left unowned

phase-396 W1 took the build tier off the merge group because it was NOT RUNNABLE
there, and named exactly two missing artifacts:

    native check: 40 example(s) are missing their generated message bindings
    BuildFailed("Test fixture binary not prebuilt: …/platform_hdr_posix_cpp_heap/.compile-ok")

and said plainly what restoring it needs: "giving the job its artifacts FIRST
(PR #14 adds build-compile-check-fixtures; **the bindings half is unowned**)".
Issue 0871 did the fixture half. This is the bindings half.

## Why the gate is right and the job was wrong

Example msg crates (`generated/<msg>/`) are build-time AND ROS-version-dependent,
so they are gitignored and never shipped in git. `just check` deliberately
refuses to materialise them — that would couple a pure static check to a live
ROS environment and silently rewrite each example's `[patch.crates-io]` block —
so it GATES instead, with one clear error naming the remedy
(`just generate-bindings`) rather than a cryptic per-example cargo failure deep
in parallel output.

That design is sound. The defect was only that no CI job ever ran the remedy,
so a required-adjacent tier asserted an artifact nothing produced.

## Fix

`just generate-bindings` as a step ahead of `check-build`, on the same
`schedule`/`workflow_dispatch` condition the tier itself now uses — so it never
runs on the cheap pull-request path.

Affordable because the CI image already has ROS (`nano-ros-ci:humble`).

## Measured

* `just generate-bindings` on a humble host: **rc=0, 23 s** for the whole tree,
  against the ~587 s tier it unblocks.
* `just native check` afterwards: the bindings precondition reports **0**
  failures and the run proceeds into per-example clippy — i.e. it gets past the
  gate that previously stopped it dead.

## What remains, and is NOT this

Restoring `check-build` to the merge group is a separate decision with its own
cost argument (phase-396 W1 moved it off deliberately, and post-submit already
catches compile-tier breaks on `main` minutes later). This issue only makes the
tier RUNNABLE where it still runs — nightly and manual. Whether it should also
gate merges again is phase-396's call, not a side effect of this.
