---
id: 2162
title: "Under the default `Trigger::Any`, a triggered guard condition is not dispatched while any subscription or service is idle"
status: open
type: bug
area: [core, executor]
severity: medium
found: 2026-10-30
related: [2144]
---

## What

`spin_once` builds `non_timer_mask` from every entry that is neither a timer
nor a guard condition
(`packages/core/nros-node/src/executor/spin.rs`, the readiness scan:
`if !matches!(meta.kind, EntryKind::Timer | EntryKind::GuardCondition)`), and
evaluates the default trigger as

```rust
Trigger::Any => bits & non_timer_mask != 0 || non_timer_mask == 0,
```

So a guard condition's readiness never makes `Any` pass. When the executor has
at least one subscription, service or client, a triggered guard is dispatched
only in a spin where one of those ALSO has data. The `!trigger_passes` branch
sweeps timers only, so the guard's flag stays set and its callback waits for
unrelated traffic, possibly forever.

Measured while writing issue 2144's test (zenoh, `nros-tests`, one node):

| entries | trigger, then `spin_once(20 ms)` | guard callback ran |
|---|---|---|
| guard only | | yes |
| subscription (idle) + guard | | **no** (flag still set after the spin, and after a further 50 ms spin) |

Same on `origin/main` before 2144's change, so it is not the notifier.

rclc's `rclc_executor_trigger_any` returns true when ANY handle has
`data_available`, guard conditions included. nano-ros leaves timers out on
purpose (they are swept whether or not the trigger passes); guard conditions
get neither treatment. `Trigger::All` leaving guards out is arguably right (a
rarely-triggered guard would block every spin), `Any` leaving them out is not.

Every existing guard test registers the guard ALONE (`trigger_conditions.rs`,
`wake_latency.rs`, `xrce_guard_wake.rs`), so `non_timer_mask == 0` lets `Any`
pass and the defect never shows.

## Direction

Count a ready guard condition toward `Any` (and the `!trigger_passes` sweep, or
an equivalent), mirror rclc, and add a test with an idle subscription beside
the guard — the shape every current test avoids. Check the C `rclc_executor`
trigger helpers (`rclc_executor_trigger_any`) agree.
