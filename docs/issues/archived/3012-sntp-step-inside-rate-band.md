---
id: 3012
title: "An SNTP server step whose measured rate lands inside 0.001–100 is not flagged, and the next re-sync gap is paced at that rate"
status: resolved
type: bug
severity: low
area: [zephyr, platform, clock]
related: [rfc-0141, phase-582, issue-2884, issue-3013]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix-sntp-step-detect"
reporter: "nros NXP 175 (ASI, while verifying issue 0758)"
---

## What happened

Reported by NXP 175. RFC-0141 D5 paces the SNTP re-sync on the server's
clock, by the rate the last two answers measured, and distrusts a rate only
outside 0.001 to 100 (`nros_sntp_pace_rate`,
`packages/platform/nros-platform-api/include/nros/clock_resolve.h`). A server
step adds its size to one gap's server advance. A +1000 s step at a 10 s
interval read a rate of about 53, inside the band, so it was taken as the
server's real rate: no "a step?" line, and the next gap was paced at 53x,
about 53 times too soon. At a 2 s interval the same step reads far outside
the band and was caught (ASI run 46018854324).

## Fix

A step of S over an interval I multiplies the rate by about 1 + S/I, while a
simulator's rate drifts far less between two gaps (0.60 to 0.88 measured on
an Arm FVP). The pace state now keeps the rate that paced each gap
(`paced_rate_ppm`). A gap whose rate is more than
`NROS_SNTP_PACE_STEP_FACTOR` (8x) away from that rate, either way, is
`NROS_SNTP_PACE_RATE_STEP`, and it is paced on monotonic time like an
out-of-band rate. The Zephyr re-sync thread says so once, on the change. The
next gap is judged on its own, so pacing resumes at the server's rate after
one gap. RFC-0141 D5 is amended to state the rule.

## Test

`tests/clock-resolve-tests.sh` case 12, host-built:
- NXP's case (rate 1, then a +1000 s step over a 19 s gap, rate about 53)
  is a step, and that gap is due at the interval in monotonic time, not 53x
  sooner;
- the gap after it is rate 1 again;
- drift of 0.599 to 0.884 is no step;
- a -90 s step that keeps the clock moving forward is a step.

Negative control: with the new check disabled, the three step cases fail.
