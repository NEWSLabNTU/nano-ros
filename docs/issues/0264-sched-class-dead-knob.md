---
id: 264
title: "posix-family sched_class is a dead knob: accepted, carried, consumed by nothing (nuttx hardcodes SCHED_FIFO)"
status: open
type: limitation
severity: low
area: orchestration
related: [phase-302]
---

## Finding (implementation-completeness audit, 2026-07-25)

`[tiers.*.{posix,nuttx}] sched_class = "..."` passes bake validation
(posix-family only), rides `TierRtosSpec`/`ResolvedTier`, and is absent
from `TierSpec` — no runtime consumer on any platform;
`nuttx_run_tiers.c` hardcodes SCHED_FIFO regardless.

## Fix direction

Decide: (a) implement — thread it into `TierSpec` and let nuttx/posix
select FIFO/RR/SPORADIC explicitly, or (b) reject the knob at bake until
implemented (kill the silent dead end). Phase-302 W4 takes (b) unless a
consumer lands first.
