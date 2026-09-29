---
id: 1583
title: "`tier-priority-plan-image` fails the derived Zephyr image's lowered band [4, 9] against the realtime bringups' `tiers.high.zephyr = 9`"
status: open
type: bug
area: [zephyr, codegen, testing]
severity: medium
found: 2026-09-29
related: [issue-1508, issue-1537, issue-1571]
---

## What happens (reported by the issue-1571 agent, not yet re-measured)

The Zephyr post-build gate `tier-priority-plan-image` fails on the derived
realtime image. The image's lowered priority band is [4, 9], and the gate
checks it against the authored `tiers.high.zephyr = 9` in the realtime-c,
realtime-cpp and realtime-rust bringups. None of those `system.toml` files
were changed by the 1571 work, so this appears to be the state on `main`
since #1508 (baked priority plan) and the #1537 derived-tier lane.

## To decide

Which side is wrong: the gate's reading of an authored priority against a
derived band, the band, or the bringups' authored value? #1508 made the
image's own `.config` plan the source of priorities, and an authored raw
priority at the band edge may simply be illegal now. If so, the gate is right
and the bringups need updating.

## Acceptance

- The gate passes on every realtime bringup's Zephyr image, or the bringups'
  values are corrected with the reason recorded.
- A merge-gating or nightly lane runs the gate, so it does not sit red unseen.
