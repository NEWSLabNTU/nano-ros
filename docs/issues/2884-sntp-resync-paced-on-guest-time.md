---
id: 2884
title: "The SNTP re-sync interval runs on guest time, so on an FVP that races an idle guest a 10 s interval re-queries several times a real second"
status: open
type: design-question
severity: low
area: [clock, zephyr, network]
related: [phase-582, phase-478, rfc-0141, 0758, 2866]
found: 2026-11-23
---

**Home:** [phase-582](../roadmap/phase-582-wall-clock-source.md), rehomed
2026-11-23 from [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
I44, where the consumer found it. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`, nano-ros pin `8d4e70a382`.

## What happens

`clock.sntp_resync_interval_ms` (phase-582 W3, RFC-0141 D5) repeats the SNTP
exchange on a thread paced by `k_msleep`, i.e. the image's monotonic time;
issue 2866's ruling documents the same for the `board` source ("guest time on
a simulator"). The Arm FVP runs an idle guest's clock about 10x to 100x faster
than real time, so on the island's FVP images a 10 s interval re-queries the
server several times per real second. The island's own re-sync thread
(before phase-582) behaved the same way, which is why it was off.

The thread's fixed 2048-byte stack is the other half of making W3 usable on
the FVP (the same `sntp_simple` call measured 7024 bytes there); that is in
review as PR #4632, with its issue file.

## Island workaround

`clock.sntp_resync_interval_ms` stays 0 on every image in
`actuation_module/src/controller_bringup/system.toml`, so the island has one
boot-time exchange and no re-acquisition, which is the 0758 requirement it
asked for. Its phase-15 P5: "Its thread is paced by `k_msleep` (guest time,
raced ~10x to ~100x by the FVP when the guest idles, ...) ... the pacing has
no issue yet."

## The question

One of:

1. Pace the re-sync on the wall clock it maintains: the next query is due when
   the synced wall clock has advanced by the interval since the last answer
   (checked on a short monotonic tick). On hardware the two agree; on a
   racing simulator the wall clock follows the server, so the query rate is
   the stated one in real time.
2. Keep guest-time pacing (it is what the knob says), and add a floor on the
   real spacing, for instance never re-query before the server's previous
   answer plus the interval.
3. Keep it and document that a simulator multiplies the rate; the consumer
   keeps 0 on simulator images.

## Acceptance

- The ruling is recorded here and in RFC-0141 D5.
- Under 1 or 2: on the FVP with a 10 s interval, a local SNTP responder logs
  queries no closer than 10 s of host time apart over a 2-minute run; the
  island sets a non-zero interval on its FVP images (with PR #4632's stack
  knob) and its FVP lane stays green.

## Ruling (2026-11-24, user)

Option 1: pace each re-sync on the synced wall clock. The next query is due
once that clock has moved forward by the interval, checked on a short
monotonic tick. Recorded in RFC-0141 D5.

Applying it showed that this port's wall clock is monotonic time plus an
offset between answers, so it runs at guest rate too. Read literally, the rule
would change nothing. To meet the ruling's intent, a real-time rate, the
re-sync follows the server's clock by its RATE, measured over the last two
answers. The next query is due once the monotonic time since the last attempt,
scaled by that rate, reaches the interval.

Two edges, agreed with the manager:

- The first re-sync after boot has one answer and no rate, so it is paced on
  monotonic time and can come early on a racing simulator. Every later gap is
  real-time paced. The FVP acceptance below is measured from the second query.
  A calibration query instead is the user's call, and is not done.
- A rate outside 0.001 to 100 (a server step either way, no monotonic time
  between answers) falls back to monotonic pacing for that gap, and the image
  prints that.

## Implementation

- **The rule.** `nros_sntp_pace_answer`, `nros_sntp_pace_rate` and
  `nros_sntp_pace_due` in `nros/clock_resolve.h`. They are pure: the caller
  supplies every time.
- **Host tests.** Cases 10 and 11 of `tests/clock-resolve-tests.sh`: monotonic
  pacing before two answers, a 10x racing guest waiting 100 s of guest time for
  a 10 s interval, hardware at rate 1, and the out-of-range cases (a step
  forward, a step back, a zero monotonic delta, below the bound), plus the
  upper bound itself.
- **Zephyr.** `nros_zephyr_sntp_resync_loop` takes the boot answer when sntp is
  latched, checks the rule on its tick, and notes each answer. It prints when
  the pacing changes: on the server's clock, with the rate, or out of range.

Still open: the FVP measurement (a local SNTP responder, a 10 s interval, a
2-minute run, queries from the second on no closer than 10 s of host time),
and the island setting a non-zero interval on its FVP images. This issue
closes when both are done.

## FVP measurement (2026-11-24, nros 175) and the follow-up

nros 175 ran the measurement on the island's FVP image
(FVP_BaseR_AEMv8R 11.31.28):
- `sntp,platform`, a 10 s re-sync interval;
- a host responder logging each query's host monotonic time;
- 200 s per run.

**Before** (main 2e94c2f841), at a guest/host ratio of 2.90: 58 queries, with
gaps of 2.6 to 4.7 s.

**After** (#4763, d03ec73295), at 3.64: 21 queries. Host-time gaps, the first
being the boot exchange to the first re-sync:

    4.531 6.086 10.321 9.943 10.094 10.030 10.304 10.059 9.988 10.222
    9.967 10.117 10.131 10.170 10.045 9.957 10.139 10.134 10.050 10.058

The fix works, but the literal acceptance misses twice:

- **The second gap is 6.086 s.** Its rate (0.452) came from the 4.5 s boot
  window, and the guest then raced harder (steady state 0.27).
- **Four gaps fall 12 to 57 ms short of 10 s.** The last gap's rate lags the
  guest's speed changes.

Manager's decision: keep the acceptance as written. #4763 merges on its gate,
and a follow-up adds two margins (RFC-0141 D5):

- a +1 % guard band on every paced deadline;
- a doubled deadline while the rate's window is shorter than one interval of
  server time.

Host-tested in `tests/clock-resolve-tests.sh` case 10, including the
measured boot window, whose doubled deadline is 12 s or more of host time at
3.64x. The issue closes on 175's re-run at the follow-up's head.

