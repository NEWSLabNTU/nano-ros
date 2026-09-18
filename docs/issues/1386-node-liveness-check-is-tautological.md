---
id: 1386
title: "`node_ref_is_live(node_ref_of(node))` compares the current generation
  with itself, so three `NROS_RET_STALE_NODE` arms can never fire"
status: open
type: bug
area: [api]
severity: medium
found: 2026-09-18
related: [phase-417, phase-379, issue-1384]
---

## What is true

The phase-379 W4 generation scheme is sound where an entity holds a reference
minted when it was created: `nros_node_ref_t` records `(node_id, generation)`,
`rcl_node_fini` bumps the slot's counter, and a stored reference stops
resolving. `publisher.rs`, `subscription.rs` and `executor.rs` all use it that
way and are correct.

Three sites do not store a reference. They mint a fresh one and immediately ask
whether it is live:

```rust
// packages/api/nros-c/src/node.rs — rcl_node_is_valid
if node_ref.is_multi_session() {
    return node_ref_is_live(node_ref_of(node));
}
// packages/api/nros-c/src/node.rs — nros_node_resolve_name
if !node_ref_is_live(node_ref_of(node)) {
    return NROS_RET_STALE_NODE;
}
// packages/api/nros-c/src/guard_condition.rs — nros_node_create_guard_condition
if !crate::node::node_ref_is_live(crate::node::node_ref_of(node)) {
    return NROS_RET_STALE_NODE;
}
```

`node_ref_of` builds `{ node_id, generation: current_generation(node_id) }` and
`node_ref_is_live` is `r.is_bound() && current_generation(r.node_id) ==
r.generation`. The second conjunct therefore reads the same counter twice and
compares it with itself. The first conjunct is `generation != 0`, and the
counters are initialised to 1 and wrap to 1, never 0.

So for any in-range slot the predicate is **constant true**. It is false only
when `node_id >= MAX_NODES` (4 in the shipped configuration), which a node
built by `nros_executor_node_init` cannot be.

Consequences:

* `nros_node_resolve_name` documents `NROS_RET_STALE_NODE` and cannot return it.
* `nros_node_create_guard_condition` — new in PR #1064, phase-417 W4.e —
  documents `NROS_RET_STALE_NODE` in its `# Returns` block and cannot return it.
* `rcl_node_is_valid`'s doc comment says the third thing it checks is that
  "the executor slot it is bound to still carries the generation it was bound
  at", naming the C copy-after-fini case it exists for. It cannot check that,
  because an `nros_node_t` does not store the generation it was bound at —
  there is nothing to compare against.

## How it was measured

Same stub-RMW C TU as issue 1384 (`just check c`'s harness, feature set
`std,rmw-cffi,platform-posix,ros-humble,param-services`). Two nodes are built
on one executor, each is copied, both originals are finalised, and the copies
are re-asked:

```
G after fini: is_valid(copy of primary)=1 is_valid(copy of second)=1
```

The SECOND node is the one that matters: `node_id == 1`, so
`is_multi_session()` is true and `rcl_node_is_valid` really does evaluate the
generation arm — and still answers "valid" for a copy whose original was
finalised and whose slot was retired. That is the exact case the doc comment
says the generation catches.

(The primary node's `1` is issue 1384 rather than this one: it never reaches
the arm at all.)

## Fix shape

The check needs a reference minted EARLIER than the call, which means the node
struct has to carry one. `nros_node_t` already has `node_id`; binding-time
generation is the missing field, and it is appendable at the end of the struct
the way `qos_overrides` was, so the C ABI holds. `nros_executor_node_init`
stores `current_generation(node_id)` beside `node_id`, and the three sites ask
`node_ref_is_live(nros_node_ref_t { node_id, generation })` built from the
struct's own fields rather than from a fresh read.

If that is judged too much for what it buys, the alternative is to delete the
three calls and the `NROS_RET_STALE_NODE` lines from the two doc blocks, so the
surface stops promising a verdict it cannot produce. Either is better than a
check that reads as coverage — an inert guard is the shape
`check-no-vacuous-tests` and issue 1167 exist for, one layer over.

Acceptance is the measurement above inverted: the copy of a finalised node must
read invalid, and `nros_node_resolve_name` on it must answer
`NROS_RET_STALE_NODE`.

## Ordering

Cheapest fixed together with issue 1384: two of the three sites are the same
two functions, and 1384's fix is what makes this arm reachable on a
single-node image for the first time.
