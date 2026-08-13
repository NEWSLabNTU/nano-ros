---
id: 565
title: "NuttX Rust, BOTH arches: the 100 ms low tier is never scheduled — `/telem` never reaches 5 deliveries"
status: open
type: bug
area: boards
related: [issue-0564, issue-0263, issue-0246, phase-281, phase-285]
---

## Symptom

`cargo nextest run -p nros-tests --test realtime_tiers_e2e`, with issue 0564's
timeout budget in place so the sweep actually completes:

```
realtime_tiers: 2 of 16 row(s) FAILED:
  nuttx-arm/rust:   [nuttx-arm rust] low-tier /telem never reached 5 deliveries —
      the low tier was not scheduled
      (phase-281 W3-nuttx: QemuArmVirt::run_tiers (std::thread per tier),
       the cell that completed the 12-cell Model-1 matrix)
  nuttx-riscv/rust: [nuttx-riscv rust] low-tier /telem never reached 5 deliveries —
      the low tier was not scheduled
      (phase-285 W6 / #165: QemuRvVirt::run_tiers)
```

14 of 16 rows pass, including every zephyr, freertos, threadx and native cell,
and including the NuttX **C/C++** cells. Only the two NuttX **Rust** rows fail,
and they fail identically on both arches.

## Why it was not seen before

It was hidden behind issue **0564**: the consolidated test ran on the default
60 s kill while needing 127–204 s, and rows are evaluated IN ORDER, so these
cells were never reached. The verdict said `TIMEOUT`, which says nothing about
the rows it never ran.

So this is not necessarily a NEW regression — it may have been failing for as
long as the budget has been wrong. Establishing when it started is part of the
work, and a bisect here must account for the truncation: any revision whose run
was killed before these rows reports TIMEOUT, not PASS, and must not be read as
good. (Issue 0268's lesson, in a different mechanism: when a bisect's first-bad
looks implausible, the test was tracking a confounder.)

## What the shape suggests

The high tier runs; the low tier does not. Both arches, Rust only, C/C++ fine on
the same boards — so it is unlikely to be the board's tier plumbing as such, and
more likely the Rust `run_tiers` path specifically: `QemuArmVirt::run_tiers`
(std::thread per tier, phase-281 W3-nuttx) and `QemuRvVirt::run_tiers`
(phase-285 W6 / #165).

Adjacent and RESOLVED, worth reading before starting: issue **0263** — "NuttX
Rust arm: spawned tiers get their stack but not their priority off the sporadic
path". Same family (spawned tiers on NuttX Rust), different failure: 0263 was a
wrong PRIORITY, this is no scheduling at all.

## Where to look

* `QemuArmVirt::run_tiers` / `QemuRvVirt::run_tiers` — does the second tier's
  thread get spawned at all, or spawned and never given a timer?
* the 100 ms tier specifically: the 10 ms `/ctrl` tier is observed fine, so
  whatever fails is not the tier mechanism in general.
* `nros_platform_clock_ns` landed the same week (RFC-0073 / phase-352) and
  phase-352 W6 retired the ms/us wrappers outright. A 100 ms period is exactly
  where a unit or resolution error would strand a timer while a 10 ms one keeps
  firing — worth ruling in or out FIRST, since it is cheap and the timing lines
  up. Issue **0532** (platform clock ABI unit and resolution) is the same area.

## Acceptance

* both NuttX Rust rows pass `realtime_tiers_e2e`, i.e. `16 of 16`;
* whatever the cause, the fix names which of the two arches' `run_tiers` paths
  was wrong and why it affected only the Rust lane.
