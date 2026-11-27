---
id: 2462
title: "The Zephyr QEMU Cortex-M board promises `cadence = \"daily\"`, and the job RFC-0122 D8 binds that cadence to builds Zephyr images and runs none"
status: resolved
resolved: 2026-11-27
resolved_in: "the change that archives this issue"
type: limitation
area: [ci, zephyr, testing]
severity: medium
found: 2026-11-12
related: [1029, 1720, phase-535, phase-558, phase-559]
---

**Home: [phase-535](../../roadmap/phase-535-example-reorg-and-cadence-ci.md) W8.4.**
Recorded in `.config/cadence-job-baseline.toml`, the baseline of
`just check cadence-jobs` (phase-535 W8's acceptance gate).

## What the registry says

`packages/boards/board-support.toml` gives `nros-board-mps2-an385-zephyr`
(`ZephyrQemuCortexM`) `cadence = "daily"` with `nightly_token = "zephyr"`.
`check-board-tiers` accepts that because `zephyr` is a module the nightly
reaches (`nightly_tokens()` adds it by hand: "zephyr and linux have their own
homes").

## What the job does

RFC-0122 D8 binds `daily` to the nightly `platform` jobs and, for Zephyr, to
the Zephyr nightly. Every `platform` job ends in `just test <platform>`. The
Zephyr nightly (`nightly.yml`, the 05:00 cron: `zephyr-example-matrix`,
`zephyr-dual-line-summary`, `zephyr-copy-out`) runs `just zephyr build-one`,
`just zephyr ci-both` and the copy-out check. All of them build. None runs a
Zephyr cell. So no daily job runs this board's cells, which is what a
`daily` cadence promises (RFC-0114, as amended by RFC-0122 D8).

The board's cells may still reach a runtime through the self-hosted tier-2
pairwise cover (`matrix-nightly`), when its greedy cover happens to pick
them. That is a property of the cover, not a job bound to the cadence.

## Fix

Either of:

- phase-535 W8.4: the Zephyr nightly goes to 2 lines x 3 languages with a
  runtime step (`just test zephyr`), and the baseline entry is deleted;
- RFC-0122 D9: lower the row's cadence to what a job keeps.

## Status (2026-11-17): still true

`nightly.yml`'s Zephyr jobs only build: `build-one`, `ci-both`,
`check-copy-out`, `test-c-port` and `stack-floor-drift`. None runs `just
test zephyr`. `.config/cadence-job-baseline.toml` still carries the
`ZephyrQemuCortexM` row as unserved. Closing it is either a runtime job for
the `daily` cadence, or lowering the board's cadence under RFC-0122 D9.

**Suggested owner: the phase-535 W8.4 owner, or a user decision** to lower
the cadence.

## Resolution (2026-11-27)

Closed by the tier-2 nightly, as the user ruled on 2026-11-22 (phase-585,
"Zephyr runtime coverage"): "fix tier 2 first, with no dedicated Zephyr
nightly job". So neither fix above was taken. W8.4's runtime step in the
Zephyr nightly is superseded, and the cadence stays `daily`.

- **The job that keeps it** is `run-matrix.yml`'s scheduled `matrix-nightly`
  (`just ci matrix-nightly`, cron 06:00). Its pairwise cover runs a cell of
  every platform in its pool, which is every platform without an own lane.
  `ci_lane`'s `lanes_touch_every_declared_value_of_every_axis_they_cover`
  holds that, so the binding is a property of the cover's construction, not
  of which cells the greedy walk happens to pick.
- **Measured:** the scheduled tier-2 nightly of 2026-11-26 (run 45767321390,
  green) passed 16 `ZephyrQemuCortexM` tests.
- **`check-cadence-jobs`** accepts that job for a daily row whose module the
  nightly `platform` job cannot run (`zephyr`), and only for such a row. A
  module the `platform` job runs stays held to its per-board job, and an
  own-lane platform (`Px4`, read from `PlatformId::own_lane`) is refused.
  Four self-test cases cover this. The baseline entry is deleted, and the
  gate itself demanded it ("served now — delete the entry").
- **`tier-health.py`** reports the row's lane as the tier-2 nightly instead of
  UNCHECKED.
- **RFC-0122 D8** is amended the same day, and phase-535 W8.4 is marked
  superseded.

The verdict is the tier-2 nightly job's, shared by every platform in the
cover. The per-board split of that verdict is `board-results.py`'s, on
`run-matrix.yml` (issue 2997).

