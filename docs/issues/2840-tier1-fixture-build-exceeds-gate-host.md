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

## Measured end to end, 2026-11-24

nros 142 ran `just build-test-fixtures lane=tier1` on a clean worktree at
main `fb2c34e379` (which has issue B22's census-prepass fix, #4739),
aarch64, host 142. It finished, rc 0, in 6,800 s (1 h 53 min). `df` was
sampled every 10 s and `du` every 60 s. An earlier attempt at `6a86f706fe`
died in the census prepass after 1,957 s at a lower bound of 14.0 GiB, and
is superseded.

**Peak: +177 GiB of worktree** (`du` 4.2 to 182.1 GiB). The filesystem delta
was 181.2 GiB, including a little other activity on the host. The peak is
the END state: the lane only grows and frees nothing during the run.
Sampled every 10 min, in GiB: 4, 10, 14, 17, 27, 68, 70, 121, 137, 160, 181.
The two large steps came at about 40-50 min and 60-72 min.

Where it sat at the end, in GiB:

| tree | GiB |
|---|---:|
| `build/zephyr-workspace-builds/3.7` (32 dirs, ~1.22 each) | 35.0 |
| `build/sizes-probe` (one `rustc-1.99.0` dir) | 34.6 |
| `build/cmake-fixtures` | 11.0 |
| `build/corrosion-cargo` (native 4.8, metadata-probe 2.9) | 7.8 |
| `build/compile-check-fixtures` | 7.1 |
| `build/cargo-fixtures` | 2.5 |
| `examples/workspaces` (rust 12.9, features 8.1, cpp 3.8, c 3.1) | 37.3 |
| `examples/linux` (rust 10.9) | 13.7 |
| `packages/testing/nros-tests` | 18.6 |
| `packages/cli/target` | 2.4 |

sccache grew 0.4 GiB (30.2 to 30.6), outside the worktree. The shared caches
were warm (cargo registry, sccache, the `~/.nros` store, the Zephyr
workspaces), so a cold host adds the store on top of the 177 GiB.

**Acceptance is not met.** 177 GiB is above a 150 GB gate host, and above the
127 GB lower bound measured on 11-22. Against this table, the mitigations
above in their ranked order:

- mitigation 1 reaches about 55 GiB: `build/sizes-probe` 34.6, the
  Corrosion trees 7.8, plus the workspace configures, which issue 2702's
  prune already reaches;
- mitigation 2 is fixed at 35 GiB: the west dirs stay;
- `packages/testing/nros-tests` at 18.6 GiB is a tree none of the three
  names.

The raw samples and logs are kept on host 142 in
`/home/aeon/repos/nano-ros/tmp/issue-2840/`: `2840-run2-samples.tsv` and
`2840-run2-fixtures.log` for this run, `2840-run1-*` for the incomplete one.
