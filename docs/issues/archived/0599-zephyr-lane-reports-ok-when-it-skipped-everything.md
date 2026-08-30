---
id: 599
title: The Zephyr fixture lane exits 0 when the workspace is missing, so it reports OK and the failure surfaces 20 minutes later as four missing `.inputsig`
status: resolved
type: bug
area: testing
related: [issue-0196, issue-0482, phase-350, rfc-0061]
---

## What happens

On a host with no provisioned Zephyr workspace, `just build-test-fixtures`
prints:

```
== zephyr ==
== zephyr == OK
```

and its own lane log says:

```
Zephyr skip: zephyr-workspace not set up (run `just zephyr setup`)
```

`just/zephyr-ci.just:32` prints that line and then **`exit 0`** (the sibling at
`:28` does the same for a missing `west`). The driver sees rc=0 and records the
lane as OK, so the summary a reader trusts says the lane succeeded when it built
nothing.

The consequence arrives much later, in a different recipe, naming different
objects:

```
ERROR: 4 compile-check fixture(s) are missing or stale:
  zephyr_self_pkg_rust     (missing build/compile-check-fixtures/zephyr_self_pkg_rust/.inputsig)
  west_board_import        (missing …/west_board_import/.inputsig)
  zephyr_self_pkg_sibling  (missing …/zephyr_self_pkg_sibling/.inputsig)
  west_bringup_zephyr      (missing …/west_bringup_zephyr/.inputsig)
  Run `just build-test-fixtures` before test-all.
error: Recipe `_lane-gate` failed with exit code 1
error: Recipe `ci-matrix` failed with exit code 1
```

Those four are owned by the west lane by design —
`examples/fixtures.toml:4553`: *"Built by the WEST lane (west-fixtures.sh),
never by compile-check-fixtures.sh: west needs a provisioned Zephyr workspace,
so the lane that owns one runs them."* They are also unattributable to a
coordinate, so by the phase-340 W3 rule they are **never skipped** by lane
narrowing: every run scope requires them.

So the chain is: precondition missing → lane skips silently → lane reports OK →
20 minutes of other lanes build → the gate fails naming an artifact, not the
precondition. The remedy it prints (`just build-test-fixtures`) is the command
that just "succeeded".

## Why it is a bug and not just an unprovisioned host

The unprovisioned host is legitimate — not everyone has a Zephyr SDK, which is
exactly why the skip exists. What is wrong is that **the skip is invisible at
the level where the decision is made**, and the report is affirmatively wrong:
`OK` is not "skipped".

`just check tier-preconditions` — which exists to report *every* unmet
precondition before committing to a run — does not mention it either. It checks
the CLI stamp, leaf syncs, build sources and fixture coverage, and reports the
tier as runnable.

This is the issue-0196 shape (a gate whose coverage is narrower than the rule it
enforces) applied to a lane's success report, and it is close kin to issue 0482:
the build side and the run side disagree about which fixtures exist, and neither
says so at the point of disagreement.

## Evidence

Observed 2026-08-15 on a tree with no `zephyr-workspace`:

* `tmp/build-test-fixtures-<stamp>/zephyr.log` — one line, the skip.
* Lane summary — `== zephyr == OK`, alongside seven genuinely-built lanes.
* `just ci-matrix` — dies in `_lane-gate` on the four `.inputsig` files above.
* `just check tier-preconditions` — reports no Zephyr-related problem.

Reproduce by moving or not creating `zephyr-workspace` and running
`just build-test-fixtures lane=tier2`.

## Direction

1. **Distinguish SKIPPED from OK in the lane report.** The driver already
   distinguishes OK from FAILED; a third verdict costs one exit code (or a
   marker file the driver reads) and makes the summary honest. `== zephyr ==
   SKIPPED (workspace not provisioned)` would have ended this in one line.
2. **Have `check-tier-preconditions` name it.** It is precisely the "report
   every unmet precondition at once, before the run" recipe (issue 0466). A lane
   that cannot run for a tier the caller asked for belongs in that list.
3. **Make the downstream error name the cause.** `_lane-gate` knows the four
   fixtures are west-owned; when the west lane did not run, it should say the
   workspace is unprovisioned rather than that an `.inputsig` is missing.
4. Both `exit 0` sites in `just/zephyr-ci.just` (west missing, workspace
   missing) are the same rule and must move together — fixing one leaves the
   other reporting OK, which is how this class recurs.

## Impact

Tier 2 and tier 3 cannot complete on a host without a provisioned Zephyr
workspace, which is correct — but the operator learns it as an artifact error
after a full fixture build rather than as a precondition beforehand. Every
agent session on such a host pays that round trip once.


## Resolved 2026-08-17

Directions (1), (2) and (4) had already landed: `nros_lane_skip`
(`scripts/build/lane-skip.sh`) gives SKIPPED its own verdict at BOTH `exit 0`
sites, and `check-tier-preconditions` warns that the lane will skip and names
the four fixtures.

**(3) was still open** — the downstream error named the artifact and prescribed
`just build-test-fixtures`, the command that had just reported OK. It now names
the cause:

```
ERROR: 4 compile-check fixture(s) are missing or stale:
  west_board_import (missing …/.inputsig)
  …
  CAUSE: no provisioned Zephyr workspace, so the west lane SKIPPED.
  These are built by that lane and by no other, and they are
  unattributable to a coordinate, so every run scope requires them:
    west_board_import
  Re-running the build below will skip the lane again and report OK.
  Provision first:  just zephyr setup
```

The west-owned ids are DERIVED from `examples/fixtures.toml`
(`[[compile_check_fixture]]` with a `west-*` builder), not hand-listed — a
copied list is exactly what goes stale when a row is added, and this file would
have no way to notice.

Verified in all three branches, since two of them are about staying QUIET:

| condition | behaviour |
| --- | --- |
| west row missing, no workspace | names the cause |
| non-west row missing, no workspace | silent — unrelated failure |
| west row missing, workspace PRESENT | silent — a real build failure must not be blamed on provisioning |

### A defect in the derivation, found by running it

The first cut emitted **58 ids that do not exist**. `id` and `builder` keys
appear in `[[fixture]]` and `[[workspace_fixture]]` blocks too, so without an
in-block flag an id from one block paired with a builder from another. The
awk now tracks which block it is inside. (A prior cut also used a greedy
`gsub(/.*"|".*/)`, which ate the whole line and emitted nothing at all.)
