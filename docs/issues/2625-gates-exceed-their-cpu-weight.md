---
id: 2625
title: "Weight-1 gates in check::build run up to 22 processes; no fix until one measures as a problem"
status: open
type: tech-debt
area: [ci, testing]
severity: low
found: 2026-11-16
related: [phase-559]
---

**Owner:** nros 175. **Home:** phase-559. The ruling (manager, 2026-11-16):
file these as one issue and fix none of them until one is measured as a
problem.

## What happens

The gate harness gives each check::build gate one slot of its nproc-wide
`--test-threads` unless the recipe says `[group('cpu:<n>')]`
(`threads-required = n`, `scripts/check/check-gate-lists.py`). Only
`linux-check` (`cpu:8`) and, since this issue's sibling fix, `cli-tests`
(`cpu:4`) say so. Every other gate counts as one CPU, whatever it spawns.

phase-489 W13's token pool reports the gap per run, in its "ran more
processes than their weight" line. Two solo gates on newslab-175 (20 cores),
W13 branch, check::build:

| Gate | Peak runnable, run 1 | Peak runnable, run 2 | Weight |
|---|---|---|---|
| `scaffold-builds#c_project_talker` | 12 | **22** | 1 |
| `required-features-tests` | **20** | — | 1 |
| `workspace-features` | **18** | — | 1 |
| `cpp` | **16** | — | 1 |
| `template-copy-out#local-msg-package` | 8 | 15 | 1 |
| `scaffold-builds#cpp_project_server` | — | 15 | 1 |
| `scaffold-builds#rust_project_client` | 13 | 6 | 1 |
| `template-copy-out#multi-package-workspace` | — | 11 | 1 |

A dash means the gate was not in that run's top 15; the report then names
"… and N more", 28 and 18. Run 1 counted 43 over-weight gates and run 2
counted 33. Peaks vary between runs because they depend on what runs beside
them.

## Why it is not fixed yet

`cli-tests` had the same shape, and it was a problem: it hit the harness's
2400 s kill on loaded hosts (139 ×3, 243 ×1). It was fixed by MEASURING
where its time goes. Its test run, idle, took 530 s at nproc threads, 523 s at
4 and 596 s at 2, so the weight and the thread cap were set at the knee, 4.
None of the gates above has been reported timing out or slowing the lane, and
a weight raised without that measurement just takes slots from other gates.

## When to pick it up

When one of these gates times out, or shows up as the long pole of
check::build, measure it the way `cli-tests` was measured: idle wall time
against its own parallelism setting. Then set `cpu:<n>` at the knee, with a
matching cap on whatever it spawns. A gate's peak in the pool report is where
to look, not the size of the fix.
