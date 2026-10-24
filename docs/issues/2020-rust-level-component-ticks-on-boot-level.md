---
id: 2020
title: "On a Rust level image every component's tick runs on the boot (least urgent) level"
status: open
type: limitation
area: [executor]
severity: low
found: 2026-10-24
related: [phase-509, rfc-0108, 2016]
---

## Measured

`nros::levels::LevelCore` (phase-509 P10) shares one `ExecutorNodeRuntime`
across the levels. Spawned levels run band passes
(`Executor::spin_band_once`), which by design skip the executor-wide chores
(RFC-0108 "As landed in item 2"). The runtime's per-component `tick` runs in
`ExecutorNodeRuntime::spin_once_shared`, which only the boot level calls.

So a component registered by an urgent level ticks at the least urgent
level's priority and period. That covers its service-client polls and its
action-server completion and feedback.

Callbacks are not affected: subscriptions and timers dispatch on the level
that registered them.

## Closes when

A component's tick runs on the level that registered it. For example, the
runtime keeps a per-level list and each band pass ticks its own. A Rust level
fixture must show an urgent level's action server completing at its own
level.
