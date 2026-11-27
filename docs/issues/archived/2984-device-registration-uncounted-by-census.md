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
resolved_in: "the change that archived it"
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

To check while fixing: the Rust `Node::create_guard_condition` route calls
no hook either. If the census misses a Rust guard too, it belongs to the same
fix (raised with the census owner on 2026-11-27).

## Resolution (2026-11-27)

Fixed at the root, together with issue 2985. The class was wider than the
device: every route that registers a timer, guard condition or device
directly on `nros-node` reached no census hook. That covered the Rust
executor primitives (`register_timer*`, `register_guard_condition*`,
`register_device*`), the `Node` methods, a direct device registration, and the
C graph-change guard (`rclc_node_get_graph_guard_condition`, which called no
hook at all). An observer on those entry points would have double-counted,
because the C and C++ shims call the same primitives.

So the slot is now reported once, from the executor's registration funnels
that every route crosses (`register_timer_entry_with`,
`register_guard_condition_on`, `register_device_on`), through
`registration_observer::ObservedCallbackSlot`. The per-language hooks are
gone: the C and C++ timer and guard hooks, the declarative Rust timer hook,
the C device hook, and the public `census_hooks::on_timer_create`,
`on_guard_condition_create` and `on_device_create`. The recorder entry point,
`census_hooks::on_callback_slot`, is `pub(crate)`, so no API crate can count a
slot twice. A profile labels the slot from the same funnel, with the same
sequence, so the two files still join.

The W4 half this issue planned (the companion package's Rust routes calling
`on_device_create`) is no longer needed: those routes reach
`register_device_on` like every other.

Proof:
- `nros-cpp`'s `every_route_records_each_callback_slot_exactly_once` drives a
  C++ timer and guard, a C timer, guard and device, and a Rust executor
  timer, guard and device, and asserts exactly eight rows.
- `nros-rmw-metadata`'s `imperative_timer_census` covers the Rust `Node`
  methods (one row each) and a declarative component (its one timer is one
  row).
- Mutations: dropping the device or timer funnel report turns those tests red.
- `check-census-hooks-complete` now derives the funnels (every body installing
  a timer, guard or device entry must report). It refuses a slot hook in any
  API source, an observe call in a `Node` method, and a `pub` recorder entry
  point, with 14 mutations red.
