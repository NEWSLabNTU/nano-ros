---
id: 2905
title: "live-peer's host job builds about 141 fixture rows for 41 cells, each from scratch, and now ends minutes under GitHub's hosted-job cap"
status: resolved
resolved: 2026-11-27
resolved_in: "#4715 (fixes 1 and 2) and #4781 (the shards, phase-591 W3)"
type: bug
area: [ci, testing]
severity: high
found: 2026-11-23
related: [phase-590, 2889, 1846, 2130]
---

## What fails

`live-peer.yml`'s `regression` job ("rows whose board IS this runner") runs on a GitHub-hosted
`ubuntu-22.04` runner. GitHub ends a hosted job at 360 min, and the job's `timeout-minutes` is
350. Its step "Build the fixtures those rows resolve" now takes most of that:

| Run | Date | Builds | Median per build | Step total | Result |
| --- | --- | --- | --- | --- | --- |
| 44235514551 | 11-18 | 132 | 0.86 min | 143 min | green |
| 44456527279 | 11-19 | 138 | 1.28 min | 261 min | green |
| 44672701128 | 11-20 | 137 | 0.98 min | 215 min | green |
| 44864729982 | 11-21 | 137 | 0.60 min | 145 min | cells red (the `service list` ordering defect, fixed in #4718) |
| 45148744207 | 11-23 (dispatch) | 149 | 1.33 min | 309 min | cells red; job ended about 13 min under the cap |

Builds are counted between the step's `disk: … free` markers (issue 1846). The row count
barely changes from run to run (`ubuntu22/20261111.360` on both 11-18 and 11-19, both at
`-j8`), yet the step time nearly doubled from 11-18 to 11-19. Most of that swing is the
prepass below, not the runner. The comment above
`timeout-minutes` ("longest green 177.7 min") is out of date: 11-19 took 282 min end to end.
A step this close to a fixed cap fails on any slow day, and `release-lanes-green`
has no flake allowance, so this blocks a release ([phase-590](../../roadmap/phase-590-nros12-release-plan.md) B16).

## Cause

Measured by nros 118:

- **A prepass that never converges.** Each row runs a bare `nros configure` first, which
  configures every image in the workspace. The hosted runner has no Zephyr SDK, so the Zephyr
  images never stamp, and the prepass runs again for the next row. It cost 39.3, 121.2 and
  105.9 min on 11-18, 11-19 and 11-20: most of the 11-19 doubling.
- **Too many rows.** `check-interop-verdicts.py --host-workspace-fixtures` returns 141 rows
  for 41 host cells. Only 14 come from fixture names the tests use. 80 come from
  `_linux_migrated_leaf_rows`, selected by coordinate, and 74 of those are reached by nothing
  else.
- **No sharing.** Every row builds from scratch: `Compiling nros-node` appears 124 to 163
  times in each job's log.

## Fix

In order, without raising the cap first (raising it only moves the cliff):

1. Configure only the image a row builds, so the prepass does not re-run per row.
2. Name the rows by test id, with a self-test that pins the count, so the job builds what its
   cells resolve. 118 measures this as about 60% fewer rows.
3. Share a cargo target group per coordinate across the remaining rows (phase-340), so the core
   chain builds a handful of times instead of once a row.
4. Then reconsider a cache or a self-hosted runner, and re-derive `timeout-minutes` from
   measured runs.

#4715 carries fixes 1 and 2 (two commits); 118 expects the fixture loop to fall from about
226 min to 70 to 90 min.

## Acceptance

Two scheduled `live-peer` runs in a row where the `regression` job's fixture step finishes in
under half the cap, and the step's build count matches the self-test's pinned count.

## Resolution (2026-11-27)

Fixes 1 and 2 landed in #4715 (2026-11-23). The host fixture rows then moved
onto four parallel shards plus one cells job in #4781 (phase-591 W3,
2026-11-24), which takes the place of fix 3's sharing for the wall clock. The
acceptance is met on two consecutive scheduled runs:

| run | shard fixture steps (min) | cells job's own build step | units per run | workspace rows (shards) |
| --- | --- | --- | --- | --- |
| 45545116210 (11-25) | 46.0, 49.7, 43.0, 45.0 | 2.7 min | 105 | 26 + 25 + 25 + 21 = **97** |
| 45753606904 (11-26) | 33.3, 47.8, 48.6, 36.5 | 2.2 min | 105 | 25 + 21 + 26 + 25 = **97** |

- **The time bar:** every fixture step is far under half the 350-min cap,
  against 143 to 309 min single-job before.
- **The count bar:** the build count matches the self-test's count. 97
  workspace rows is the count `check-interop-verdicts.py` recorded when it
  pinned the derivation (97 on 2026-11-23, ceiling
  `HOST_WORKSPACE_ROWS_CEILING = 100`), against 141 under the coordinate
  rule.
- **Both runs green** end to end in 85 and 86 min.

Fix 4 (a cache or a self-hosted runner) is not needed at these numbers.

