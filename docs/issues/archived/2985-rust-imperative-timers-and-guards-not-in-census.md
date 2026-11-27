---
id: 2985
title: "A timer or guard condition Rust code creates through a `Node` method is not counted by the census, so a census-sized image is one callback slot short for each"
status: resolved
type: bug
severity: medium
area: [census, rust-api, executor]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "the change that archived it"
related: [2670, 2681, phase-463]
---

## What happened

Another session asked whether the Rust guard-condition route reaches the
census. A trace of main showed that it does not, and that the timer routes do not
either.

Timers and guard conditions never touch the RMW, so the census counts a
callback slot for one only when a hook reports it:

- The C routes (`rclc_executor_add_timer*`,
  `rclc_node_create_guard_condition`) and the C++ routes
  (`nros_cpp_timer_create*`, `nros_cpp_guard_condition_create`) call
  `nros::census_hooks::on_timer_create` / `on_guard_condition_create`.
- The declarative Rust route (`ExecutorSink::create_entity`, which
  `node.create_timer_for_callback_name` reaches in a component's
  `configure`) calls `on_timer_create`. It has no guard-condition form.
- The imperative Rust routes, `Node::create_guard_condition` and the whole
  `Node::create_timer_*` family (`_in_group`, `_repeating`, `_oneshot`,
  `_inert`, `_on_clock`, `_on_clock_in_group`, `_with_info`), live in
  `nros-node`, below `nros`. They called no hook and reported to no
  observer.

A Rust component reaches those methods through `NodeRuntime::executor_mut()`,
which its documentation offers for "custom guard conditions". A
census-sized image whose Rust code does that has a census one callback slot
short for each such timer or guard, and fails at start with
`NodeError::ExecutorFull`:

    the executor's callback table is full (NROS_EXECUTOR_MAX_CBS)

No in-tree census-sized image does this today. The Rust guard users in the
tree (the interrupt-wake test images) open their own executor outside a
census. So the defect is latent in the tree and live for a user.

`check-census-hooks-complete` did not catch it because its Rust rows were an
authored list, and the list named only the declarative sink and the
parameter routes.

## Workaround (nros12)

- Create timers with `node.create_timer_for_callback_name(..)` in the
  component's `configure`; the census counts those.
- For each guard condition, and each imperative timer you keep, raise the
  callback table above the `max_cbs` that `nros build` prints in its
  `resolved → …/resolved.toml (max_cbs <n>, …)` line:

      [image.<id>.knobs.executor]
      max_cbs = <n + extra>

  A stated knob overrides the derived value. The executor arena is derived
  from `max_cbs`, so it grows with it, unless `NROS_EXECUTOR_ARENA_BYTES` is
  pinned by hand, in which case raise that too.

The user ruled on 2026-11-27, relayed by the release session, that this is
not an nros12 blocker. It goes on the release page as a known issue, and the
fix ships after the release.

## Fix (branch `fix/rust-census-timers-guards`)

- `nros-node`'s `registration_observer` gains `observe_callback_slot`, the
  same shape as the node-create and parameter-declare observers. Every Rust
  `Node` timer and guard method reports through it, before the executor
  answers, so a slot the full table refuses is still counted.
- `nros::metadata_mode` installs the sink and routes each report to
  `on_timer_create` (with its kind and period) or
  `on_guard_condition_create`. All four routes then record alike.
- The observer sits only in the `Node` methods, never in the
  `Executor::register_*` primitives. Those primitives are what the C and C++
  shims and the declarative sink call, and those record through their own
  hook; observing there would count their slots twice.
- `check-census-hooks-complete` derives the Rust rows instead of listing
  them:
  - every `pub fn` in `node.rs` that calls a timer or guard primitive must
    call the observer;
  - every executor `register_timer*` / `register_guard_condition*`
    primitive must be listed with the callers that record for it, so a new
    one fails until someone decides.
  Two new self-test mutations prove each half.

Test: `packages/rmw/metadata/tests/imperative_timer_census.rs` drives every
Rust `Node` timer and guard method on an executor over the `metadata`
backend. It checks one `timers[]` row each, of its own kind, on its own
node. With the observer disabled, it fails.

## What stays open

A Rust caller of the `Executor::register_*` primitives themselves, rather
than a `Node` method, is still not counted. The node documentation calls
them the primitive the `Node` methods forward to, but they are `pub`.
Counting them needs the C and C++ shims to stop recording at the call site
and the primitive to record instead. That is a larger change, tracked here
until it is made.

## Resolution (2026-11-27)

Fixed with issue 2984, which found the same gap one level down: every Rust
executor primitive (`Executor::register_timer`, `register_guard_condition`
and the rest, the spellings in-tree Rust code actually uses), not only the
`Node` methods. An observer on the `Node` methods alone, which was this
issue's first fix, left those uncounted. One on the primitives would have
double-counted the C and C++ shims, which call them too.

The slot is now reported once, from the executor's registration funnels
that every route crosses, and the per-language hooks are gone. See issue
2984's resolution for the change and its proof. This issue's test,
`imperative_timer_census`, kept its coverage of every `Node` method. Its kinds
now follow what the slot is (a steady-clock timer in a group is `in_group`
whichever method made it), and it gained a declarative-route case.
