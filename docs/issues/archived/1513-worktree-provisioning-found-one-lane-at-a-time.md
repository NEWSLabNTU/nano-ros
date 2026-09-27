---
id: 1513
title: "A fresh checkout's provisioning gaps are found ONE LANE RUN AT A TIME, and
  `setup-worktree`'s authored list had drifted two submodules behind the gates"
status: resolved
type: bug
area: [ci, build, tooling]
severity: medium
found: 2026-09-27
related: [1487, 1043, 0463, 0196]
---

## What was wrong

Running `just ci gate` in a fresh agent worktree cost **three lane runs to learn
three facts**, each knowable in a second:

* run 1 — `check::fast` red: `capability-conditionals` + `xrce-vendored-versions`
  (`zenoh-pico`, `micro-cdr`, `micro-xrce-dds-client` not checked out);
* run 2 — `check::build` red, 6 of 24 gates: `template-copy-out` and
  `scaffold-builds` (the `nros` on PATH belonged to the PARENT checkout),
  `cli-tests` (no `nros-launch-resolve`), `test-targets`, `workspace-all` and
  `workspace-features` (`third-party/dds/cyclonedds` unprovisioned);
* run 3 — the actual verdict.

Measured across **four agent sessions in one day**, each paying it separately.

`just setup-worktree` already existed and nothing pointed a new session at it
before the first run — and it would not have been enough anyway.

## The drift, and it was self-documented

`setup-worktree` carried an AUTHORED array described as *"the submodules the fast
gate tier reads"*, with the note *"When a gate grows a dependency on a fourth one,
it belongs in this list."* Gates then grew dependencies on a fourth and a fifth —
`third-party/dds/cyclonedds` and `packages/cli/third-party/play_launch`, both in
`check::build` — and nobody added them. The comment predicted its own failure.

## The fix

**One list, as DATA:** `.config/worktree-provisioning.txt`, five rows, each
carrying the gate that reads it. Both consumers read it — `setup-worktree`
(checks them out) and `_require-worktree-provisioning` (reports what is missing).
There is no second copy to drift.

**A preflight at the head of `just ci gate`** that reports EVERY unmet
precondition at once — the five submodules and both binaries — with the remedies
in order. Measured on a fresh clone with 20 uninitialised submodules: **7
preconditions in one report**, against the three lane runs it replaces.

It **fails** rather than warns, unlike `check-tier-preconditions`, and the
distinction is not stylistic: every item here is one this lane needs, so warning
only moves the same failure later. `tier-preconditions` reports and continues
because it covers items a given lane may not need.

**In the recipe body, not in `steps`.** A non-`check::` step of `ci gate` must be
reached by a workflow `run:` block (`check-default-gates-run-somewhere`, issue
1226) — and no workflow should run a provisioning preflight, because CI
provisions explicitly and is not a worktree. Same position
`check tier-preconditions` takes at the head of `just ci`. This is issue 1487's
lesson applied before the fact: that gate went red on its own PR because it
asserted a developer-tree invariant somewhere CI restores state from a cache.

Not worktree-specific despite the remedy's name: a fresh CLONE reaches the same
state, and the check asks "is it provisioned", never "am I a worktree".

## Verified

Both failure arms driven, on a genuinely unprovisioned `git clone`:

* an uninitialised submodule is reported (5 of them, plus both binaries = 7);
* a row naming no submodule of this checkout is reported rather than skipped —
  that is how a RENAMED submodule is caught instead of silently dropping its
  gate's dependency;
* the provisioned main checkout reports OK.

`check-gate-lists` and `check-default-gates-run-somewhere` both OK, still 2
non-`check::` steps, so the preflight demands no workflow.

## One defect found in writing it

An earlier draft of `setup-worktree`'s closing message contained
`` `just ci gate` `` in an `echo`. `just` treats a backtick in a recipe line as
COMMAND SUBSTITUTION, so printing the message **executed the gate**. The strings
carry no backticks now, with that reason beside them — the same class CLAUDE.md
records for column-0 lines and heredocs in `just` recipes.

## Not fixed here

`just format` is unrunnable in a fresh worktree at all: `_require-leaf-includes`
refuses until `nros sync` has run (issue 0463's guard). Reported by the same
sessions; a separate question, since the guard is correct and the remedy is a
different one.
