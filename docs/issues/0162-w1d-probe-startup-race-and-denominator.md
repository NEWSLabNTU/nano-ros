---
id: 162
title: "w1d tier probe: ~1-in-11 startup race delivers 0 (INCONCLUSIVE), and the delivered/published denominator is off by one"
status: open
type: bug
area: testing
related: [issue-0148, phase-282]
---

## Summary

Two nits in `w1d_native_tier_generation_probe` (`#[ignore]` measurement
scratch, but it decides perf questions — #145/#148 were judged on its output):

1. **Startup race → INCONCLUSIVE**: in ~1 of 11 runs (2026-07-08 #148 rerun
   series) the sink receives NOTHING for the whole 15 s window — the sink's
   "Listener" readiness banner precedes zenoh session/route establishment, so
   the entry can boot, publish into the gossip gap, and never match. The run
   prints `INCONCLUSIVE (no /ctrl values received)` but still PASSES. A
   measurement run that delivers zero should retry once or fail loud, not
   pass with a shrug.
2. **Off-by-one denominator**: the ctrl counter starts at 0, so `max_value`
   understates the published count by one — `delivered/published` uses
   `count/max` and can print >100 % (1498/1497). Cosmetic, but the probe's
   verdict thresholds (`count >= 0.8*max`) inherit it.

## Work

- Gate the measurement on first delivery: wait for one `Received:` line
  (bounded, e.g. 10 s) BEFORE opening the 15 s window; if none, retry the
  boot once, then panic (fail loud, per repo rule).
- Use `max + 1` as the published count.

## References

`packages/testing/nros-tests/tests/w1d_native_tier_generation_probe.rs`,
archived issue 0148 ("Residual observations").
