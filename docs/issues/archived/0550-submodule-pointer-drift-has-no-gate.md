---
id: 550
title: "A submodule left BEHIND the recorded pointer had no gate — a stale cyclonedds took the fixture sweep down 17 leaves in"
status: resolved
resolved_in: issue-0550
type: bug
area: build
related: [issue-0528, issue-0548, issue-0466, rfc-0061]
---

## Symptom

`just build-test-fixtures` (lane=all), leaf 17, `build-rs-action-server-cyclonedds`:

```
CMake Error at <repo>/zephyr/cmake/modules/extensions.cmake:428 (add_library):
  Cannot find source file:
    <repo>/third-party/dds/cyclonedds/src/ddsrt/src/sync/zephyr/sync.c
Call Stack: <repo>/zephyr/CMakeLists.txt:65 (zephyr_library_named)
```

The zephyr module is an order-only prerequisite of every other platform, so
this takes the whole sweep down — same blast radius as 0528 and 0548, which is
why it read as "the next code bug in the lane."

## Cause

It was not a code bug at all. `third-party/dds/cyclonedds` was checked out at
`6eb9227`; the superproject records `8601ca6` — 7 commits ahead, including:

```
a09babf ddsrt: Zephyr-native sync backend — k_mutex/k_condvar instead of pooled pthreads
```

`a09babf` is what ADDS `src/ddsrt/src/sync/zephyr/`. The in-tree half of that
change (`nros_rmw_cyclonedds.cmake` swapping the TU under `DDSRT_WITH_ZEPHYR`)
was current, so cmake named a file the stale vendored half does not carry.

`8601ca6..6eb9227` was empty and the submodule worktree clean — no local work,
a strict ancestor. A plain `git submodule update third-party/dds/cyclonedds`
fixed it, and the leaf then built to `zephyr.elf`.

CLAUDE.md already states the rule twice — "both halves move together or the
layouts disagree" for this exact pair, and "never leave a submodule at an older
local commit when the remote pointer advanced." Neither helps at diagnosis
time, because the symptom names a FILE. Nothing connects the missing file to
the pull that moved the pointer.

## Why no existing gate caught it

`just doctor` and `check-tier-preconditions` between them cover the CLI source
stamp, leaf `nros sync`, vendored build sources and per-lane fixture freshness.
None looks at submodule pointers. Nor could `check-fast`: drift is a
WORKING-COPY state — the index and the commit always agree in anything you can
push, so a source gate can never observe it. It is a tier PRECONDITION.

The cost profile matches issue 0466 exactly: invisible until the previous stop
cleared, and ~17 leaves of build time to surface.

## Fix

`scripts/check-submodule-drift.sh`, run as the FIRST item of
`check-tier-preconditions.sh`. `git submodule status | grep '^+'` is the whole
detection; what the script adds is direction, because the direction picks the
remedy:

* **behind** (checked-out is an ancestor of recorded) — FAIL. `git submodule
  update <path>` is a fast-forward, no local work at risk. The script prints
  the missing commits, so `a09babf` names itself.
* **ahead** (recorded is an ancestor of checked-out) — OK, with a note. This is
  the normal middle of CLAUDE.md's vendored-fork workflow: the agent commits and
  rebases locally, the maintainer pushes, and only then does the superproject
  pointer move. Failing here would flag every in-flight fork fix as broken.
* **diverged** — FAIL, and the remedy is a REBASE, not an update: `git submodule
  update` checks out the recorded commit detached and leaves the local commits
  unreferenced.

Uninitialized submodules (`-` prefix) are not drift and are not reported — px4,
play_launch's layer-3 runtime submodules and the nuttx tree are all deliberately
absent until a recipe inits them.

It runs FIRST, ahead of the CLI stamp, because its remedy rewrites source
mtimes: `git submodule update` re-arms the CLI's stamp and every fixture, so
clearing it after those would invalidate the work just done. The
"Order matters" footer now says so.

## Acceptance

* `bash scripts/check-submodule-drift.sh` passes on a matched tree.
* Checking cyclonedds back to `6eb9227` makes it fail, naming the 7 commits and
  the fast-forward remedy; `git submodule update` clears it. (Verified
  2026-08-13, both directions.)
* `just build-test-fixtures` gets past `build-rs-action-server-cyclonedds`.
  (Verified via `just zephyr build-one rust/action-server cyclonedds` →
  `Built: .../build-rust-action-server-cyclonedds/zephyr/zephyr.elf`.)
