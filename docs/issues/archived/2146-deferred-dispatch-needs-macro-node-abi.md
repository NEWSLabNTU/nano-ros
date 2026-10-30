---
id: 2146
title: "Deferred dispatch is reachable only through the `nros::node!`-emitted callback ABI, so a hand-written host cannot run callbacks in its own task"
status: resolved
resolved: 2026-10-30
type: enhancement
area: [core, executor, macros]
severity: low
found: 2026-10-29
related: [2144, 2134, 2161, phase-520]
---

## What

RFC-0112 D2's Tier 0 contract needs a **Defer** part: a host enqueues a ready
callback in the RMW callback context and runs it later in a task it chooses.
That is what RTIC does, and what any priority-scheduled framework needs.

The mechanism exists: `register_dispatch_slot`
(`packages/core/nros-node/src/executor/spin.rs:6756`) and `dispatch_callback`
(`:6887`). But its contract is the per-node
`__nros_node_<pkg>_on_callback(state, cb_id_ptr, cb_id_len, ctx)` ABI that
`nros::node!` emits, and `state` comes from
`nros::__macro::__private_node_state_into_raw`. A caller that did not go
through the macros has no supported way to fill a slot. So deferred dispatch is
a Tier 1 (macro-generated) feature only, and a Tier 0 recipe or a Tier 2
out-of-tree framework cannot use it.

## Direction (RFC-0112 D2.2)

Add a hand-callable form:

- registration takes a plain `fn(ctx, callback-id)` and context, with no
  macro-private state type;
- the executor enqueues on readiness and the host drains with one call from
  its own task.

The RTIC emit should then be rewritten on top of it, so there is one ring and
not two. Until this lands, RFC-0112 D2 lists Defer as Tier 1 only.

## Resolution

The executor now has a deferred-dispatch queue that hand-written code drives
directly, with no macro-private type
(`packages/core/nros-node/src/executor/deferred.rs`):

- `Executor::set_deferred(handle, bool)` marks one registered callback, by the
  `HandleId` its registration returned. It returns `false` for a dead handle
  or a timer: a timer's readiness is decided inside its own dispatch, so there
  is nothing to queue ahead of running it.
- `Executor::set_defer_notifier(fn(ctx, HandleId), ctx)` (`nros::DeferNotify`)
  installs a plain function and context pointer. When the spin finds a
  deferred callback ready, it queues it instead of running it and calls the
  notifier once. A callback that is already queued is not queued or notified
  again.
- `Executor::run_deferred()` runs everything queued, and
  `Executor::run_deferred_handle(handle)` runs one queued callback, for hosts
  that run each callback in a task at its own priority. `has_deferred()` and
  `is_deferred_pending(handle)` poll the queue.

The queue has one bit per callback slot (the callback table is already capped
at 64), so it allocates nothing. The spin enqueues at two points: in the
readiness scan, and in the mid-pass merge (`merge_mid_pass`), so a callback
that becomes ready during a pass is queued too. A deferred callback is left out
of the spin's `Trigger`. Releasing a slot clears that slot's deferral.

The notifier runs on the spinning thread, inside the pass, which is where the
executor learns about readiness. It does not run in the backend's arrival
callback. A host that needs to hear about work from interrupt context pairs
this with the executor-level readiness notifier (issue 2144).

Tests (`packages/core/nros-node/src/executor/tests.rs`, `cargo test -p
nros-node --features alloc --lib deferred`):

- a hand-written subscription is queued and notified, then run by the drain;
- the per-handle drain runs only the callback it names;
- timers are refused, and a released slot leaves the queue;
- a callback that becomes ready mid-pass is queued, not run. Removing the
  merge hook makes this test fail.

Docs: RFC-0112 D2's Defer row now names this API.
`book/src/internals/dispatch-strategy.md` gained "Deferring callbacks without
the macros", and the Embassy chapter now points to it.

**With the readiness notifier (issue 2144).** The two hooks fire at different
moments and do not replace each other. `set_ready_notifier` fires when data
ARRIVES (from the backend's wake, on any thread) and tells the host to spin.
`set_defer_notifier` fires during that spin, when the executor finds a deferred
callback ready, and tells the host which callback to run. A host that sleeps
until work arrives uses both: spin on the first, drain on the second.

**Left for phase-520 W3:** emitting deferred dispatch from `nros::setup!` for
`dispatch = "deferred"` components
(`docs/roadmap/phase-520-setup-macro-host-loop.md`). This change provides the
executor API that W3 builds on, and does not touch `setup!` or
`main_macro.rs`.

**Not done:** moving the RTIC emit onto this queue, so there is one ring and
not two. While reading the RTIC code for that, it turned out the old macro-only
registry has no caller (`register_dispatch` is emitted and never called), and
the RTIC board queue's only producer is the `e2e-synthetic-callback` feature.
That work, or retiring the registry, is filed as issue 2161. It needs the RTIC
QEMU cell, and sibling issue 2150 is editing the same `main_macro.rs` branch.
