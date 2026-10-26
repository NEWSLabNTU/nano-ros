---
id: 2075
title: "A Rust component cannot start a timer it declared stopped: no declarative reset / cancel, so autostart = false is useless"
status: resolved
resolved: 2026-10-26
type: enhancement
area: [api]
severity: low
found: 2026-10-26
related: [2038, phase-488]
---

## What

Issue 2038 gave declared timers rcl's `autostart` flag:
`create_timer_for_callback_name_with_autostart(name, period, clock, false)`
registers the timer cancelled. Nothing a component can reach could start it.
`Executor::reset_timer` / `cancel_timer` take a `HandleId`, and a declared
timer is only an `EntityId`. The component cell kept no id-to-handle table,
and a callback does not hold the executor. So `autostart = false` was useful
only to an image that reset the timer through the executor itself
(2038's Still open).

Upstream, the same component would call `timer->reset()` / `timer->cancel()`
(rclcpp `TimerBase`) or `Timer::reset` / `cancel` (rclrs) from any callback.

## Done when

- A component can reset (start) and cancel a declared timer by id, from a
  callback and from `tick`.
- A runtime test shows a stopped timer started from another callback, then
  firing. Mutation-checked.

## Resolution

Fixed 2026-10-26.

- **API.** `CallbackCtx::reset_timer(EntityId)` / `cancel_timer(EntityId)`
  and the same pair on `TickCtx` (`packages/api/nros/src/node.rs`). They are
  rclcpp's `TimerBase::reset()` / `cancel()`, addressed by the timer's
  entity id (for the `..._for_callback_name` verbs, the callback name). An id
  with no controllable timer answers the new `NodeDeclError::UnknownTimer`,
  whose message names the bound.
- **Where the request goes.** `PublisherResolver` gained a hidden, defaulted
  `request_timer(id, TimerOp)`. Both contexts already reach back through the
  resolver, so no context constructor changed. A resolver with no timer table
  answers `UnknownTimer`. The component runtime's `CellResolver` records the
  op in the cell.
- **The table.** `ComponentCell` has a sixth registry,
  `timers: heapless::Vec<TimerSlot, TIMS>`. Each row is an FNV-1a 64 hash of
  the entity id, the executor `HandleId`, and the pending op. It is a hash,
  not an `IdStr`, because an id string is 128 bytes. `node_runtime` files each
  declared timer's handle when it registers it. Two ids that collide under the
  hash are a loud registration error (the metadata already refuses a duplicate
  id).
- **When it applies.** `tick_one_cell` calls `apply_timer_ops` before the
  component's tick, for ops the pass's callbacks requested, and again after
  it, for ops the tick requested. The ops are the executor's own
  `reset_timer` / `cancel_timer`. A reset re-seeds a clock timer's reading,
  so a timer declared stopped starts a fresh period.
- **The bound.** `EntityBounds` gained `timers`, emitted by `nros::node!` as
  `ComponentSlotStorage`'s new seventh const parameter. `knob_caps()` gives it
  `NROS_RUNTIME_MAX_CELL_ENTITIES`, the same as every other kind.
  `exact(..)` keeps its five positional arguments and sets it to 0, so the
  in-tree classes that state exact bounds pay nothing for a table they never
  asked for. Their timers run as before. `with_timers(n)` raises it. A timer
  past the bound still registers and fires; it just cannot be controlled by
  id. That departs from the other registries, where overflow is a
  registration error: those entities cannot work without their row, and a
  timer can.

**Tests.**

- `component_runtime.rs::a_declared_stopped_timer_is_started_from_another_callback`
  (`component-runtime-test`, real zenohd, 40 passes of 10 ms):
  - an armed `on_kick` resets the stopped `on_late` on its 3rd activation;
    `on_late` had fired 0 times by then;
  - `on_late` then fires and cancels itself on its 3rd activation, and stays
    at exactly 3;
  - `tick` cancels `on_kick` after 6 activations, and it stays at the count
    the tick saw;
  - an undeclared id answers `UnknownTimer`.
- Unit tests:
  - `node.rs::timer_bounds_default_to_none_under_exact_and_name_the_bound`;
  - `node_runtime.rs::a_declared_cell_pays_for_what_it_declares` now also
    checks that a timer row costs at most 32 B an instance.

**Mutations (measured).**

- `apply_timer_ops` not calling the executor: `on_late` never fires, so the
  `late == 3` check fails.
- No apply after `tick`: `on_kick` fires past the tick's cancel.
- Timers filed under the wrong key: `reset_timer` is refused.

## Still open

- The ops apply at the component's next tick, not at the call. A callback
  cannot reach the executor, so a reset requested in a callback takes effect
  after that pass's dispatch. Upstream's `reset()` is immediate.
- Only the single-node component runtime (`node_runtime`, both the
  `register_node` and the `nros::node!` install paths) carries a timer table.
  A resolver with none (the RTIC board's, a test's) answers `UnknownTimer`.
- No `is_canceled` / `time_until_trigger` query from a context yet; the
  executor has both (`timer_is_canceled`, `timer_time_until_next_call_ns`).
