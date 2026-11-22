---
id: 2840
title: "A tier-1 fixture build needs more than 127 GB on main, so it does not fit a 150 GB gate host"
status: open
type: limitation
severity: medium
area: [ci, fixtures, disk]
related: [issue-2702, issue-2838]
found: 2026-11-22
---

**Home:** [phase-489](../roadmap/phase-489-ci-build-speedup.md), the CI build
cost, alongside its runner-side items.

## What happened

Measuring issue 2702's per-image configures needed a full
`just build-test-fixtures tier1`, so it ran on newslab-243 under the host
lock, stopped when free space reached 50 GB. Both trees hit that floor in
the linux stage, before the lane finished:

| tree | start free | stopped after | peak use |
|---|---|---|---|
| main (`56f71a03ec`) | 174 GB | 2,160 s | ≥ 127 GB |
| issue 2702's branch, with its cargo prune | 117 GB | 1,530 s | ≥ 68 GB |

Where the disk sat when each run stopped, in GB:

| tree | main | branch |
|---|---|---|
| `build/zephyr-workspace-builds` | 31.8 | 31.9 |
| `build/sizes-probe` | 17.9 (109 dirs) | 24.8 (151 dirs) |
| `build/corrosion-cargo` | 7.7 | 7.7 |
| `build/metadata-probe` | 2.0 | 4.2 |
| `build/cargo-fixtures` | 2.3 | 2.3 |
| workspace cmake configures (`examples/**/build/<coordinate>`) | 31.0 (61 posix) | 10.8 (120 posix) |

So the lane does not fit a 150 GB host on main, independent of issue 2702,
which makes it ~13 GB lighter at the same point. A gate host is sized for
`just ci gate`, which builds no fixtures, so the gate is not affected. Any host
or runner expected to build the tier-1 fixtures is.

## Candidate mitigations

Ranked by nros 142 (2026-11-22), not yet measured:

1. **Extend the lane's cargo prune** to the Corrosion trees
   (`build/corrosion-cargo`) and the probe trees (`build/sizes-probe`,
   `build/metadata-probe`): their cargo `deps/` and `incremental/` once their
   artifacts are out. Issue 2702's `nros_prune_cargo_intermediates` does this
   for a workspace configure: 1,329 MB to 241 MB a configure, with a
   byte-identical rebuild.
2. **Not the Zephyr west build dirs.** The staleness probes read them, and a
   tree that lost them could not recover (issue 0834's shape). They stay.
3. **Split the lane**, building, testing and pruning one family at a time, only
   if the prune leaves the peak above ~100 GB.

Re-keying the sizes probe on its size-relevant fields only (issue 2838) cuts
the probe tree at its source.

Acceptance: a tier-1 fixture build measured end to end, under the host lock,
with its peak below the gate host's free-space floor.
