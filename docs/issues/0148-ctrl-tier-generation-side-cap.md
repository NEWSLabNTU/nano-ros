---
id: 148
title: "100 Hz control tier: ~20% residual tx drop on the split-lock path (generation is at line rate — NOT generation-limited; premise corrected 2026-07-08)"
status: open
type: tech-debt
area: executor
related: [issue-0145, phase-279, phase-282]
---

## 2026-07-08 — discriminator RUN; premise DISPROVEN (not generation-limited)

The #1 discriminator below ("published-count vs delivered") was run —
`tests/w1d_native_tier_generation_probe.rs` (`#[ignore]`, commit `33b3ba574`).
The ctrl node publishes a monotonic counter, so delivered Int32 *values* encode
the published sequence: `max_value/window` = publish rate, `count/window` =
deliver rate. Native ws-realtime-rust, batch+split, fork `ef065b9c`, 15 s, 3
runs, rock stable:

| | publish rate | deliver rate | delivered/published |
| --- | --- | --- | --- |
| /ctrl (100 Hz) | **99.5/s** | **79.2/s** | **80%** |

**The tier is NOT generation-limited** — the timer fires at ~99.5/s, i.e. line
rate. The `~40 msg/s` table below is **pre-`ef065b9c`**: the W2.c overflow-steal
fork fix (which landed AFTER phase-282 W1.d's measurement) roughly doubled ctrl
delivery (34→79). The real residual is a **~20% tx drop** on the split-lock
path, NOT a missing generation axis. This issue's framing (executor timer
under-fire / native_sim scheduling) is superseded: recommend **retitle to the
~20% split-lock tx-drop residual** (candidates: batch/flush-cadence coalescing
vs the 10 ms tier, spare-drain backpressure at sustained 100 Hz) OR resolve if
80% at line-rate generation is acceptable for the promotion decision. The
"no-transport timer-fire-only" discriminator is now unnecessary — publish rate
is already proven at line rate WITH transport.

## Summary (original — premise now corrected above)

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
