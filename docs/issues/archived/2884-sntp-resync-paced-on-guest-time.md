---
id: 2884
title: "The SNTP re-sync interval runs on guest time, so on an FVP that races an idle guest a 10 s interval re-queries several times a real second"
status: resolved
resolved: 2026-11-28
resolved_in: "the change that adds this line"
type: design-question
severity: low
area: [clock, zephyr, network]
related: [phase-582, phase-478, rfc-0141, 0758, 2866]
found: 2026-11-23
---

**Home:** [phase-582](../../roadmap/phase-582-wall-clock-source.md), rehomed
2026-11-23 from [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
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
  queries no closer than 10 s of host time apart over a 2-minute run
  (amended 2026-11-28, below: the interval holds from the third answer on,
  apart from a server step); the
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

Was open: the FVP measurement (a local SNTP responder, a 10 s interval, a
2-minute run, queries from the second on no closer than 10 s of host time),
and the island setting a non-zero interval on its FVP images. This issue
closes when both are done.


## Measured on the FVP (2026-11-27, the island's lane)

The island's `Zephyr FVP` job runs the fvp-tap controller image with a
non-zero re-sync interval against its own SNTP responder, which logs each
answer at host time. Run 46018854324 of `autoware-safety-island`
(head 6b93f0f613, branch `asi-w3-resync`) passed (`FVP TAP SNTP re-sync OK`).
In that run:

- The interval was 2,000 ms, not 10 s, and the run lasted about two minutes
  (61 answers). The server stepped its clock once, between answers 30 and 31.
- The guest raced: the image printed `paced on the server's clock (0.884424
  server s per image s)`, then `0.599206`, so the image's clock ran up to
  1.67x real time.
- The responder's host-time gaps between answers, in order: 1.8, **1.3**,
  2.0, 2.1, 2.1, 2.0, 2.0, 2.1, 2.0, 2.0, 2.1, 2.0, 2.0, 2.0, 2.1, 2.1, 2.1,
  2.0, 2.0, 2.2, 2.0, 2.1, 2.0, 2.1, 2.0, 2.0, 2.0, 2.1, 2.0, 2.0, **1.2**,
  then 29 more gaps, each between 2.0 and 2.1 s.

So the second half of the acceptance is met: the island sets a non-zero
interval on its FVP images, and its FVP lane is green. Unpaced, a 1.67x guest
would have re-queried every 1.2 s of host time; 57 of the 60 gaps hold the
interval, within 10 %.

Three gaps are short, and each is an edge this issue already names:

- **Gap 1 (1.8 s):** the first re-sync after boot has one answer and no rate,
  so it is paced on monotonic time.
- **Gap 31 (1.2 s):** the server stepped. The rate across that gap is out of
  range, the image printed so, and the gap was paced on monotonic time. That
  is 2 s of image time, 1.2 s of host time at 1.67x.
- **Gap 2 (1.3 s):** this is the one the acceptance as written does not
  allow. The rate pacing a gap is the one measured over the gap before it.
  The first measured rate, 0.884, spans the boot answer and the first
  re-sync, while the guest was still busy booting and ran nearer real time.
  The guest then went idle and sped up to 0.599, so the second gap waited
  2 / 0.884 = 2.26 s of image time, which is 1.36 s of host time. From the
  third gap on, the rate is current.

Not a defect of the rule. A rate measured over the last gap lags a change in
the simulator's speed by one gap, and boot is where that speed changes most.
Only a calibration query would remove it, and that was left to the user and
not done. The acceptance's "queries from the second on" therefore cannot hold
on a guest whose speed changes after boot. "From the third on" holds in this
run: 57 gaps, all between 2.0 and 2.2 s, apart from the step.

**Ruled (2026-11-28, user):** the acceptance is amended to "the interval
holds from the third answer on, apart from a server step", and this issue
closes on the run above. No calibration query. The FVP run used a 2 s interval
rather than the 10 s the acceptance names. The rule is the same at both, and
2 s gives five times the samples in the same two minutes.

`nros_sntp_pace_rate`'s step detection in PR #5025 does not change
any of this. It refines which rates count as a step inside the 0.001–100
band, and none of the rates above is near a boundary.
