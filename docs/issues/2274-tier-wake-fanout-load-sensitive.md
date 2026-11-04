---
id: 2274
title: "tier_wake_fanout's threaded fan-out bound fails under the gate's parallel load and passes solo"
status: open
type: bug
severity: low
area: [testing, executor]
related: [phase-521]
found: 2026-11-05
---

## What

`nros-tests::tier_wake_fanout a_non_attributing_backend_wakes_every_high_tier_and_each_wake_is_bounded`
failed once inside `just ci gate` (`check::build` → `required-features-tests`,
the `trigger-test` feature lane) on a docs-only branch, in 0.97 s. The same
binary, run alone six times right after (`cargo nextest run --cargo-profile
nros-relwithdebinfo -p nros-tests --features trigger-test --test
tier_wake_fanout`), passed six times. Two earlier gates on the same host that
day passed it.

The failure text was not kept: the lane's log shows only the FAIL line, so it
is not known which of the test's two assertions fired.

## Why it is load-sensitive

The test measures a deployment-shaped cost on real threads: every tier parks
in its own thread, 2 ms-spaced arrivals land on the low tier, and it asserts

- at least `0.9 × (TIERS - 1)` high-tier wakes per arrival, and
- under 100 µs of thread CPU per spurious wake.

Both depend on the host scheduler. Under `check::build`'s parallel lanes a
parked thread may not run between two arrivals (fewer wakes than arrivals,
so the first assertion fails), and a wake that is switched out mid-pass is
charged more CPU time (the second). Neither is a property of the executor.

## What a fix looks like

Make the measured quantity independent of whether the host schedules the
thread in time, rather than retrying: for example count wakes from the
session's fan-out counter instead of from parks that ended, and measure the
per-wake cost from a pass the test drives itself. Or run this one test in a
nextest group with no parallel neighbours, if the bound is meant to be a
real-time property of an idle host. Say which in the RFC-0113 D2.5 text.
