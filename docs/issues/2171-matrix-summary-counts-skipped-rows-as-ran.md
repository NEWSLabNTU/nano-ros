---
id: 2171
title: "realtime_tiers and sched_dims count a lane-skipped row as one that RAN, and file it under `skipped`, not `out of lane`"
status: open
type: bug
severity: low
area: [ci, testing]
related: [2004, 0571, 1758, phase-466]
found: 2026-10-31
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)** — W5 is read off these two summaries.

## Measured

The summaries issue 2004 made visible are now in the tier-2 junit artifacts
(`post-submit-junit` from `run-matrix`, `tier2-nightly-junit` from `nightly`).
Run-matrix 41013529125 (2026-10-30, tier 2) printed:

```
realtime_tiers: 22 row(s) ran, 18 skipped, 0 out of lane
sched_dims: 17 cell(s) ran, 16 skipped, 0 out of lane
```

Only **4** realtime rows executed (`native/rust`, `native/rust-derived`,
`nuttx-arm/c`, `nuttx-riscv/c`) and only **1** sched-dims cell
(`CorePin/linux/rust`, the one `ACCEPT` line). Every one of the 18 and 16
"skipped" entries is a `[SKIPPED:lane] out of lane: … is at coordinate …`
note, so "0 out of lane" is also wrong. The pairwise nightly
41018247377 has the same shape: `22 row(s) ran, 9 skipped` for 13 executed.

## Why

Two separate miscounts, in both files:

1. **"ran" includes the skipped.** `realtime_tiers_e2e.rs:685` does
   `ran += 1` before the row runs; `sched_dims_applied_e2e.rs:590` computes
   `cells.len() - out_of_lane.len()`. Neither subtracts `skipped`.
   `entry_e2e.rs:764` already does it right
   (`cells.len() - skipped.len() - failed.len()`).
2. **Lane deselection lands in `skipped`.** The `out_of_lane` list is filled
   only when `lane_scope::admits(platform)` refuses the whole PLATFORM. Since
   tier 1/2 narrow by COORDINATE (platform, lang, rmw), an admitted platform
   whose row is out of lane reaches `run_one` / `run_cell`, the fixture
   resolver raises `lane_skip!` (`fixtures/lane.rs`), and the catch puts it
   in `skipped`. That is the case the "out of lane" column exists for.

The verdict logic is not affected: `ran == 0 || skipped.len() == ran` still
means "nothing executed", and every inner unmet precondition still fails.
Only the printed line misleads — but that line is the whole of 2004's
evidence, and "22 ran" for 4 is the "1 of 16 ran" vs "16 of 16 passed"
confusion issue 0571 was filed against.

## Fix

Print executed = attempted − skipped − failed, and route a caught panic whose
reason is a LANE skip (`[SKIPPED:lane]`) into `out_of_lane`, keeping
`skipped` for anything else. Fix both files together, and check
`multihost_e2e` and `roundtrip_xprocess_e2e` for the same shape (they share
the catch). A unit test over the summary function, fed one admitted
out-of-lane row, is the negative control.
