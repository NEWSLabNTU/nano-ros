---
id: 1714
title: "The contract-violation ring keeps the FIRST eight verdicts since boot,
  and a board with no console has no other channel, so a late violation is
  neither stored nor seen"
status: resolved
type: limitation
area: [core, diagnostics, zephyr]
severity: medium
found: 2026-10-01
related: [phase-474, issue-0514, issue-1635, rfc-0052]
resolved_in: "phase-474 I1 (feat/violation-ring-and-arming)"
---

## What happened

On the Autoware Safety Island (S32K344, phase8-W31 bring-up) the executor's
violation ring, read over SWD by scanning RAM for rule-id string pointers,
held 8 of 8 slots, all start-up entries (`takeover-trace.md` section 11). The
ring was a `heapless`-shaped vector whose full push was REFUSED and counted, so
it kept the first `MAX_VIOLATIONS` (8) verdicts since boot and lost every later
one. The console the log floor writes to (issue 0514) is lpuart0, which is not
wired on that board, and nothing traced a violation. A monitor whose verdict
cannot leave the board was not yet a measurement.

## Resolution (phase-474 I1)

- The ring keeps the LATEST `NROS_EXECUTOR_MAX_VIOLATIONS` (Kconfig
  `CONFIG_NROS_EXECUTOR_MAX_VIOLATIONS`, default 8): a push into a full ring
  evicts the oldest and counts it (`violations_dropped`); every stored verdict
  is numbered (`violations_total`, `drain_violations_numbered`).
- `NROS_VIOLATION_RECORD`: a `#[no_mangle]` `repr(C)` all-`u32` static,
  present with the boot report, never drained, keeping the latest N of every
  executor with `total` / `head` / `dropped` / `suppressed_before_arm` /
  `armed`. `scripts/read-violation-record.py` decodes it (`--addr-only` for
  the `savemem` line); `just check violation-record` holds its constants and
  rule table to `monitor.rs`.
- Trace markers 21-24 (`seq << 8 | rule code`, endpoint FNV-1a, measured,
  declared) at detection with `CONFIG_NROS_TRACE_CALLBACKS`.
- An opt-in drain-and-report hook (`CONFIG_NROS_VIOLATION_DRAIN_REPORT`) logs
  each entry with its number and the counters at the end of its spin; a
  build-time executor default, so it reaches the C, C++ and Rust entries.

Verified by unit tests in `nros-node` (`the_violation_ring_keeps_the_latest_and_numbers_them`,
`the_record_keeps_the_latest_and_counts_what_it_overwrote`,
`a_stored_violation_emits_its_trace_markers`,
`the_drain_report_hook_drains_at_the_end_of_the_spin`) and phase-474 T4's
end-to-end test. Not measured on the board: the island's next bring-up reads
the record by name instead of scanning.
