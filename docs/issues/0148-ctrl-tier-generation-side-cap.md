---
id: 148
title: "100 Hz control tier generates only ~40 msg/s regardless of tx configuration — executor-timer / scheduling axis, not transport"
status: open
type: tech-debt
area: executor
related: [issue-0145, phase-279, phase-282]
---

## Summary

With the phase-282 tx levers landed (batch + flush thread + split lock — the
transport no longer blocks publishers), the ws-realtime 100 Hz `ctrl` tier
still tops out at **~40 msg/s in EVERY tx configuration** measured on
native_sim:

| tx config | ctrl (100 Hz target) |
| --- | --- |
| 5 ms socket timeout, no batch (200 windows/s) | 33.4 |
| 5 ms + batch + flush thread | 43.6 |
| 100 ms + batch + flush thread + split lock | 34.4 |

A cap that is INDEPENDENT of socket timing while the 10 Hz telemetry tier
runs at ideal rate means the bottleneck moved off the tx path (issue 0145's
mechanism, now resolved): the tier is **generation-limited** — the puts are
never produced at 100 Hz in the first place.

## Suspects (from phase-282 W1.d)

1. Executor timer fire-once-late semantics: a timer that misses its deadline
   fires once instead of catching up, so any stall converts directly into
   lost ticks (no burst recovery).
2. native_sim scheduling of the 1 ms-spin tier thread (the harness runs
   `SLOWDOWN_TO_REAL_TIME`; host scheduling jitter at 10 ms periods).

## Discriminators (do these first)

- Instrument published-count at the TALKER side vs delivered — separates
  generation from delivery definitively.
- A no-transport variant: same tier config, counting timer callback fires
  only (no publish) — if it also caps at ~40/s, the executor/scheduling axis
  is confirmed with zero transport involvement.
- Measure on hardware (native_sim scheduling artifacts disappear).

## References

phase-282 W1.d (measured table + reasoning);
`packages/testing/nros-tests/tests/w1_zephyr_tx_throughput_measure.rs`
(`--ignored` harness); book user-guide/tx-tuning.md.
