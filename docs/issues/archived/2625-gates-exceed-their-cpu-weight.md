---
id: 2625
title: "Weight-1 gates in check::build run up to 22 processes; no fix until one measures as a problem"
status: resolved
type: tech-debt
area: [ci, testing]
severity: low
found: 2026-11-16
related: [phase-559, 2683, 2718]
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

## Resolved (2026-11-19, nros 139)

A gate timed out: `workspace-all` was killed at 2400 s on hosts 139 (twice,
cold) and 175, all on trees that already had `workspace-probes`' `cpu:6`
(issue 2683). So this was picked up, and the measurement changed the fix.

**A measured peak is not a weight.** Per-gate peak runnable across 9
build-tier runs on host 139 (32 CPUs, a 32-token pool): `workspace-features`
2 to 54, `scaffold-builds` 12 to 31, `cli-tests` 2 to 32 (its tests are capped
at 4), `workspace-all` 5 to 24. On a 48-core host the same gates peak at
39 to 42. A peak counts the pool tokens a gate's cargo held at its busiest
moment, so it scales with the host and the moment. Weights derived from it
would vary by host, and a check failing on it would be a flake.

**The rule instead.** Every build-tier gate states its weight AND how it was
chosen: `[metadata("cpu-basis", "<kind>", "<how>")]` beside
`[group('cpu:<n>')]`. The number stays in the group, the one source that
`check-gate-lists.py --weight` and the nextest config read.

- `knee`: measured, with the numbers. `cli-tests` 4, `workspace-probes` 6.
- `default`: a parallel build whose knee is not measured: exactly
  `DEFAULT_BUILDER_WEIGHT`, 2.
- `light`: runs one thing at a time, weight 1, with the reason. 8 gates, each
  peaking at 1 in most of the 9 runs.

`check-gate-lists` (fast line) fails on a build-tier gate with no basis, a
`default` that is not 2, or a `light` that is not 1. Its self-test covers each
case. `linux-check` declared `cpu:8` as a time-budget fix, not a measurement;
no unit of it ran more than 4 threads in 9 runs, so it takes the default.

**Choosing the default**, cold `just check build` on host 139, same tree
otherwise:

| default weight | total | slowest gate | timeouts | lane runnable p50 | free tokens p50 |
| --- | --- | --- | --- | --- | --- |
| 1 (before) | 2234 / 2901 / 3196 s | `workspace-all` killed | 0 / 1 / 1 | 2-3 | 21-27 |
| 4 | 4431 s | `cli-tests` 1930 s | 0 | 1 | 28 |
| 2 | 3185 s | `cli-tests` 2016 s | 0 | 2 | 28 |

At 4 the tier ran too few gates at once, so the lane sat mostly idle. At 2 it
takes about as long as before with no timeout: `workspace-all` ran in 235 s
instead of being killed. One run per row, and the same configuration
measured +43 % three hours apart, so the 2 row needs more runs before it can
be called faster. It is not slower, and it does not time out.

**A peak above the whole pool** is a different fault: the pool cannot have
granted it. The load sampler now records, with `--pool`, each sample in which
a gate runs more threads than the pool, with its top commands. The report
names them; it does not fail, because whether it happens depends on timing.
Its first catch: `cli-tests` at 109 runnable, 101 of them `rust-lld`, the host
linker running a thread per CPU outside the jobserver. That is issue 2718.
