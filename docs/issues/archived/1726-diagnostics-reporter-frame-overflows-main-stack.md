---
id: 1726
title: "The /diagnostics violation reporter built a 5 KB DiagnosticArray on
  the spin thread's stack: a 10 KB frame that overflowed a 16 KiB main stack"
status: resolved
type: bug
area: [core, diagnostics]
severity: high
found: 2026-10-06
related: [phase-474, issue-1635, issue-1724]
resolved_in: "phase-474 I7, stack half (feat/474-timer-latency)"
---

## What happened

On the safety island (S32K344, phase9-W4, main stack 16 KiB) the board
halted 60 ms after its first stored violation: fatal 35
(`K_ERR_ARM_USAGE_ILLEGAL_EPSR`), PSP inside the idle thread's stack, which
was zeroed along with the bottom of `z_main_stack`. The reporter (issue
1635) runs on the spin thread, which is main. The island raised
`CONFIG_MAIN_STACK_SIZE` to 24576 (high-water 18,780 B).

## Cause

`nros::contract::publish_violation` built the report as a value:
`DiagnosticReporter::report_violation` returns `Option<DiagnosticArray>`, a
four-slot vector of `DiagnosticStatus` (1,224 B each, eight key/value slots
apiece) plus a `Header` with a 256 B frame id: 5,176 B on a 64-bit host,
plus the status built before it and the 512 B CDR buffer. Measured on the
host dev build: `publish_violation`'s frame was 0x1000 + 0x1000 + 0x8d8 =
10,456 B. The phase-474 reading blamed the 512 B buffer; it was the smaller
part.

## Resolution (phase-474 I7, the stack half)

`nros_diagnostics::write_violation_report` streams the same report into the
CDR writer from the borrowed strings, byte-identical to serializing the
value (test `the_streamed_report_is_the_value_reports_bytes`, XCDR1 and
XCDR2, including a string past its field's capacity, which both forms send
empty). `publish_violation` uses it. Measured on the same host build:
`publish_violation` 0x260 = 608 B (the 512 B buffer and the writer) and
`write_violation_report` 0x188 = 392 B, about 1 KB against 10.2 KB. The
512 B buffer stays on the frame: moving it to `.bss` would need one buffer
per executor or a lock between tiers, for a tenth of the saving. Not
measured on the board; the island's 24576 can be re-tried at 16384.

Not done here (phase-474 I7's other half): the reporter still publishes on
the spin path at detection. Each report costs 15-20 ms of spin on the
island's 921,600-baud serial link, which itself trips
`release-jitter-runtime` and `timer-overrun-runtime` (one verdict breeds
more). Deferring the publish to an idle slot or coalescing reports is a
scheduling change, left open.
