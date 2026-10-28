---
id: 2100
title: "host-tests' tier-1 gates outgrow the GitHub-hosted runner's disk: the 26 concurrent check-build gates leave ~110 G against ~75 G free"
status: resolved
type: bug
area: [ci, build]
severity: medium
found: 2026-10-27
related: [phase-466, 1353, 2052, 2130]
resolved_in: "PR #2754 (2026-10-27)"
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**, since 2026-10-27.

## Measured

- **Symptom.** `host-tests`' "tier-1 gates (just check)" job ran out of disk
  (`No space left on device`, then `ld` Bus error) on 4 of the 7 scheduled
  nights to 2026-10-26.
- **Same class on `live-peer`.** Its host job ("rows whose board IS this
  runner") died on 10-26 with the runner agent's own
  `IOException: No space left on device`, so it gave no verdict.
- **The runners.** Every job on both lanes runs on a GitHub-hosted
  `ubuntu-22.04` runner (`GitHub Actions <n>`, group "GitHub Actions"): 4 CPU,
  15 GiB RAM, a 146 G root, and 75-111 G free after `reclaim-disk.sh`. That is
  the hosted image's size, not something we provision, so issue 1353's
  "runner configuration" ruling (which was about self-hosted runners) does not
  apply here. The job's own disk use is the only lever.
- **Run 40263677513.** Reclaim left 75 G free. The gates then grew `build/`
  from 29 G to 61 G, `target/` by 20 G and `packages/cli/target` by 7 G, and
  the runner reached 100 % used.

Each of the 26 `check build` gates was measured cold, one at a time, in a
fresh worktree from main `509b31606c`, on 2026-10-27. Peak is the largest drop
in filesystem free space while the gate ran, which includes `/tmp`.

| gate | peak | left | where |
| --- | --- | --- | --- |
| `linux-check` | 42.1 G | 42.1 G | 36 G `build/example-lint` (one clippy `--target-dir` per example leaf, 49 of them, 0.4-1.2 G each), 4.5 G sizes-probe, 2.8 G metadata-probe |
| `cli-tests` | 18.4 G | 18.4 G | the CLI workspace's debug test target |
| `template-copy-out` | 15.4 G | 13.2 G | 7.8 G `build/corrosion-cargo`, 3.9 G sizes-probe |
| `cpp` | 6.2 G | 6.2 G | three `target-check-cpp*` dirs, sizes-probe |
| `required-features-tests` | 4.9 G | 5.0 G | `target/` |
| `ament-census-sizing` | 4.7 G | 4.7 G | sizes-probe |
| `borrowed-e2e` | 3.7 G | 4.0 G | `target/` |
| 12 more | 1-2.8 G each | | `target/`, `target-check-*` |
| the other 7 | under 1 G | | |

CI runs the 26 gates at once and never cleans in between, so the job's peak
is about the sum, roughly 110 G. That is an upper bound, because gates that
share `target/` overlap. It is consistent with the after-report above.

## Cause

In all three of the largest gates, the bulk is output that nothing reads again
once the gate has its verdict. It persists so that a developer's next run is
incremental. A hosted CI run starts cold and gains nothing from it.

## Fix (2026-10-27)

`NROS_CI_PRUNE_GATE_OUTPUTS=1`, set by `host-tests` on the tier-1 gates step,
makes each of the three release its output. Unset, nothing changes. The one
helper is `scripts/build/gate-prune.sh`, and it refuses any path outside the
checkout.

- **`linux-check`.** `lint-example-leaf.sh` removes each leaf's lint target dir
  as soon as clippy has given its verdict, pass or fail. Measured on
  `examples/linux/rust/talker`: 635 MiB kept with the switch off, removed with
  it on, same clippy result.
- **`cli-tests`.** `packages/cli/target/debug` goes when the gate exits.
  `target/release/nros`, which every other gate runs, stays. Measured: the gate
  passes, the debug target is removed (35 G on the host that measured), and
  the release binary is untouched.
- **`template-copy-out`.** Each copy's build cache goes under its own scratch
  dir, which the gate already deletes. That means `NROS_BUILD_ROOT`, plus the
  sizes-probe dir that `scripts/build/cargo.sh` exports from the checkout's
  root. This is set inside `build_copy` and `build_cross`, so the self-test
  copies are covered too. Measured: the gate passes (10 projects built) and
  leaves 110 MiB, against 13.2 G cold, with nothing new under the checkout's
  `build/`. The first attempt covered `run_one` only and still left 1.6 G
  through the self-test copy's metadata probe.

Together the three account for about 67 G of the ~110 G.

## What would close it

A scheduled `host-tests` run whose tier-1 gates job reaches its after-report
with free space left. The per-directory breakdown that `disk-report.sh`
prints since #2717 should show `build/example-lint`, `packages/cli/target`
and `build/corrosion-cargo` small. `live-peer`'s host job is the same class but
a different population: its fixture build, not these gates. If it still runs
out of disk, it gets its own item.

## Verified (2026-10-29)

The first scheduled `host-tests` run after #2754, run 40625461704
(2026-10-28), passed its tier-1 gates job. The after-report read
`68% used, 48G free`, against 0 bytes free on the four failing nights.

`live-peer`'s host job still ran out of disk on 2026-10-28. As said above,
that is a different population (its fixture build and cells), so it is
[issue 2130](../2130-live-peer-host-job-disk-exhausted.md), not this one.
