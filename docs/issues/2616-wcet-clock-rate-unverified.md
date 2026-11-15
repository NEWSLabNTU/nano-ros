---
id: 2616
title: "A WCET profile's clock_hz is never checked against the clock the part actually runs at"
status: open
type: limitation
severity: low
area: [orchestration, platform]
found: 2026-11-15
related: [0403, phase-555]
---

**Owner:** unassigned. **Home:** RFC-0078's "Open question this design cannot
close by itself". No active phase owns it.

## What happens

A WCET declaration (RFC-0078) is in CYCLES and carries its profile's
`clock_hz`, and `exec_ms` is derived from the two. Nothing checks that the
profile describes the hardware. A profile that says `clock_hz =
168_000_000` on a part running at 84 MHz is well formed and wrong, and
halves every derived `exec_ms`. The loader rejects only `clock_hz = 0`
(`ClockRateZero` in `nros-orchestration-ir/src/wcet.rs`).

## Why it stays open

The check needs the running image to report its own clock rate. The platform
ABI has a clock (`nros_platform_clock_ns`) but no rate query. The same gap is
why issue 0403's bench writes `convertible_to_time: false` into its artifact.

## What closes it

A platform clock-rate query, reported by the WCET bench beside each
measurement, and a check that the selected profile's `clock_hz` matches it.
That one query closes this issue and 0403's `convertible_to_time` gap.
