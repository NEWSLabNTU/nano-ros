---
id: 1496
title: "Destroying a C++ action server or client is a no-op over its arena entry
  — the five RMW entities and the goal table live for the executor's lifetime,
  and the destructor that looks like it releases them drops a struct of `Copy`
  fields"
status: resolved
type: bug
area: [api, core]
severity: medium
found: 2026-09-25
related: [rfc-0096, phase-456, issue-1335, issue-0810, issue-1225]
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

## Resolution 1 TAKEN, 2026-09-28 — and what it did not fix

**The memory-safety arm is closed. The other two bullets above are still true**,
which is why this stays open: they are resolution 2, and nothing here gives the
arena a removal path.

What landed:

* `Executor::detach_action_server_raw{,_sized}` and
  `Executor::detach_action_client_raw`
  (`packages/core/nros-node/src/executor/action.rs`) cut the one edge that
  outlives the owner. The server entry's `goal_callback` / `cancel_callback`
  become `detached_goal_callback` / `detached_cancel_callback` — stubs that take
  no notice of `context` — its `accepted_callback` goes `None` and its `context`
  goes null. The client entry's three callbacks are already `Option`s the
  dispatch guards, so clearing them is the whole cut.
* The stubs are asymmetric on purpose: a late GOAL is **rejected**, a late CANCEL
  is **accepted**. Accepting a cancel runs entirely inside the arena's own
  `ActionServerCore` (it moves the goal to `Canceled` and replies), so a client
  holding a goal accepted before the server died learns the goal ended instead of
  waiting for a result nobody will complete.
* `nros_cpp_action_server_detach(storage, executor_handle)` is a NEW FFI, called
  by `~Server()` and by `Server::operator=(Server&&)` before
  `nros_cpp_action_server_destroy`. A second call rather than a parameter on the
  existing one, for the reason issue 0796 gives about
  `nros_cpp_action_server_set_callbacks`: `*_destroy` is declared in three places
  and growing it breaks every caller. `CppActionServer` keeps no executor
  pointer, and nothing in `nros-cpp` can recover one from a bare storage pointer
  (a `CppContext` lives in caller-provided storage; there is no registry).
* `nros_cpp_action_client_destroy` needed no signature change — `CppActionClient`
  holds `executor_ptr` + `arena_entry_index` — so it detaches in place.
* **Names kept, docs made honest.** Renaming the two `*_destroy` symbols was
  weighed and rejected: they are cbindgen output (so the header, and every
  consumer's copy of it, moves with them) reached from five C++ call sites plus
  the polling twins, and the rename buys a better word where the doc comment can
  say the whole thing. Both now state that they abandon rather than release, name
  the bump allocator, and point at the detach; so do `~Server()` and `~Client()`.

Where the late-arrival path is rejected, by file:line on the tree that landed
this:

* server, goal: `packages/core/nros-node/src/executor/arena.rs:2603` —
  `(*goal_callback)(…, *context)`. There is **no null-context branch here and
  deliberately so**: after the detach the callback IS the reject stub, so the
  entry cannot dispatch through `context` whatever its value. A guard in the
  dispatch would have to be paid on every goal on every spin.
* server, cancel: `arena.rs:2589` — same shape, the accept stub.
* server, post-accept: `arena.rs:2620` — `if let Some(post) = *accepted_callback`,
  which the detach sets to `None`.
* client: `arena.rs:2725`, `2749`, `2793` — `if let Some(cb) = …`, all three
  cleared by the detach.

Measured, not reasoned (`cargo test -p nros-node --lib --features alloc`):
`detaching_a_raw_action_server_rejects_a_late_goal_and_never_reads_its_context`
feeds the SAME `send_goal` frame to the same arena entry either side of the
detach — before, the callback runs and gets the owner's address; after, the
callback does not run at all and the client gets a reply whose `accepted` byte is
0. Mutation-checked: neutering the `goal_callback` write fails it on the "must
NOT reach the callback" assertion, and neutering the client's
`goal_response_callback` write fails its sibling test.

### Still open here

* **The arena never gives a slot back** (resolution 2). The three service servers
  and two publishers stay advertised, `arena_used` only grows, and a
  create/drop loop still exhausts `NROS_EXECUTOR_MAX_CBS`. A detached entry is
  cheap but not free: it keeps answering, rejecting goals from anyone who finds
  the action in the graph.
* **`nros_cpp_action_client_relocate` leaves the arena `context` pointing at the
  MOVED-FROM storage.** Found while doing this, unfixed, and now written into
  `action_client.hpp` beside the move ctor. `Server` re-points its own context by
  calling `install_callbacks()` after the relocate; the client tier has no FFI
  that can re-point one, so a moved-from client whose storage then dies (a
  temporary) reproduces exactly this issue's dangling-context arm through a
  different door. Destruction is handled; the move is not. The fix is the
  mirror-image of the detach — a retarget call on the same arena write — and it
  belongs with whoever next touches that relocate.

## Resolution 3 TAKEN — the survey, and the gate

All nine `nros_cpp_*_destroy` functions classified, by `needs_drop` rather than by
reading struct definitions (`packages/api/nros-cpp/src/destroy_shape.rs`, whose
asserts fail the build if a type's answer moves):

| destroy FFI | drops | `needs_drop` |
| --- | --- | --- |
| `nros_cpp_action_server_destroy` | `CppActionServer` | **no-op** |
| `nros_cpp_action_client_destroy` | `CppActionClient` | **no-op** |
| `nros_cpp_guard_condition_destroy` | `nros_node::GuardCondition` | **no-op** |
| `nros_cpp_publisher_destroy` | `CppPublisher` | releases |
| `nros_cpp_subscription_destroy` | `RmwSubscriber` | releases |
| `nros_cpp_service_server_destroy` | `RmwServiceServer` | releases |
| `nros_cpp_service_client_destroy` | `RmwServiceClient` | releases |
| `nros_cpp_action_server_destroy_polling` | `ActionServerCore<…>` | releases |
| `nros_cpp_action_client_destroy_polling` | `ActionClientCore<…>` | releases |

**Three of nine, so the gate was worth writing.** The split is not about the API
tier, it is about WHERE the state was put: the four RMW handles and both L1
POLLING action cores sit inline in the caller's storage and their `Drop` destroys
the backend entity, while the two arena-registered action entities and the guard
condition hold a handle to something the arena owns.

`nros_cpp_guard_condition_destroy` is the third site, and it is the milder shape:
the arena entry does NOT hold the destroyed object's address —
`nros_cpp_guard_condition_create` captures the CALLER's callback and context — so
there is nothing to detach and no freed-storage read, only the unreclaimed slot.
It is now documented as a no-op (that is what the gate first caught on this
tree). `GuardCondition::closure_` in the C++ header would create the dangling
arm; it is freed by that destructor and no in-tree call site attaches a block to
a guard condition, `attach_closure_block` having no caller for this type.
(Issue 1225 is about that member's effect on `sizeof`, not this.)

Gate: **`check-cpp-destroy-shape`** (`scripts/check-cpp-destroy-shape.py`, fast
line, `just/check/abi.just`). It is the coverage half a Rust table cannot state —
every destroy FFI must have a row, every row must name a real function and the
type that function actually drops, and a `NO_OP` row's doc comment must say so
and name this issue. Its selftest runs on the normal path with four controls: the
compliant shape, an unlisted destroy, an undocumented `NO_OP`, a row whose type
the function never drops, and a row for a function that no longer exists.

## How to re-measure

```
# the handle is Copy with no Drop
grep -n 'impl Copy for ActionServerRawHandle' packages/core/nros-node/src/executor/action.rs
# no removal path anywhere in the executor
grep -rn 'fn unregister\|fn deregister\|fn remove_entry\|arena_used -=' packages/core/nros-node/src/executor/
```

```
# the detach, and the two sides of it
grep -n 'fn detach_action_server_raw\|fn detach_action_client_raw' \
    packages/core/nros-node/src/executor/action.rs
cargo test -p nros-node --lib --features alloc detaching

# the class: nine destroy FFIs, three no-ops, all classified
python3 scripts/check-cpp-destroy-shape.py
```

## Resolution 2 TAKEN, 2026-10-01 — the arena gives an entry back

Branch `fix/executor-arena-exact-0810-1340-1370-1036-1496`.

* **A removal path.** The executor keeps a table of RELEASED arena regions
  (`Executor::arena_freed`, 8 entries, adjacent holes coalesce, first-fit with
  the tail split off). `arena_alloc`, `arena_alloc_bytes` and
  `arena_alloc_with_trailing` try it before moving the bump pointer, so the
  arena is still a bump allocator for everything that is never released.
* **`Executor::release_action_server_raw{,_sized}` /
  `release_action_client_raw{,_sized}`** drop the entry in place — the RMW
  handles' `Drop`s destroy the three service servers and two publishers (or the
  three service clients and the feedback subscription), so the action LEAVES
  THE GRAPH — free the callback slot (`entries[i] = None`, binding reset) and
  hand the entry's bytes to the free list. Wrong kind or an empty slot refuses
  and touches nothing.
* **C++.** `nros_cpp_action_server_detach` (symbol kept: cbindgen output, called
  from `~Server()` and the move-assignment) and `nros_cpp_action_client_destroy`
  now RELEASE rather than detach. Both destructor comments say so.
* **The move arm this file recorded as UNFIXED** —
  `nros_cpp_action_client_relocate` now re-points the entry's `context` at the
  new storage (`Executor::retarget_action_client_raw`), so a moved-from
  temporary no longer leaves the entry naming dead bytes.

### Measured

`cargo test -p nros-node --lib --features std`, on a mock session with the
shipped defaults (`MAX_CBS = 4`, `ARENA_SIZE = 74,240`):

* `an_action_server_created_and_released_in_a_loop_never_exhausts` — 200
  register/release cycles of a raw action server; `arena_used()` equals the
  first registration's high-water mark on every one of them;
* `an_action_client_created_and_released_in_a_loop_never_exhausts` — the same
  for a raw client;
* `without_the_release_the_same_loop_exhausts` — the negative control: the loop
  without the release fails at registration 4;
* `released_regions_split_and_coalesce`,
  `a_release_of_the_wrong_kind_or_an_empty_slot_is_refused`,
  `retargeting_a_raw_action_client_moves_its_context`.

Mutation: replacing the free-list lookup in `arena_alloc` with `None` fails both
loop tests at iteration 0 ("the arena grew; the released region was not
reused").

### Not covered

* **The C API** has the same shape and no executor-remove call to fix it with —
  filed as issue 1609.
* Typed Rust action handles and every other entity kind have no release call;
  the free list serves whatever is released, and only the two raw action kinds
  release today.
* Not run on a target: the release is host-measured on the mock session. The
  C++ destructor path compiles (`cargo check -p nros-cpp`, the regenerated
  `nros_cpp_ffi.h`) and is covered by `check-cpp-destroy-shape`; no C++ image
  exercising a create/destroy loop was run.
