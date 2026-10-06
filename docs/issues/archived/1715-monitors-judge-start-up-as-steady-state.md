---
id: 1715
title: "Contract monitors judge start-up as steady state: registration, the
  first join and inputs not yet started are reported as overruns, jitter and
  silence"
status: resolved
type: limitation
area: [core, diagnostics]
severity: medium
found: 2026-10-01
related: [phase-474, issue-1714, rfc-0052]
resolved_in: "phase-474 I2 (feat/violation-ring-and-arming)"
---

## What happened

The island's W31 bring-up ring held, before any act ran: 4
`timer-overrun-runtime`, 1 `release-jitter-runtime` (`spin` measured=57751
declared=10000), 1 `silence-runtime`
(`/mrm_handler/operation_mode_availability` declared=500) and 2
`rate-hierarchy-runtime` on on-demand topics (phase-474 D2). None is a fault
of the running system; the monitors were armed from the first spin.

## Resolution (phase-474 I2)

- Arming policy (`monitor::MonitorArming`), read from Kconfig / the build
  only, never the contract: `CONFIG_NROS_MONITOR_ARM_ON_CALL` (default off =
  armed at the first spin, the old behaviour) and
  `CONFIG_NROS_MONITOR_ARM_GRACE_MS` (default 0; a deadline after the first
  spin).
- The application arms on entering RUN: `nros_monitors_arm()` (C),
  `nros::arm_monitors()` (C++), `nros::monitor::request_monitor_arming()` or
  `Executor::arm_monitors()` (Rust); per executor
  `set_monitor_arming` / `nros_cpp_executor_set_monitor_arming`.
- Before arming a verdict is counted (`violations_suppressed_before_arm`, the
  SWD record's `suppressed_before_arm`) but not stored, logged, sunk or
  traced. Arming reopens the rate/age/silence windows, discards latency and
  age measured before it, clears the release-jitter statistics, and re-reports
  a stack low-water mark already past its minimum.

Not fixed here: the two `rate-hierarchy-runtime` rows on on-demand topics are
a contract question (an on-demand topic has no rate), answered by
play_launch's on-demand-topic key, not by arming.
