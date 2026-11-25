---
id: 2971
title: "An ordinary violation-cpp tick costs 37..65 ms of guest time on mps2/an385 under icount, and nobody knows whether that is the emulator or nano-ros"
status: open
type: bug
severity: low
area: [testing, zephyr, qemu, monitors, zenoh]
related: [issue-2273, issue-2508, phase-544, phase-474]
found: 2026-11-25
---

**Home: [phase-544](../roadmap/archived/phase-544-harness-run-plan-followups.md)** (the QEMU leg, issue 2273).

## What

`examples/workspaces/violation-cpp`'s handler does, on an ordinary 10 Hz tick, three
zenoh publishes and one `printf`. `max-latency-runtime` times callback entry to exit,
so the dispatch minus any busy-wait is the cost of that work. On mps2/an385 under the
board's `-icount shift=auto`, measured in guest time:

| date | main | how measured | ordinary dispatch |
| --- | --- | --- | --- |
| 2026-11-09 | bf9fccba83 | tick 6 (150 ms busy-wait) read 164..177, issue 2273 | 14..27 ms |
| 2026-11-13 | bf9fccba83, rebuilt | same tick read 189, 215, 189, issue 2508 table | 39, 65, 39 ms |
| 2026-11-13 | 121a6a99c6 + 110 ms overrun | tick 6 read 147 | 37 ms |
| 2026-11-25 | cffdca5c9b, tier 2 (four lanes at once) | tick 6 read 152; tick 5 (no busy-wait) read 63; tick 10 read 61 | 42, 63, 61 ms |

The same commit (bf9fccba83) read 14..27 ms on 11-09 and 39..65 ms when rebuilt on
11-13, which already argues against a per-commit regression: issue 2508's bisect found
no commit that owns the change. But the 11-09 numbers were never re-measured on a fresh
build, so the series cannot rule one out either.

Why it matters: the contract declares 50 ms on that path, so the board's ordinary
dispatch straddles its own bound. The test now identifies the provoked overrun instead
of counting verdicts (`check_provoked_overrun` in `violation_channel_e2e.rs`), which
holds only while an ordinary dispatch stays under the overrun's 110 ms. If the cost
is a real per-publish cost growing in nano-ros, it is a product bug that every
Cortex-M image pays, not a test margin.

## How to tell the two apart

`shift=auto` lets guest time follow host time while the guest waits, so host load and
SLIRP latency leak into what the guest measures. A fixed shift removes that:

1. Run the same `zephyr_mps2` image under `-icount shift=N,sleep=off` (N fixed, e.g. 4)
   instead of `shift=auto`, solo, 10 runs. Record the per-tick dispatch (temporarily
   lower the declared 50 ms, or log the dispatch time, so every tick prints a value).
2. Repeat at bf9fccba83 and at current main, both built fresh.
   - Same distribution at both ends under a fixed shift ⇒ the drift is emulation and
     host load. Close as such.
   - Higher at main under a fixed shift ⇒ a real per-dispatch cost grew. Bisect with
     the fixed shift, which makes each step deterministic.
3. Independently, split the cost: time each `publish` and the `printf` inside the
   handler, to say which of the three publishes (or the console) owns it.

## Follow-up option, not taken for the release

Make the 50 ms declaration true on that board instead of identifying the provoked
verdict. Today it needs a second bringup: the contract is one
`<stem>.contract.yaml` beside the launch file, with no per-image key in `system.toml`.
A per-image contract key would let the QEMU image declare its own budget, but a budget
derived from a peak that moves 14 → 65 ms is only stable once this issue says why it
moves.
