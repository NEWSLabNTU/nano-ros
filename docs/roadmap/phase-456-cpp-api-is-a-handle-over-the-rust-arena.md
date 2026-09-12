# phase-456 — the C++ API becomes a handle over the Rust arena

**Status (2026-09-12). Opened.** Carries the remainder of
[phase-442](phase-442-one-freestanding-rclcpp-api.md) under the design
[RFC-0096 D9 revision 3](../design/0096-cpp-freestanding-core-and-porting-layer.md)
settled on. phase-442 keeps W0–W7, which landed; its W8 text describes a design
that revision superseded, and the work items below replace it.

## The decision this implements

> The C/C++ API is a thin wrapper over the Rust API. Entity lifetime is defined
> by a Rust data structure.

Stated by the owner, and it turned out to be a description of what the runtime
already does rather than a proposal. For the rclcpp dispatch model the Rust
arena owns the entity — `packages/api/nros-cpp/src/subscription.rs` says so:

> arena (rclcpp dispatch model), as opposed to the poll-style
> `nros_cpp_subscription_create` above. **The arena owns the subscriber**; spin …

## What is wrong today, measured

There are two ABI paths per entity, and the C++ API uses both at once.

| path | entry point | owner |
| --- | --- | --- |
| poll-style | `nros_cpp_subscription_create(…, void *storage)` | the caller |
| dispatch | `nros_cpp_subscription_register(…, out_handle_id)` | the arena |

For a subscription with a callback, one subscription's identity exists in three
places:

1. `SubBufferedRawCEntry` in the arena — `handle: RmwSubscriber`, `buffer:
   BufferStrategy`, `callback`, `context`. This is the real one.
2. `rclcpp::Subscription<M>`, 888 bytes, of which
   `alignas(8) uint8_t storage_[NROS_SUBSCRIBER_SIZE]` is **unused** on this
   path.
3. a heap `detail::SubscriptionCallback<M>` cell, allocated by `nros.hpp`'s
   returning `create_subscription`, whose only job is to hold a
   `std::function` — which (1)'s `callback` + `context` already model.

`component.hpp` shows what the rest of the API could have looked like. Its own
comment calls it a *"Thin wrapper over `nros_cpp_subscription_register`"*, and it
keeps nothing.

Services are the same shape one step less far along:
`nros_cpp_service_server_register` returns a handle id, so the slot exists, but
it is handed `&out` — the C++ object — as its trampoline context. That
back-reference is what `service.hpp`'s move constructor warns about:

> A callback-style service must NOT be moved after register — the arena holds
> `this` as the trampoline context (Phase 189.M3.3.e); the move only transfers
> bookkeeping and leaves that pointer stale, so don't.

A hazard that exists only because the C++ side kept an object the arena did not
need.

## The end state

`X::SharedPtr` for a **dispatch** entity is `nros::Handle` over
`{executor, handle_id}` — two words, copyable, what `nros::Timer` already is and
what phase-442 W3 already built. `nros::Owned<T>` stays for the entities with no
dispatch (publishers, the poll-style forms), where no arena slot exists and the
C++ object genuinely is the entity.

Four things follow, and each is a simplification rather than a trade:

* no new arena, no pool, no node template parameter — the entry point exists;
* `X::SharedPtr` is COPYABLE, which REMOVES the move-only difference RFC-0096 D5
  had to record;
* the "must not move after register" hazard loses its subject;
* `sizeof` stops being a design input. The 4 672-byte `Client<int>` that made
  every pooling answer look unaffordable was the C++ object holding the reply
  buffer.

## Work items

* **W1 [core, abi] — the arena carries the callback's CAPTURE.** This gates
  everything else, and it is the only genuinely new code in the phase.

  `context` is a single `void*`. It carries a `[this]` capture — 7 of the 11
  capture sites phase-442 W0 measured — and not `[this, state]` (two pointers)
  or `[obj, method]` (three, because a pointer-to-member-function is two words
  on the Itanium ABI). So a capturing lambda needs somewhere for its bytes, and
  by this phase's own principle that somewhere is the registration, in Rust.

  Shape: `SubBufferedRawCEntry` gains a fixed `[u8; NROS_CALLBACK_CAPTURE_BYTES]`
  (32 — phase-442 W0's measured `4 * sizeof(void*)` on a 64-bit target), an
  ADDITIVE ABI entry point copies the caller's capture into it, and `context` is
  set to the entry's own copy rather than to anything of the caller's.
  `nros::InplaceFn`'s invoker becomes the `callback` field.

  Additive on purpose: the existing `nros_cpp_subscription_register` keeps
  working for the `[this]`-sized cases and for `component.hpp`, which already
  uses it.

  *Acceptance:* a capturing lambda of each measured shape registers and
  dispatches with no C++-side allocation; the capture survives destruction of
  every caller-side temporary; `just check abi-bindings` green with the
  regenerated `generated.rs` committed (RFC-0054).
  *Cost to state, not estimate:* 32 bytes per subscription entry, against a
  measured `NROS_CPP_EXECUTOR_STORAGE_SIZE` of 89 352 — and it replaces an
  888-byte C++ object plus a heap cell.

* **W2 [cpp] — `create_subscription` returns a handle, and the handle stops
  lying.** The design was re-explored after W1 landed, and the question turned
  out not to be "where does the state live" — W1 settled that — but "what should
  the returned thing BE".

  *The finding that drives it.* `Subscription<M>` is two types wearing one name,
  and the tree already documents the consequence, in `nros.hpp`'s own comment on
  the returning factory:

  > WHAT THE RETURNED POINTER IS: a keep-alive … The executor owns the real
  > subscriber, so `sub->take(msg)` on it answers `NotInitialized` — the sample
  > went to your callback.

  So a `Subscription<M>` handed back by the callback factory carries `take()`,
  `take_serialized()`, `take_validated()`, `take_sequence()` and `borrow()` —
  every one of them present and guaranteed to fail. A method set that lies, on
  the type a porter is handed.

  *Measured*, what callers actually invoke on a subscription across the corpus,
  the tests and the book:

  | call | count | path |
  | --- | --- | --- |
  | `try_recv` | 16 | poll |
  | `take` | 11 | poll |
  | `take_serialized` / `take_validated` | 6 | poll |
  | the three QoS-event setters | 14 | either |
  | **anything at all, in the ported templates** | **0** | dispatch |

  The ported corpus holds it and drops it. That is the whole requirement, and it
  is what the header already calls it: a keep-alive.

  *The shape (candidate C of three).* `Subscription<M>` keeps its storage and
  its taking API for the out-ref POLL form, which nothing in this work item
  touches. `Subscription<M>::SharedPtr` becomes a distinct two-word
  `nros::SubscriptionHandle<M>` over `{executor, handle_id}`, exposing only what
  a dispatch subscription can actually do. The returning `create_subscription`
  calls `nros_cpp_subscription_register_capturing` (W1) and allocates nothing:
  no `SubscriptionCallback<M>` cell, no `make_shared`, no `owned_entities` push.

  *The cost, stated.* `SharedPtr::element_type` is no longer
  `Subscription<M>` — the alias and the class become different things. That is a
  surprise for a reader and it needs a ledger row saying so. It is accepted
  because the alternative is keeping a type whose methods fail by construction.

  *Acceptance:* the ported node body compiles and dispatches on all three arms;
  no allocation on the path; `owned_entities` loses its subscription push; the
  dispatch handle exposes no operation that cannot work.

  *Blast radius, measured — three files outside the API headers:*
  `examples/templates/topic-state-monitor-port/src/topic_state_monitor.cpp:31`
  (an explicit `std::shared_ptr<rclcpp::Subscription<…>>`, which becomes the
  alias), `tests/compile/ros2_api_adoption.cpp:158-183` (`static_assert`s that
  INVERT), and `tests/compile/ros2_one_dispatch_path.cpp:163` (already spells
  `::SharedPtr`, so it keeps working). They land in the same commit; the tree
  cannot be green in between.

* **W2b [cpp] — `Subscription<M>` becomes the dispatch type, and the taking API
  moves out (candidate B).** W2's destination, and a separate item because it is
  a rename of a widely-used type rather than a change to one factory.

  Upstream rclcpp has NO poll-style subscription — its taking goes through a
  `WaitSet`. So `take()` / `take_serialized()` / `take_validated()` /
  `take_sequence()` / `borrow()` on `Subscription<M>` are an nros extension
  wearing an upstream name, which is what forces W2's alias/class split in the
  first place. Moving them beside `PollingSubscription<M>` lets
  `Subscription<M>` mean what rclcpp means by it, at which point
  `SharedPtr::element_type` can be `Subscription<M>` again and W2's stated cost
  is repaid.

  *Blast radius:* 33 in-tree poll call sites (`try_recv` 16, `take` 11,
  `take_serialized` / `take_validated` 6). None in the ported templates.
  *Not started, and deliberately after W2:* W2 removes a lying method set
  immediately and without touching those 33; W2b is the tidy-up that makes the
  naming honest.

* **W3 [cpp] — services, then actions.** Same audit, one kind at a time: the
  trampoline context stops being `&out`, so the object the arena knows by
  address ceases to exist. `service.hpp`'s move-constructor warning is DELETED
  in the commit that removes its subject — a warning outliving its hazard is how
  the next person learns to distrust the comments.

* **W4 [cpp] — publishers.** No dispatch, so no arena slot exists today.
  `nros::Owned<T>` covers them, and the RMW handle relocates, so the entity can
  live in the ported file's own member. Whether a publisher should get an arena
  slot anyway, for uniformity, is the open question — it is small either way and
  it is the only place `Owned<T>` is load-bearing.

* **W5 [cpp, examples] — THE FLIP, and it is ATOMIC with the corpus.** The
  moment `X::SharedPtr` stops being `std::shared_ptr`, every file spelling the
  pointer type explicitly stops compiling, and the tree cannot be green in
  between. This is why phase-442's "W9 is deliberately last" does not survive:
  measured blast radius outside `nros-cpp/include` —

  | | count |
  | --- | --- |
  | files | 15 |
  | `::SharedPtr` uses (these KEEP working — the alias changes meaning) | 64 |
  | explicit `std::shared_ptr<rclcpp::X>` / `make_shared` spellings to change | ~30 |

  plus 524 lines of gated blocks in `nros.hpp` and 211 in `node.hpp`.

  Two probes INVERT and must be rewritten in the same commit:
  `ported_create_publisher_freestanding_probe.cpp`, which exists to assert a
  ported `create_publisher` FAILS freestanding, and
  `ros2_api_adoption.cpp:149`, which `static_assert`s that
  `Publisher<M>::SharedPtr` IS `std::shared_ptr<Publisher<M>>`.

  *The binding constraint is unchanged and satisfiable:* `local-msg-package`
  compiles under real ROS 2 Humble via `just colcon-parity`, and `::SharedPtr`
  is valid under both, so those members move TO the alias rather than away.

* **W6 [ci] — the gates become structural.** phase-442 W10, inherited:
  `check-cpp-freestanding-includes` loses its baseline;
  `check-cpp-capability-layout` asserts a constant rather than ratcheting.
  *Acceptance:* zero `NROS_CPP_HAS_*`, zero `NROS_CPP_STD`, zero
  `NROS_CPP_NODE_HOSTED`; both gates fail on a mutation that reintroduces a
  `std` type in a public signature, and the mutation is in the selftest.

## What this phase does not do

* It does not change the poll-style path. Caller storage is correct there —
  nothing dispatches, so nothing holds the address.
* It does not touch `nros-c`. That API puts both its publisher and its timer in
  caller-declared structs, which is a third shape for one concept and is
  issue 1335's remaining half.
* It does not revisit RFC-0096 D8 (the deduced name parameter) or the two
  mechanisms, which landed in phase-442.

## Related

* [RFC-0096](../design/0096-cpp-freestanding-core-and-porting-layer.md) — D9
  revision 3 is the design; D5's fourth entry narrows to publishers when W2
  lands.
* [phase-442](phase-442-one-freestanding-rclcpp-api.md) — W0–W7, landed.
* issue 1335 — the C++ API uses the poll path where it means the dispatch path.
* issue 1225 — the capability-layout rule these entities kept breaking.
