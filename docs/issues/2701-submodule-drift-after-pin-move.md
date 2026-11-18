---
id: 2701
title: "A submodule left off its pin after a rebase crashes check-book-probe-tracks, and the lanes agents run never name the drift"
status: open
type: bug
severity: medium
area: [testing, build]
related: [550, 1513, 1758, 2306]
found: 2026-11-18
---

## What happens

Two parts with one root cause.

**1. A gate crashes instead of reporting.** `fast::check::book-probe-tracks`
raised a traceback from its non-tip rehearsal (phase-567 W3c, added by #4180)
when a submodule's checkout differed from its recorded pin:

    IsADirectoryError: [Errno 21] Is a directory: '/tmp/…/repo/packages/rmw/zenoh/zpico-sys/zenoh-pico'

`non_tip_rehearsal()` overlays every path from `git diff --name-only HEAD`
onto a depth-1 clone of HEAD. A drifted submodule shows up in that list as its
gitlink path, which is a directory, and the clone holds it as an empty
directory. The overlay found no file at the source and called `unlink()` on
the clone's directory. NXP 175 hit this on W13's gate (red #2). It reproduces
on any worktree: move `packages/rmw/zenoh/zpico-sys/zenoh-pico` to `HEAD~1`
and run `python3 scripts/check-book-probe-tracks.py`.

**2. Nothing in the agents' path refuses, or even prominently names, the
drift.** A rebase across a pin move updates the gitlink and leaves an
already-checked-out submodule where it was. `just setup-worktree` deliberately
updates only UNINITIALISED submodules. For one that is checked out off its pin
it prints

    [KEPT]    <path> is checked out at a commit other than its pin (behind:…).
              NOT updated - that would discard work in it (AGENTS.md).
              When you are sure it holds nothing you need:
                  git submodule update <path>

and then exits 0. `scripts/check-submodule-drift.sh` (issue 0550) classifies
exactly this case (behind or diverged is a FAIL, ahead is OK because that is the
vendored-fork workflow), but its callers are `just doctor` and
`check tier-preconditions` (the head of `just ci`). Neither `just ci gate`'s
dev-lane preflight nor `just check fast` (the pre-push hook) runs it. So the
first thing an agent sees after a rebase across a pin move is whichever gate
trips on the old submodule tree, and the message names a file or a traceback,
never the submodule. Sessions 133 and 175 both hit this on 2026-11-18.

## Fix

- **Part 1 (done in the PR that files this):** `edited_paths()` uses
  `git diff --name-only --ignore-submodules=all HEAD`, so a gitlink is never an
  overlay path. `overlay()` also skips any path that is a directory on either
  side, so it never unlinks one. A self-test builds a superproject whose
  submodule's checkout has drifted from its pin, asserts that the plain diff DOES
  list the gitlink (the negative control), asserts that `edited_paths` drops it,
  and asserts that `overlay` given the unfiltered list neither raises nor removes
  the gitlink directory. Mutation-checked: dropping `--ignore-submodules=all`
  turns the self-test red.

- **Part 2 (open, proposed):** REFUSE, never update. The AGENTS.md rule stands:
  updating discards work someone may be in the middle of in that submodule.
  1. The lane preflight (`scripts/check/gate-lane-preflight.py`) runs
     `check-submodule-drift.sh` for every lane that claims `submodules`, and
     reports a BEHIND or DIVERGED submodule as an unmet claim with the remedy
     `git submodule update <path>`. That reaches `just ci gate` and
     `just check fast` before any gate runs, and the message names the
     submodule and the command.
  2. `setup-worktree` exits non-zero on a `[KEPT]` row that is behind or
     diverged. An AHEAD row stays OK: that is the vendored-fork workflow
     mid-flight.
  Open question for the reviewer: should the pre-push hook refuse on BEHIND? A
  push with a stale submodule checkout is legal, because the gitlink, not the
  checkout, is what gets pushed. The hook runs the fast lane, so (1) would
  make it refuse. The alternative is to scope the drift claim to the `dev` lane
  only.
