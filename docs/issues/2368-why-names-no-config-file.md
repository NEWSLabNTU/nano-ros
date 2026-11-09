---
id: 2368
title: "`nros run --dry-run --why` reports a Zephyr locator read from the build's `.config` on the board rung without naming the file"
status: open
type: limitation
severity: low
area: [cli, run, zephyr]
related: [phase-548, 2301]
found: 2026-11-09
---

**Home: [phase-548](../roadmap/phase-548-three-languages-every-board.md)**
(found fixing issue 2301).

## What

Issue 2301's fix falls back to the build's rendered `zephyr/.config` when a
Zephyr board names no locator. `--why` reports that value on the `board`
rung, the same word a board descriptor's own default gets, and does not say
it came from `<build>/zephyr/.config` (`CONFIG_NROS_ZENOH_LOCATOR`). Issue
2301 asked for the file to be named, so a reader can find and change it.

## Done when

`--why` names the `.config` path and symbol for a locator read from it, with
a unit test on the provenance text.
