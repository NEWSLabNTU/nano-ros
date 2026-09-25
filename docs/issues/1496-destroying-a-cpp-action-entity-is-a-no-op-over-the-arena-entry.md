---
id: 1496
title: "Destroying a C++ action server or client is a no-op over its arena entry
  — the five RMW entities and the goal table live for the executor's lifetime,
  and the destructor that looks like it releases them drops a struct of `Copy`
  fields"
status: open
type: bug
area: [api, core]
severity: medium
found: 2026-09-25
related: [rfc-0096, phase-456, issue-1335, issue-0810]
---

## What happens

`~rclcpp_action::Server<A>()` calls `nros_cpp_action_server_destroy(storage_)`,
and `~rclcpp_action::Client<A>()` calls `nros_cpp_action_client_destroy`. Both
FFI functions are:

```rust
core::ptr::drop_in_place(storage as *mut CppActionServer);
```

`CppActionServer` is `{ handle: Option<ActionServerRawHandle>, goal_cb,
cancel_cb, accepted_cb, cb_ctx, node_id, _reserved, qos }` and `CppActionClient`
is `{ callbacks, arena_entry_index: i32, executor_ptr }`. **Every field of both
is `Copy` or a raw pointer** — `ActionServerRawHandle` is explicitly
`impl Copy` with no `Drop` — so `drop_in_place` runs no destructor at all. It
is a no-op with the shape of a release.

What is NOT released is the part that matters. The action server's five RMW
entities (three service servers for send_goal/cancel_goal/get_result, plus the
feedback and status publishers), its `active_goals` table, its `completed_results`
table and its `result_slab` all live in an `ActionServerRawArenaEntry` in the
executor arena, which `register_action_server_raw` builds. The arena is a bump
allocator: `arena_used` only grows, nothing sets `entries[i]` back to `None`
(the one place in the tree that says so is `callback_trace.rs:220`), and there
is no `unregister`/`deregister`/`remove_entry` anywhere in
`packages/core/nros-node/src/executor/`.

So after a C++ action server goes out of scope:

* its three service servers are still advertised, and a `send_goal` query still
  reaches the arena entry;
* the goal trampoline's `context` is the address of the destroyed object's
  `storage_`, so a goal arriving after destruction reads freed C++ storage;
* nothing about the arena's occupancy changed, so a program that creates and
  drops action servers in a loop exhausts `NROS_EXECUTOR_MAX_CBS` and then the
  arena.

## Why it was not noticed

Every in-tree C++ action site holds its server for the whole program: the five
`examples/*/cpp/action-server/src/main.cpp` leaves declare it in `main` or as a
file-scope object, the four `examples/{zephyr,workspaces}/cpp` action leaves
hold `::nros::ActionServerStorage` / `ActionClientStorage` members on a
component that lives for the app lifetime, and `component.hpp`'s own doc comment
says so outright: *"lives for the app lifetime — the executor arena holds it"*.
A lifetime nobody shortens cannot expose a release that does nothing.

## Why this is filed rather than fixed here

phase-456 W4 refused an arena slot for publishers precisely to avoid creating
this state — *"an arena publisher would silently turn `pub_.reset()` and scope
exit into no-ops holding a live RMW publisher for the executor's lifetime"*. For
actions that is not a risk to avoid; it is the measured status quo, reached
before the phase started. Recording it separately keeps W4's argument honest:
the argument is about what moving an entity to the arena would CREATE, and it
cannot be used either for or against a change to actions without first saying
that actions are already there.

## Candidate resolutions, none taken

1. **Say so, and make the destructor honest.** Rename or document the two FFI
   functions as "abandon" rather than "destroy", and make the C++ destructor
   detach the arena entry's `context` (set it to null, so a late goal is
   rejected instead of reading freed storage). Cheapest, and fixes the only arm
   of this with memory-safety consequences.
2. **Give the arena a removal path.** The general fix, and much larger than
   actions: it is what `SubscriptionHandle<M>` has no `cancel()` for, and it
   would change the arena from a bump allocator to something with a free list.
3. **Leave it, and gate the shape.** A check that every `*_destroy` FFI whose
   body is a bare `drop_in_place` over a `Copy`-only struct is documented as a
   no-op. This is the class, not the site: the same question should be asked of
   every `nros_cpp_*_destroy`.

Resolution 1 is the one this issue recommends; it is a C++-side and
`action.rs`-side change with no arena work.

## How to re-measure

```
# the handle is Copy with no Drop
grep -n 'impl Copy for ActionServerRawHandle' packages/core/nros-node/src/executor/action.rs
# no removal path anywhere in the executor
grep -rn 'fn unregister\|fn deregister\|fn remove_entry\|arena_used -=' packages/core/nros-node/src/executor/
```
