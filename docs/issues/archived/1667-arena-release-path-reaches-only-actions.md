---
id: 1667
title: "The arena can release an entry now, but only action entities use it —
  subscriptions, services, service clients and guard conditions still live for
  the executor's lifetime, and a `cancel()` on a copyable two-word handle needs
  a generation check the handle does not carry"
status: resolved
type: enhancement
area: [core, api]
severity: medium
found: 2026-10-03
related: [1496, 1036, 1705, phase-456, phase-476, rfc-0096]
---

## What changed, and what it did not reach

`9768795b1d` (2026-10-02, issue 1496 resolution 2) gave the executor arena a
removal path. A bounded table of RELEASED regions — 8, coalescing, first-fit
with split — is tried by `arena_alloc`, `arena_alloc_bytes` and
`arena_alloc_with_trailing` before the bump pointer moves, and
`Executor::release_entry` frees the entry's callback slot, its scheduling state
and its bytes. Measured there: 200 create/release cycles hold `arena_used` at the
first cycle's high-water mark.

**Only action entities call it.** `git grep -n 'pub unsafe fn release_'`
finds four functions, all in `executor/action.rs`:
`release_action_server_raw{,_sized}` and `release_action_client_raw{,_sized}`.
For every other arena entity the old behaviour stands:

| entity | C++ handle | what happens on `reset()` / scope exit |
| --- | --- | --- |
| dispatch subscription | `SubscriptionHandle<M>` | the callback goes on firing |
| dispatch service server | `ServiceHandle<S>` | the handler goes on being dispatched |
| dispatch service client | `ClientHandle<S>` | the response handler goes on being dispatched |
| guard condition | — | `spin_once` still polls it; a create/drop loop exhausts `NROS_EXECUTOR_MAX_CBS` |

So `SubscriptionHandle<M>` still has no `cancel()`. It used to be IMPOSSIBLE; it
is now a missing feature.

## The stale REASON was in ~25 places, and is fixed here

The behaviour above was explained everywhere as "the arena has no removal path".
That stopped being true on 2026-10-02 while staying the stated reason in the C++
headers users read, five parity-ledger rows, the cbindgen'd FFI doc, three
compile probes, a gate, a test and two doc comments in the core crate itself —
one of which still said resolution 2 "would change the arena into something with
a free list", the day after it did. Each is rewritten to the TRUE reason: this
entity kind is not wired to the release path, or, for publishers, that an arena
slot would make the C++ lifetime diverge from Rust's caller-owned
`EmbeddedPublisher<M>` (phase-456 W4's argument that did NOT expire).

The sweep was run twice, because the first grep — for "arena has no removal" and
its close variants — missed seven present-tense sites the second, broader one
found:

```sh
git grep -n -i "no removal path" -- ':!docs/roadmap/archived' ':!docs/issues/archived'
```

What it reports now is past tense by construction ("expired", "until
`9768795b1d`", "first argued"), plus `docs/design/0096-…:178`, which is about
`NodeHosted::owned_entities` — a `std::vector`, not the executor arena — and is
correct.

`destroy_shape.rs` needed more than wording. Its table classifies each
`*_destroy` FFI by `needs_drop` of the type its `drop_in_place` drops, and
`9768795b1d` made that diverge from what the FUNCTION does:
`nros_cpp_action_client_destroy` now releases its entry through the executor
before a `drop_in_place` that is still a no-op. The row stays `NO_OP` — true of
the drop — and the table now says that `NO_OP` describes the DROP, not the
entity's fate. A third shape (`RELEASES_VIA_EXECUTOR`) would let the table say
what each destroy does rather than what its last line does; not done here.

## Why `cancel()` is not a one-liner — measured, not guessed

* **A released slot is reused immediately.** `Executor::next_entry_slot` returns
  the FIRST `None` in `entries` (`spin.rs`, `.position(|e| e.is_none())`), and
  `release_entry` sets the released one back to `None`. So the next registration
  of anything takes the index a released entity held.
* **`HandleId` is a bare index.** `pub struct HandleId(pub usize)` — no
  generation.
* **The action release is sound by CONTRACT.** `release_action_server_raw`'s
  `# Safety` says *"no `ActionServerRawHandle` naming it may be used afterwards —
  the slot is handed to the next registration"*. That is enforceable where the
  owner is unique: the C++ `Server` object is destroyed on release, so no copy
  survives.
* **A `SubscriptionHandle<M>` is two COPYABLE words** (`{executor_,
  sched_handle_id_}`), deliberately, so ported code can store it in as many
  places as it likes. A `cancel()` on one copy would leave every other copy
  naming a slot that the next registration — of possibly a different KIND — then
  occupies. The same `unsafe` contract cannot be offered to C++ callers who copy
  by design.

So the prerequisite is a generation (or equivalent) that a stale handle can be
checked against: either in `HandleId`, which widens a public type, or in the
entry metadata with the handle carrying the generation it was issued with.

## Progress — phase-476 W0 (2026-10-05)

The prerequisite is in place. A `HandleId` now carries the generation of the
slot it was issued for, packed into the same `usize` (slot in the low 16 bits,
a 15-bit generation above, never 0). `Executor::resolve_handle` answers the slot
only while that registration still occupies it, and every public lookup goes
through it. A test releases a timer, lets a subscription take the same slot,
and checks that every call through the stale timer handle fails without
touching the subscription. A second test does the same with a timer reusing the
slot.

Also from W0: subscriptions, services and clients registered through nros-cpp
record their node as owner, and `nros_cpp_node_destroy` releases them. A
capture is part of its entry's recorded region, so releasing a capturing entry
no longer leaks the capture.

Still open here: `SubscriptionHandle<M>::cancel()` and the service twins, and
the guard-condition release. The stale-copy bullet below now holds in the
executor. It has not yet been exposed through those C++ handles.

## Acceptance

* Subscriptions, service servers and service clients registered through the
  arena can be released, through `release_entry`, and their RMW entities leave the
  graph — measured with a create/release loop like `9768795b1d`'s, holding
  `arena_used` flat.
* A handle that outlives its release cannot reach the slot's next occupant:
  tested by releasing, registering a DIFFERENT kind into the reused index, and
  calling through the stale copy — which must fail loudly, never dispatch.
* `SubscriptionHandle<M>::cancel()` (and the service twins) exist only once both
  hold, and their docs say what a stale copy does.
* Guard conditions get the same release, or a written reason why not.
* The 8-region bound is revisited with a workload that churns: `9768795b1d`'s
  "arena FRAGMENTED" diagnostic names the failure, and this is where it would
  first fire.

## Resolution (2026-10-06)

Every acceptance item is met. One finding outside them was fixed on the way,
and one gap is filed as issue 1705.

**`reset()` releases, for subscriptions, services and clients.**
`SubscriptionHandle<M>::reset()`, `ServiceHandle<S>::reset()` and
`ClientHandle<S>::reset()` now release their registration through
`nros_cpp_subscription_unregister`, `nros_cpp_service_server_release` and
`nros_cpp_service_client_release`. The callback stops, the entity leaves the
backend, and the entry and its capture are freed. This is the contract
`TimerHandle::reset()` got in phase-476 W2, and the verb is upstream's
(`sub_.reset()`); there is no separate `cancel()`.

- A stale copy reaches nothing. Every copy carries its slot's generation, so
  once one copy is reset the others resolve to nothing.
- The three FFIs, and the timer's, share one body, `release_dispatch_entry`
  in `nros-cpp/src/lib.rs`.
- The subscription verb is `_unregister` because `nros_cpp_subscription_release`
  is the loan-release verb.
- `arena_capture_lifetime_runtime` covers this. It checks that the stub
  backend's live-entity count drops on each reset, that a subscription's
  capture is destroyed exactly once, and that resetting a stale subscription
  copy leaves the timer that took its slot running.

**Release from inside the entry's own callback is deferred.** This was found
while wiring the above, and it already affected `timer_.reset()` from W2. The
upstream one-shot idiom calls `timer_.reset()` inside the timer's callback.
That dropped the entry while its callback was still running: the C++ capture
was destroyed under the running lambda, and `spin_once` then read the
`CallbackMeta` the release had just taken.

- The spin thread now marks a slot `IN_DISPATCH` around each `try_process`.
  A release in that window sets `RELEASE_PENDING`, which stops the handle
  resolving at once, and the entry is released when the callback returns
  (`begin_dispatch` / `finish_dispatch`).
- The flags ride in `SlotTag`'s padding byte, so no backing size moves.
- Tests: `a_callback_releasing_its_own_entry_is_deferred_until_it_returns` in
  Rust and probe step (8) in C++. The Rust test fails when the marking is
  removed.
- Not covered: the `scheduler-os-priority` worker thread, which dispatches
  outside that bracket. Filed as issue 1705.

**Guard conditions: released by the C++ destructor. The Rust verb is `unsafe`,
and here is why.** `Executor::release_guard_condition` exists. Unlike the other
release verbs, a generation cannot make it safe: the `GuardCondition` trigger
handle points at the flag inside the entry, and it is `Clone` + `Send`. A clone
on an ISR or another thread would write into whatever reuses those bytes, so
the Rust contract is "no trigger handle is used afterwards".

The C++ `nros::GuardCondition` is non-copyable and the only holder of its
trigger handle, so its destructor can keep that contract. It calls
`nros_cpp_guard_condition_release` before dropping the handle, and before
freeing its hosted closure block, which the entry could otherwise call into.
`nros_cpp_guard_condition_create` gained an `out_handle_id`. Probe step (9)
runs 100 create/destroy cycles, far past `NROS_EXECUTOR_MAX_CBS`; without the
release it fails.

**The 8-region bound, revisited with a churning workload: replaced.**
`a_churning_mixed_workload_never_exhausts_the_arena` runs four entry sizes
(40 B to 4.6 KiB), at most three live, released in pseudo-random order.

- On the old table, 200 000 rounds filled the 74 240 B arena (73 248 B used)
  with at most ~14 KiB live and only 5 424 B listed as released. After that,
  103 128 registrations failed.
- The bytes were lost, not fragmented. A full table discarded a region, and
  `arena_take_freed` discarded any split tail under 64 B. Neither could ever
  coalesce again.
- The table is now an INTRUSIVE, address-ordered free list. Each free region
  holds its node (`next`, `len`) in its own first bytes.
  - It has no capacity limit, coalesces with both neighbours, and hands a hole
    that ends at `arena_used` back to the bump pointer.
  - Every arena region is now a multiple of the 8-byte grain and at least one
    node long (`ARENA_GRAIN`, `ARENA_MIN_REGION`), so every released region can
    be listed. A split tail too short to list stays with the allocation, and
    is freed with it.
- On the same workload: 0 failures, a peak of 19 568 B, and an arena that
  coalesces back to `arena_used == 0` with no holes once nothing is live.
- The `Executor` value lost the 136 B table and gained an 8 B head.

Tier run: `just check cpp` (green, including the three new probe steps) and the
nros-node unit tests (`node-std-tests` features, 593 passing).
