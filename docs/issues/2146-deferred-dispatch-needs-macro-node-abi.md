---
id: 2146
title: "Deferred dispatch is reachable only through the `nros::node!`-emitted callback ABI, so a hand-written host cannot run callbacks in its own task"
status: open
type: enhancement
area: [core, executor, macros]
severity: low
found: 2026-10-29
related: [2144, 2134]
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
