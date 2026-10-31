---
id: 2162
title: "Under the default `Trigger::Any`, a triggered guard condition is not dispatched while any subscription or service is idle"
status: resolved
resolved: 2026-10-31
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

## Resolution (2026-10-31)

`spin_once`'s readiness scan now keeps a `guard_mask` beside
`non_timer_mask`, and the default trigger reads

```rust
Trigger::Any => bits & (non_timer_mask | guard_mask) != 0 || non_timer_mask == 0,
```

so a triggered guard passes `Any` the way a subscription with data does, as
rclc's `rclc_executor_trigger_any` counts every handle. An IDLE guard still
holds nothing back (it is not in `non_timer_mask`), so `All`, and the
timer/guard-only `non_timer_mask == 0` arm, are unchanged. `Trigger::All`
still leaves guards out, as the issue argued.

There is one evaluation site, and every language reaches it: Rust's default,
C++, and C's `rclc_executor_set_trigger(exec, NULL, …)` all map to the
internal `Any`. A C user who installs `rclc_executor_trigger_any` explicitly
already agreed with rclc, because the bridge's readiness array is the
snapshot `bits`, which always carried the guard's bit.

Regression test
`executor::tests::a_triggered_guard_beside_an_idle_subscription_fires_under_trigger_any`
(`node-std-tests`), the shape every earlier guard test avoided: an idle
subscription registered beside the guard, on the default trigger and on `Any`
set explicitly. Negative control: with the old `Any` expression the test fails
at the guard assertion (`explicit=false: the guard must fire although the
subscription is idle`). The Verus model gains `trigger_any_guarded` with two
proofs (a ready guard passes; with no ready guard it equals `trigger_any`;
not run by `just verify-verus` here, Verus is not provisioned in this box), and
the `Trigger::Any` doc says what passes it.
