---
id: 3028
title: "Two stack-floor rows record a static walk about 33 % above what main walks, so the drift check cannot see a rise of up to 35 KB on them"
status: open
type: bug
severity: medium
area: [memory, tooling]
related: [issue-2400, issue-2402, issue-2550, issue-2783, issue-3027, phase-557]
found: 2026-11-29
---

## What happens

`just zephyr stack-floor-drift` holds each `[[observed]]` / `[[drift]]` row's
walk to the row's recorded `static`. A walk that rises more than
`drift_rise_pct` (2 %) over it fails. A walk that falls is a note and never
fails.

Two rows record a `static` far above what main walks today. Main's
`scripts/nros-stack-floor.py` walked each image, built on host 133 on
2026-11-29 with the walk cache off (`NROS_STACK_WALK_CACHE=0`):

| row | recorded `static` | main walks | gap |
| --- | ---: | ---: | ---: |
| `action-server/rust/zenoh@nucleo-f767zi-zephyr` | 103,240 | 69,448 | -32.7 % |
| `action-server/rust/zenoh@mps2-an385-zephyr` | 108,152 | 72,924 | -32.6 % |
| `pubsub_bringup@zephyr_cortex_m_talker_clock_sntp_resync` (SNTP_RESYNC) | 90,728 | 83,580 | -7.9 % |

The other rows sit within 1 % of what main walks.

On those rows the drift check is blind. The action-server walk could grow by
about 35 KB (to 105,304 B on nucleo, 2 % over the stale number) and the
check would print nothing but a note that the walk "fell".

## Why nothing else noticed

The drift check reports a fall as a note: "re-measure the row to reclaim
the RAM if the fall is real". Nothing turns a note into work, so a row only
gets re-measured when someone reads the log.

The note's advice is also wrong for these rows. Every `[[observed]]` row
carries `upper_bound`, so `nros new` stamps the template's stack from the
row's measured `peak` plus the margin, not from `static` (`recorded_floor`
in `packages/cli/nros-cli-core/src/cmd/new_workspace.rs`, issue 2783). A
stale-high `static` therefore costs no RAM in a fresh project. What it costs
is the drift check's sight. Each build still checks its own stack against
its own walk, so a real image is not under-checked. The lane that watches
the walk over time is the one that stops seeing.

## Fix

The check should not be able to hold a row to a number its own walk no
longer reaches. Options, one for a ruling:

1. **Re-baseline.** Re-walk the three rows and record what main walks, with
   the date and host in `method`, as every earlier re-walk did. This is
   cheap and fixes today, but not the next drift.
2. **A two-sided band.** A fall beyond some percentage also fails, naming
   the row and the re-measure commands. Issue 2550's re-cut moves a walk a
   few KB either way, so the fall band has to be wider than the 2 % rise
   band (the falls measured here are 8 % and 33 %; the re-cuts in issue
   3027's table are under 4 %).
3. **Report a fall as work.** The nightly lane lists every row whose walk
   fell beyond a band, in the job summary, without failing it.

Issue 3027's change (relocation-backed image pointers) re-baselines no row,
and moves none of these three by more than 644 B on the same build.

## Acceptance

- No row's recorded `static` is more than the chosen fall band above what
  main walks.
- A future fall beyond that band is reported by the lane in a form that
  cannot be missed (a failure, or a job-summary row), per the ruling.
