---
id: 2887
title: "The stack-floor drift row for the Rust action server on nucleo-f767zi-zephyr no longer links: RAM overflows by 77,440 bytes"
status: open
type: bug
area: [stack-floor, zephyr, memory]
severity: high
found: 2026-11-23
related: [phase-590]
---

## What fails

The nightly's `stack-floor drift (observed + drift rows)` job (run 44988007873, 2026-11-22, main
`98b5d8e0c7`) reports `7 row(s), 1 failed`:

```text
== action-server/rust/zenoh@nucleo-f767zi-zephyr OPEN (recorded static 69264 B, [[observed]])
  FAIL: ... the image could not be walked: `nros build` exited 1 before any walk
  ld.bfd: zephyr/zephyr_pre0.elf section `noinit' will not fit in region `RAM'
  ld.bfd: region `RAM' overflowed by 77440 bytes
```

The talker row on the same board (69,648 B recorded) still links.

## Lead

77 KB is far more than a stack-floor movement. It is a RAM budget change on a 512 KB part; the
node-interfaces default and its parameter store (fragments `+phase-556-w13-*`) are the first
suspects. `nros build` now explains a link overflow (phase-588), so a local build of the row
names what the image reserves. Issue 2795 is the separate report-path item for this job.

## Acceptance

The row links and is walked in a scheduled nightly, or it is retired with a reason. A release
blocker: [phase-590](../roadmap/phase-590-nros12-release-plan.md) W1.
