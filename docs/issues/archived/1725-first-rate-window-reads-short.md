---
id: 1725
title: "The first rate-hierarchy-runtime window after arming reads a stream
  at its declared rate as short"
status: resolved
type: bug
area: [core, diagnostics]
severity: low
found: 2026-10-06
related: [phase-474, issue-1715, issue-1724]
resolved_in: "phase-474 I8 (feat/474-timer-latency)"
---

## What happened

On the safety island (S32K344, phase9-W4) four `rate-hierarchy-runtime`
verdicts appeared once, right after arming: 10 Hz publishers at 9984 mHz and
30 Hz ones at 29990 mHz (0.03-0.16 % short), the first rate window after
arming, and never in the 35 s after.

## Cause

`monitor::check_rate` runs at the top of a spin, before that spin's dispatch
publishes. In steady state each window rolls at a tick one spin after a
publish, so its count matches its span. The first window opens wherever the
opening spin falls (the first spin, or the spin that arms, which reopens
every window): off the stream's phase it holds N samples over N periods plus
the offset, and the rule has no tolerance. Host reproduction: a 10 Hz stream
on a 10 ms spin grid, armed at an off-grid spin, read 9986 mHz.

## Resolution (phase-474 I8)

The first window is re-anchored at the first tick that sees a new sample
(`MonitorState::aligned`), so it starts on the same phase as the later
windows. A stream silent from the opening on is not held open: once a whole
window passes with no sample it is judged from where it opened (0 Hz). A
slow stream is still judged in its first window, at most one of its periods
later. Test: `a_stream_at_its_rate_armed_mid_period_is_not_judged_slow`
(fails before with 9986 of 10000 mHz; passes after, and checks a 5 Hz stream
against 10 Hz and a silent one are still judged).
