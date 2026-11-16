---
id: 2661
title: "runner-sweep counts a hard-linked file once per eviction unit, so it stops evicting before the area is really under budget, and says nothing"
status: open
type: bug
severity: medium
area: [ci, runner]
related: [phase-559, issue-1094, issue-1353]
found: 2026-11-17
---

**Home:** [phase-559](../roadmap/phase-559-ci-workflow-organization.md)
(runner disk). Owner: nros 175.

## What happens

`_evict` in `scripts/ci/runner-sweep.sh` measures an area's `total` once, then
walks its units oldest first, adding each unit's own `du` to `freed`, and stops
when `total - freed <= budget`.

`du` counts a hard-linked inode once per invocation. The area total counts a
file shared by several units once, but each unit's `du` counts it again. Every
unit that shares the file adds its size to `freed`, so `freed` passes the real
reclaim.

West workspaces are hard-linked (`cp -al`; CLAUDE.md's Zephyr pitfall), and so
are the zephyr build trees beside them, so this is the common case on a runner:

- A real sweep stops early. It believes it reached budget, but freed fewer real
  bytes than it counted.
- The over-budget warning ("still over budget after evicting everything
  evictable") prints only when the unit list runs out, so it does not fire here
  either. The disk stays full with no line saying so.

## Measured

A decoy of 10 build dirs sharing one 200 MiB hard-linked file, reached through
`NROS_ZEPHYR_BUILD_ROOT`, made main's `check runner-sweep-eviction-order` red
in 3 of 21 checks. The budget-0 loop stopped before reaching the sandbox's
newest units. That was a leak of the host's root into the selftest, fixed
separately (the selftest now clears the inherited environment). This issue is
the sweep defect the leak exposed.

## Fix

- After each eviction, re-read the area's `du` total and stop on the measured
  value. That is one source of truth, and it is robust to hard links.
- `--check` deletes nothing, so it has no total to re-read. There, measure
  `du` over the units not yet proposed, in ONE call. A file still linked from a
  kept unit is not freed, and one `du` counts it once, so this is also the real
  remaining total.
- If that measures too slow on a large area, fall back to tracking freed bytes
  by inode. **Measured (2026-11-17), not needed:** `--check --disk` at a 0
  budget over a real 68.2 GiB fixtures area (a gated worktree's `build/` plus
  its `examples/**/build`, 76 units) took 20.7 s, against 5.8 s for the
  per-unit sum it replaces. The sweep runs once per job, so the measured
  remainder stays.
- After the loop, warn whenever the measured total is still over budget, not
  only when the list ran out.
- Test: a sweep test whose units share hard-linked files, asserting the real
  remaining total and the warning.
