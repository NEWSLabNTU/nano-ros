---
id: 1581
title: "realtime-rust's FreeRTOS and ThreadX images state no locator, so they dial the board default and every tiered test on them fails before a tier runs"
status: open
type: bug
area: [examples, testing, freertos, threadx]
severity: medium
found: 2026-09-29
related: [issue-1571, phase-383]
---

## What happens

In `examples/workspaces/realtime-rust/src/demo_bringup/system.toml`, the
`[image.freertos_realtime]` and `[image.threadx]` tables give no `locator`.
On `main` only `[image.zephyr]` states one (`tcp/10.0.2.2:7471`). Both images
therefore bake the board default, `tcp/192.0.3.1:7447`.

The tests start their router elsewhere: port 7891 on FreeRTOS, and
`tcp/127.0.0.1:9091` on threadx-linux. Every Rust tiered cell on those images
therefore fails with `Executor::open failed: ConnectionFailed` before any
tier code runs:

- `realtime_tiers` freertos/rust and threadx-linux/rust;
- `sched_dims_applied_e2e` threadx-linux/rust CorePin, PreemptThreshold and
  TimeSlice.

These were observed by the issue-1571 agent while verifying the Rust
tier-backing change. The FreeRTOS image booted fine by hand against a router
on 7447. The missing tables were confirmed by reading `main`.

## Cause

The locator was lost when phase-383 W10.a (`f1fc1202c`) migrated the
realtime workspaces. The comment at line ~182 of the same file notes that
"the locator was the deleted package's".

## Acceptance

- Each image table states the locator its test's router uses, or the tests
  read the image's baked locator rather than assuming one.
- The five cells above reach tier code.
- Check the sibling realtime-c and realtime-cpp bringups for the same gap.
