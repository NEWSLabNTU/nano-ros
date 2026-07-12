---
id: 182
title: "realtime tiers e2e: high tier does not outrun low tier (nuttx c/cpp tiers + cpp subnode/portable) — ctrl==telem counts"
status: open
type: bug
area: testing
related: [phase-281, phase-271]
---

## Summary

Deterministic (serialized rerun, fresh fixtures 2026-07-12):

- `realtime_tiers_{c,cpp}_nuttx_e2e`: `high-tier /ctrl counter 4 is not ≥3×
  the low-tier /telem counter 4 — the 10 ms tier is not outrunning the 100 ms
  tier (281 W3-nuttx / NuttxBoard::run_tiers)`
- `realtime_subnode_cpp_e2e` + `realtime_subnode_cpp_portable_e2e`:
  `ctrl=5 telem=5 — per-group binding may not be seeding bind_group_sched
  correctly (RFC-0047)`

Both shapes publish (counters advance) but at the SAME rate — the tier/
callback-group scheduling separation delivers no rate differentiation.
`realtime_tiers_rust_nuttx` failed in the parallel sweep too (needs a
serialized confirm once the rust fixture lane builds — see #181).

## Notes

- These lanes were part of the museum-binary population; unclear when they
  last ran on fresh images. Counter 4 vs 4 / 5 vs 5 looks like both groups
  ticking at the same period — either the tier spec never reaches the
  executor (bind_group_sched seeding) or both tiers land on one thread.
- freertos realtime tiers (`realtime_tiers_{c,cpp}_freertos_e2e`) were NOT
  in the failing set — nuttx + the cpp subnode pair only.
