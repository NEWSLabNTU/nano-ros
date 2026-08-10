---
id: 503
title: "nros-log Record.timestamp_ns exists but is hardcoded 0 at every emission site — on-target timestamps are one extern away"
status: resolved
resolved_in: "same-day fix; nros-log `platform-clock` feature"
type: enhancement
area: api
related: [issue-0502, issue-0504]
---

## Resolution (2026-08-11)

New opt-in `nros-log` feature `platform-clock`:

- The emission macros and the `log`-crate bridge populate
  `Record::timestamp_ns` from `nros_platform_clock_us` (the universal
  per-platform export the executor's timer accounting already links)
  via a `#[doc(hidden)]` `__timestamp_ns()` helper; without the
  feature the helper returns `0` and imposes no link-time requirement
  — the historical behavior.
- `PlatformSink` prefixes rendered lines with `[sssss.uuuuuu]` when a
  record carries a stamp. Done by message rewrite because the
  `nros_platform_log_write` ABI has no timestamp parameter; widening
  that ABI (every platform port + cffi header) was judged not worth it
  for a prefix.

Opt-in rather than default because the extern is a link-time
requirement on the final binary: every real image satisfies it via its
platform port, but host tools composing custom sinks without a
platform port would not. Flipping the default is a candidate after a
full matrix run. Verified: `cargo test -p nros-log --features
platform-clock` shows the posix round-trip emitting
`[365155.332783] error payload`. Timestamp quality on FreeRTOS
followed from issue #502 (sub-tick clock) in the same change set.

## Original problem (condensed)

`Record.timestamp_ns` ("`0` if unavailable") was hardcoded `0` in
`macros.rs` and `log_compat.rs`, and no sink printed it. Embedded log
analysis therefore rode on host-side stamps that measure the
transport, not the target — on a QEMU icount lane, host stamps
inflated a healthy 10 ms loop's late-period rate ~35x (5.7% vs 0.3%
on-target).
