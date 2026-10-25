---
id: 2038
title: "The declarative nros::Node timer has no autostart flag and no call info; only the imperative node and rclcpp layers got issue 1974's surface"
status: resolved
resolved: 2026-10-25
type: enhancement
area: [api]
severity: low
found: 2026-10-25
related: [1974, phase-488]
---

## What

Issue 1974 added Jazzy's timer call info (`expected_call_time` /
`actual_call_time`) and the `autostart` flag to three surfaces:

- the executor (`Executor::register_timer_with_info`);
- the imperative Rust node (`Node::create_timer_with_info(period, autostart, cb)`,
  `packages/core/nros-node/src/executor/node.rs`);
- C++ (`create_wall_timer(period, cb, group, autostart)`, `rclcpp::create_timer`,
  `TimerInfo`).

The DECLARATIVE surface a component uses has neither. That is the
`nros::DeclarativeNode` extension trait on `nros::Node`
(`create_timer_for_callback_name`, `..._on_clock`, `packages/api/nros/src/node.rs`).
A component timer always starts armed, and its callback is not told when it
was due or how late it ran.

Ledger: `rust:DeclarativeNode::create_timer_for_callback_name` and its `_on_clock`
sibling (divergence: entities are declared) do not mention either facility;
`rust:Node::create_timer_with_info` covers the imperative node only.

## Done when

- A declared timer can be declared stopped (an autostart flag in its
  declaration, carried into the model).
- A declared timer's callback can receive the call info.
- A unit test covers each.
- The two declarative rows name the result.

## Resolution

Fixed 2026-10-25. One shared core, no second implementation.

- **Model.** `EntityMetadata::timer_autostart`, default `true`, sits beside
  `timer_clock`. Like `timer_clock` it is read by the runtime from the
  component's `declare()` and is not emitted in the metadata JSON.
- **Declaration.** Every timer verb on `DeclarativeNode` now funnels into one
  hidden declaration, `declare_timer_with_autostart`. The public spelling is
  `create_timer_for_callback_name_with_autostart(name, period, clock, autostart)`,
  the `_on_clock` verb plus one `bool`.
- **Runtime.** `node_runtime` registers every declared timer through
  `Executor::register_timer_with_info(period, clock, autostart, ..)`, the call
  `Node::create_timer_with_info` makes. It builds the same arena entry
  `register_timer_on_clock` built, so armed timers behave as before.
  `autostart = false` registers the timer cancelled.
- **Call info.** The dispatcher's `TimerCallInfo` reaches the component through
  `dispatch_timer_into_cell`, which sets it on the `CallbackCtx`. The callback
  reads it as `ctx.timer_info()`: `Some` in a timer callback, `None` elsewhere.
  `nros::TimerCallInfo` is re-exported.

**Tests.**

- `component_runtime.rs::declared_timer_reports_call_info_and_honours_autostart`
  (`component-runtime-test`, real zenohd). An armed declared timer fires at
  least three times and every activation carries call info (expected <= actual,
  steady clock). Its `autostart = false` sibling never fires in the same window.
- `node.rs` unit test `callback_ctx_timer_info_is_attached_by_the_dispatch_site`.

**Negative controls (measured).**

- With `let autostart = true;` in `node_runtime`, the stopped timer fires 8
  times and the test fails.
- With `ctx.set_timer_info` removed, "0 of 8" activations carry info and the
  test fails.

The ledger rows `rust:DeclarativeNode::create_timer_for_callback_name` and
`..._on_clock` now name the result. New rows: `rust:TimerCallInfo` and
`::lateness_ns`.

## Still open

- **A component cannot START a timer it declared stopped.** No declarative
  handle reaches `Executor::reset_timer` or `cancel_timer`. A `NodeTimer` is an
  entity id, and the component cell keeps no timer `HandleId` registry. That
  needs a per-cell timer table (a new cell capacity, emitted by `nros::node!`)
  plus a `TickCtx::reset_timer(EntityId)`. Until then `autostart = false` is
  useful only to an image that resets the timer through the executor itself.
- `timer_autostart` is not in the metadata JSON sidecar, which `timer_clock` is
  not in either. A model consumer outside the runtime cannot see it.
