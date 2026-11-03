---
id: 2223
title: "The scheduled `gate` run's `just check build` fills the hosted runner's disk with its own build output — red on every night from 2026-10-29 to 2026-11-03"
status: open
type: bug
area: ci
found: 2026-11-03
related: [issue-1353, phase-466, phase-489]
---

## What happens

The job `check / build tier + no-std (nightly/manual only)` in `gate.yml` runs on
`schedule` and `workflow_dispatch` only, so it gates no merge. Its step
`just check build` failed on all six scheduled runs from 2026-10-29 to 2026-11-03
(40803922542, …, 41375507503, 41563531517). Phase-489 W8's dispatch on a branch
(41558995861) went red on the same job.

GitHub has no log for that job in run 41563531517: the job log API answers
`BlobNotFound`, and the run's log archive omits the job. The issue-1353 disk
transcripts the job uploads are what survive, and they are decisive.

| | before `just check build` (02:21Z) | after (03:20Z) |
| --- | --- | --- |
| `/` (GitHub-hosted, 146 G) | 103 G used, 44 G free (+6.1 G reclaimed) | **146 G used, 292 K free** |
| `build/` | 29 G (`corrosion-cargo` 26 G) | 56 G: `corrosion-cargo` 33 G, `sizes-probe` 12 G, `example-lint` 11 G |
| `target/` | 403 M | 7.0 G |
| `packages/` | 227 M | 5.9 G |
| `target-check-*` and similar | — | about 25 separate target dirs at 350–570 M each |
| `/tmp` | 2.1 G (sccache) | +3.6 G `nros-template-copy-out.*`, +781 M `nros-scaffold-builds.*` |

The job's own build output takes about 50 G after the step starts, against 50 G
free. Every gate in `check-build` keeps its build tree to the end of the job, so
the sum of all trees is what has to fit.

## Why this is not issue 1353's wontfix

Issue 1353 was closed as wontfix on 2026-10-23: "running out of disk on the CI
runner is a property of how the runner is configured and provisioned". That
ruling fits a self-hosted runner, whose volume and sweep an operator owns. This
job runs on a **GitHub-hosted** runner, which nobody here provisions. Its
disk is fixed, and the only lever is how much this project's gates leave on it.
That is a property of the gates, so it is tracked here.

## What would close it

The scheduled `gate` run's `just check build` finishes with a verdict on the hosted
runner. Candidate fixes, to choose by measurement:

- A gate that builds into a private tree (`target-check-*`, `build/example-lint`,
  `build/sizes-probe`, `/tmp/nros-template-copy-out.*`) drops that tree when it
  passes, inside the lane rather than by an outside sweep. A failing gate keeps
  its tree as evidence, and the lane stops at the first red anyway.
- Split `check-build` across more than one job, as phase-489 W8 split
  `template-copy-out`, so each job gets its own disk. The mechanism already
  exists: `NROS_BUILD_GATE_SHARD=k/n` in `scripts/build/run-gates-parallel.sh`
  (issue 2141, through `scripts/build/gate-shard.py` and
  `.config/build-gate-weights.txt`). But the `gate.yml` job runs `just check
  build` unsharded as one step, so all 28 `build-serial` gates share one
  146 G disk. This is the smaller change, and it is the preferred one.

Per CLAUDE.md, wiping a tree is never a fix for a *wrong* build. Here the trees
are correct and merely finished, so dropping a passed gate's private tree loses
no evidence.
