---
id: 1727
title: "The /diagnostics violation reporter published at detection, inside
  the spin being judged: on a slow link one verdict bred jitter and overrun
  verdicts of its own"
status: resolved
type: bug
area: [core, diagnostics]
severity: medium
found: 2026-10-06
related: [phase-474, issue-1635, issue-1726]
resolved_in: "phase-474 I7, scheduling half (feat/474-rest)"
---

## What happened

On the safety island (S32K344, phase9-W4, 921,600-baud serial link) the
trace put the stored verdicts 15-20 ms apart: each one's `/diagnostics`
publish held the spin that long (island `docs/takeover-trace.md` section 12).
Five verdicts in one spin (#3-#7) took 81 ms; a 30 Hz timer dropped an
activation (`timer-overrun-runtime`) and the next spin's release jitter read
88.6 ms (`release-jitter-runtime`). One verdict bred more.

## Cause

`ViolationChannel::record` called the sink (issue 1635's reporter,
`nros::contract::publish_violation`) at DETECTION, once per verdict. The
latency, rate and age rules run at the top of `spin_once`, before that
spin's dispatches, so the reports ran ahead of the timers that were due; and
the release-jitter rule measures the interval between spin entries, so the
reporter's own time read as a late release.

## Resolution

- `record` QUEUES the verdict for the sink (`monitor::ReportQueue`, four
  deep; a fifth evicts the oldest pending and is counted). The ring, the SWD
  record, the trace markers and the log stay at detection.
- `Executor::flush_violation_reports` runs at the end of every spin (both
  the trigger-pass tail and the trigger-miss return), after the dispatches
  and the rules. It hands the queue over only when the next timer is due no
  sooner than the last report took (or a verdict has waited 1 s, or there is
  no clock).
- The sink type is now `fn(ctx, &[Violation]) -> usize`: the reporter
  coalesces as many pending verdicts as fit its 512 B buffer into ONE
  `DiagnosticArray` (`nros_diagnostics::write_violation_reports`; each status
  byte-identical to the single report) and returns how many it took.
- The reporter's time is stated (`Executor::violation_report_cost_us`: last,
  max, pending, overflowed) and is not charged to the release-jitter sample
  that follows it.

Test: `i7_a_slow_report_breeds_no_overrun_or_jitter_verdict` (an 80 ms
handler overruns a 5 ms budget on two publishers beside a 40 ms timer; the
fake sink costs 40 ms per message). With the report at detection it stores
`timer-overrun-runtime` 1 and `release-jitter-runtime` 92 ms beside the two
latency verdicts -- the board's shape; with this change, the two latency
verdicts only, in one sink call. `a_coalesced_report_is_one_array_of_the_single_reports`
holds the coalesced bytes to the value form. Not measured on the board.

What a reader sees differently: a `/diagnostics` message may carry up to
four statuses, and it leaves at the end of the spin (or a later one) rather
than at the top of the spin that judged it. The C and C++ entries are
otherwise unchanged (same reporter, same install call).
