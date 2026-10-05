---
id: 1667
title: "The arena can release an entry now, but only action entities use it —
  subscriptions, services, service clients and guard conditions still live for
  the executor's lifetime, and a `cancel()` on a copyable two-word handle needs
  a generation check the handle does not carry"
status: open
type: enhancement
area: [core, api]
severity: medium
found: 2026-10-03
related: [1496, 1036, phase-456, phase-476, rfc-0096]
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
