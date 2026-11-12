---
id: 2462
title: "The Zephyr QEMU Cortex-M board promises `cadence = \"daily\"`, and the job RFC-0122 D8 binds that cadence to builds Zephyr images and runs none"
status: open
type: limitation
area: [ci, zephyr, testing]
severity: medium
found: 2026-11-12
related: [1029, 1720, phase-535]
---

**Home: [phase-535](../roadmap/phase-535-example-reorg-and-cadence-ci.md) W8.4.**
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
