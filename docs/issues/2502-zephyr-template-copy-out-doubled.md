---
id: 2502
title: "The Zephyr template copy-out job nearly doubled (19.5 → 37 min) in a week, and its step log no longer shows per-project time"
status: open
type: tech-debt
severity: low
area: [ci, build, templates]
related: [2219, rfc-0122, phase-559]
found: 2026-11-13
---

**Found by** the 2026-11-13 move of `check / template-copy-out [zephyr]` off
the merge queue (user ruling "Do Zephyr template check daily", RFC-0122 D9
amendment). The move took the job off the critical path, so its slowdown
would no longer be seen; this records it.

## Measured

`gate.yml` merge-group runs, conclusion success, job start to end (GitHub
jobs API). "step" is the `just check template-copy-out (the Zephyr part,
copied out)` step alone. Nov 6-11 are a sample of up to 8 runs per day.

| day | runs | job median | step median |
| --- | --- | --- | --- |
| 2026-11-05 | 4 | 22.2 min | 9.1 min |
| 2026-11-06 | 8 of 61 | 27.4 | 10.6 |
| 2026-11-07 | 8 of 82 | 24.8 | 10.7 |
| 2026-11-08 | 1 | 25.7 | 9.9 |
| 2026-11-09 | 8 of 96 | 34.7 | 18.8 |
| 2026-11-10 | 8 of 104 | 30.2 | 16.1 |
| 2026-11-11 | 8 of 52 | 28.8 | 16.8 |
| 2026-11-12 | 107 | 33.0 | 18.9 |
| 2026-11-13 | 13 | 35.1 | 21.0 |

phase-489 recorded a 19.5 min job median over 2026-11-04/05. The step is a
STEP CHANGE on 2026-11-09 (about 10 → 17-19 min), then a slower drift. The
rest of the job grew too, 13 → 14-16 min (container start, CLI build, west
workspace + SDK setup), but less.

Two runs compared step by step (minutes):

| step | 41874153706 (11-04) | 43339733451 (11-13) |
| --- | --- | --- |
| Initialize containers | 3.2 | 4.5 |
| Build the in-tree nros CLI | 4.3 | 6.6 |
| Zephyr 3.7 west workspace + SDK | 6.8 | 7.7 |
| copy-out (5 projects) | 9.3 | 23.3 |

Same five projects both times (`ament-workspace-port`,
`cpp-port-minimal-publisher`, `rclcpp-compat-smoke`,
`topic-state-monitor-port`, `zephyr-byo/app`). What differs:

- **Each project now builds one more artifact, under `build/census-host/`**
  (e.g. `build/census-host/minimal_publisher`): a host build of the program,
  run to take its entity census. On 2026-11-04 each project listed only its
  Zephyr ELF. The census pre-pass entered the leaf road on 2026-11-09
  (`77be5a5613`, `7ea99d1b07`, `ebe1658f19`, phase-530 W2), the same day as
  the step change. **This is the leading suspect, not a measured cause.**
- **The per-project lines are now printed together at the end** (all five at
  the same second), so the log no longer says which project took the time.
  On 2026-11-04 each line carried its own timestamp, about 1m45-2m05 apart.

## Next

1. Restore per-project timing in `check-template-copy-out` (print each line
   when its project finishes, or print its duration), so the next question
   answers itself from a log.
2. Build one project with and without the census pre-pass and price it. If
   the census is the cost, ask whether a copy-out check needs it at all (it
   proves the copy BUILDS; the census sizes pools for an image that is never
   run there), or whether the host build can share the target build's
   dependency compile.
3. The CLI build step grew 4.3 → 6.6 min and is shared by every gate job;
   price whether its cache is missing.
