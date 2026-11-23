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
