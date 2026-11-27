---
id: 2984
title: "A device registration claims a callback slot that the census and the metadata probe do not count"
status: resolved
type: bug
severity: medium
area: [executor, sizing, census]
related: [phase-579, rfc-0139, rfc-0120]
found: 2026-11-26
resolved: 2026-11-27
resolved_in: "phase-579-w4a1-nros-device"
---

## What

Phase 579 W3a adds `Executor::register_device`, which claims one executor
callback slot (`next_entry_slot`) for a device's continuation. Two of the
three ways an image's callback count is known now count it:

- **Declared entities** count it. `device` is an `EntityKind` in
  `nros-cli-core`'s entity inventory with `callback_slots() == 1`, and
  `check-entity-slot-costs` holds that to `register_device_on`.
- **The census and the metadata probe** do not. Those record a callback slot
  only through a hook in `nros::census_hooks`, which the API layer calls at
  registration: `on_guard_condition_create` is called by the C and C++ guard
  routes. `nros-node` sits below `nros` and cannot call those hooks, so a
  registration made directly on the executor records nothing.

An image whose callback-slot count comes from its census or probe sidecar
would therefore size `NROS_EXECUTOR_MAX_CBS` one slot short per device, and
fail with `ExecutorFull` at the device's registration.

## Who can hit it today

No one. `register_device` is a layer-1 extension hook (RFC-0139 D1). The only
caller in the tree is the `isr-wake-freertos-mps2` test image, which uses
the executor's default slot capacity and is sized by no census. Devices reach
applications through the companion package (phase 579 W4), whose
registration routes do not exist yet.

## Fix

Split between phase 579 W3a and W4, agreed with W4's owner:

- **W3a (this PR's half).** `census_hooks::on_device_create` records a
  `timers[]` row with `kind: "device"` (`TimerKind::Device`), and the census
  reader and the CLI's strict sidecar schema (`SourceTimerKind`) accept the
  word. The C layer-1 registration, `nros_executor_register_device`, calls the
  hook itself, so a C image and W4's C and C++ routes are each counted exactly
  once. The arena model prices a declared device on its guard-condition row
  (`arena_priced_count`), and `nros-node` asserts a device entry fits that
  row's fixed allowance.
- **W4.** `nros-device`'s Rust registration calls `on_device_create`, because a
  Rust route that registers on the executor directly is below `nros` and
  cannot reach the hook.

The issue resolves when W4's Rust route lands.

## Resolved (2026-11-27)

W3a's half landed with #4936. W4's half lands with the `nros-device` crate:
its two Rust registration routes, `Stage::open` and `Source::open`, each call
`nros::census_hooks::on_device_create` once, right after
`Executor::register_device` claims the slot. So every route that claims a
device slot is counted once: the C entry point inside
`nros_executor_register_device`, and the Rust routes in the companion crate.
The C and C++ layer-2 routes (phase 579 W4a-2) build on the C entry point and
add no second call.

The Rust `Node::create_guard_condition` route calls no hook either. Whether
the census misses a Rust guard condition was raised with the census owner on
2026-11-27. It is a separate question from this issue, since no device
registration is involved.
