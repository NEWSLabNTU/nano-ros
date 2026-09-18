---
id: 1384
title: "`is_multi_session()` answers for the wrong node — the FIRST
  executor-bound C node takes slot 0, so every predicate keyed on
  `node_id != 0` reads it as a legacy node"
status: open
type: bug
area: [api, core]
severity: high
found: 2026-09-18
related: [phase-417, phase-156, rfc-0089, issue-1385, issue-1386]
---

## What is true

`nros_node_t::is_multi_session()` is

```rust
// packages/api/nros-c/src/node.rs
pub(crate) fn is_multi_session(&self) -> bool {
    self.node_id != 0 && !self.executor.is_null()
}
```

and `nros_executor_node_init` stores whatever `NodeBuilder::build()` returns:

```rust
// packages/api/nros-c/src/executor.rs — nros_executor_node_init
node_ref.node_id = node_id.raw();
node_ref.support = core::ptr::null();
node_ref.executor = executor as *const nros_executor_t;
```

`build()` returns `NodeId(self.executor.nodes.len() - 1)`
(`packages/core/nros-node/src/executor/node_record.rs`), so **the first node an
executor builds is `NodeId::PRIMARY`, which is 0** — `NodeId::PRIMARY` is
declared as exactly that, and
`packages/api/nros-c/tests/run/executor_param_node_keying.c` asserts it in
words: *"the first node built should be the primary slot"*.

So a node that IS bound to an executor, through the very call that binds it,
fails `is_multi_session()` whenever it is the only node — the commonest shape
in the tree. The predicate conflates "how many siblings does this node have"
with "does this node reach an executor", and only the second question is the
one its four call sites ask.

**The predicate also names the wrong thing.** Nothing about a second node makes
a session "multi"; `nros_executor_node_init` leaves `support` NULL and routes
through the executor whether there is one node or eight. The name is from phase
156 sub-bug D, where the two questions happened to have the same answer.

## Where it lands — four sites, three distinct failures

All four are in `packages/api/nros-c/src/node.rs`
(`rcl_node_is_valid`, `nros_node_resolve_name`, `resolve_entity_name_on_node`,
`resolve_session_and_domain`). This is a CLASS, not a site: the reported
symptom was `nros_node_resolve_name`, and it is the least severe of the three
outcomes.

| site | what a primary executor-bound node gets | verdict |
| --- | --- | --- |
| `nros_node_resolve_name` (`only_expand == false`) | `NROS_RET_NOT_INIT` | loud, wrong answer |
| `resolve_session_and_domain` | the legacy arm → `node.get_support_mut()` → NULL → `None` → caller returns `NROS_RET_NOT_INIT` | **loud, and it breaks every eager entity create** |
| `resolve_entity_name_on_node` | expansion only — the node's launch remap rules are dropped | **silent**, currently masked by the row above |
| `rcl_node_is_valid` | skips the `node_ref_is_live` arm | no observable difference today, see issue 1386 |

The second row is the severe one. `nros_executor_node_init` sets `support =
NULL` *on purpose* (phase-156: "multi-Node paths key off node_id + executor"),
so on the primary node the legacy arm has nothing to read, and every C entity
that is created EAGERLY rather than at registration — `rclc_publisher_init_default`
and its QoS/options siblings, the polling subscription, `nros_service_init`,
`nros_client_init` — fails `NROS_RET_NOT_INIT` before it ever reaches the
backend.

The third row is the one that will bite the fixer. `resolve_entity_name_on_node`
runs BEFORE `resolve_session_and_domain` at all four of its call sites
(publisher, subscription, service, client), so today the remap drop is hidden
behind the harder failure that follows it. **Fixing the session predicate alone
unmasks a silent wrong-key bug** — a publisher and a subscription given the same
string landing on different wire names, which is the exact failure RFC-0089's
campaign exists to stop. Fix the class in one commit.

## How it was measured

A C TU built against the same stub-RMW harness `just check c`
uses for `executor_param_node_keying.c` (same feature set
`std,rmw-cffi,platform-posix,ros-humble,param-services`, same
`cc … libnros_c.a` link line — the recipe is in `just/check/lanes.just`). It
opens a support context, an executor, then TWO nodes, and asks the same
questions of each:

```
NROS_RET_OK=0 NOT_INIT=-7 INVALID_ARGUMENT=-3 UNSUPPORTED=-16 ERROR=-1
first node:  node_id=0 executor=0xffffe003bce0 support=(nil)
second node: node_id=1 executor=0xffffe003bce0 support=(nil)
A resolve_name(primary, only_expand=false) -> -7  out=''
B resolve_name(second,  only_expand=false) ->  0  out='/chatter'
C resolve_name(primary, only_expand=true ) ->  0  out='/chatter'
E publisher_init(primary) -> -7
F publisher_init(second ) -> -1
```

Two nodes, one executor, identical in every way a caller can see, and they
answer differently. `E` vs `F` is the discriminator for the session claim: the
stub backend refuses `create_publisher` with `NROS_RMW_RET_UNSUPPORTED`, so the
second node's `-1` is a call that REACHED the backend, while the primary node's
`-7` is `resolve_session_and_domain` returning `None` before the backend was
consulted at all.

## What is exposed today

`examples/native/c/custom-platform` is the one in-tree caller of this shape.
PR #1064 (phase-417 W4.e) moved it from `rclc_node_init_default` to
`nros_executor_node_init` — correctly, because the new
`nros_node_create_guard_condition` needs a node that reaches an executor — and
its `rclc_publisher_init_default` call is three lines further down. Its
`examples/fixtures.toml` row is **build-only**: no `matrix::CELLS` cell, no
runtime assertion, so nothing in any lane runs the binary and observes the
`Failed to init publisher: -7` this predicts.

`examples/native/c/parameters` uses `nros_executor_node_init` too but creates
no eager entity, so it is unaffected.

## The fix shape

W4.e already wrote it down and used it, one function over. From
`nros_node_create_guard_condition`:

```rust
// The predicate is "is this node BOUND to an executor", NOT
// `is_multi_session()` — that one also requires `node_id != 0`, and the
// FIRST node `nros_executor_node_init` builds takes slot 0 (the primary
// slot; `executor_param_node_keying.c` asserts exactly that). A node's
// right to create an entity does not depend on how many siblings it has.
if node_ref.executor.is_null() {
    return NROS_RET_NOT_INIT;
}
```

So: `!self.executor.is_null()` is the predicate, `is_multi_session` is renamed
to what it tests (`is_executor_bound`), and all four sites move together. Note
that the sibling helper `parameter.rs`'s `node_key` already gets this right by
a different route — it accepts `node.executor.is_null()` (the legacy path) and
only range-checks a NON-ZERO slot, with a comment saying "Slot 0 is always
legal". That is a third spelling of the same question; the fix should leave one.

Acceptance is a RUN, not a gate: extend the stub-RMW run test so a single
executor-bound node resolves a remapped name and creates a publisher, and give
`examples/native/c/custom-platform` a runtime cell so a build-only fixture
stops reading as coverage.

## Ordering

Independent of issue 1385. Related to 1386, which is the other predicate in two
of the same four functions — fixing this one makes 1386's arm reachable for the
first time, so they are cheapest together.
