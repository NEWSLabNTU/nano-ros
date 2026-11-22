---
id: 2866
title: "The `board` wall-clock source is asked once, so a board clock that drifts or later corrects itself never reaches the image"
status: open
type: enhancement
severity: low
area: [clock, freertos, zephyr]
related: [rfc-0141, phase-582, issue-2865]
found: 2026-11-23
---

## What happens

Once `board` answers and latches (RFC-0141 D2, phase-582 W1d), the image
never asks `nros_board_wall_clock_ns` again. If the board's own clock drifts,
or its SNTP/GNSS task later disciplines it, the image's wall clock does not
follow: the offset installed at the first answer stays for the rest of the run.
`sntp` does not have this limit, because `clock.sntp_resync_interval_ms`
(phase-582 W3) re-queries on an interval.

A consumer raised this while porting its island's lwIP SNTP task to the
board hook (ASI P5 on mps3-an536 / s32z270).

## Ruling (manager, 2026-11-23)

This is a follow-up, not part of the 2865 fix. If it is done, `board` gets the
SAME knob shape as SNTP, not a second mechanism:

- a `clock.board_resync_interval_ms` knob, default 0 (one-shot, today's
  behaviour, stays the default);
- above 0, the hook is re-asked on that interval, and each answer is installed
  through the one setter (`nros_clock_set_epoch_offset_us`), so the existing
  upgrade/step policy (`clock.upgrade`, the step counter) decides what a
  re-answer may change;
- the interval is the image's monotonic time (guest time on a simulator), as
  documented for `sntp_resync_interval_ms`.

## Acceptance

- On the ports that resolve `board` (POSIX, FreeRTOS), with the interval above
  0, a stub board whose answer steps mid-run has its later stamps follow the
  step within one interval. With the default 0, the step never reaches them.
- The re-ask never blocks the task that reads the clock: it runs on its own
  thread or task, as the SNTP re-sync does.
- The knob is in the registry with the SNTP interval's wording, and RFC-0141 D2
  records it.
