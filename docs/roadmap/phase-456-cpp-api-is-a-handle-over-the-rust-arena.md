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
need. (Quoted from the tree as W3 found it. W3 has since deleted that comment
along with its subject, so do not go looking for it in `service.hpp`.)

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

* **W3 [cpp] — services and clients. LANDED. Actions AUDITED and split out.**
  The item read "services, then actions. Same audit, one kind at a time." The
  audit ran, and its answer is that the two halves of that sentence are not one
  item: services and clients land together and actions do not belong with them.

  *What LANDED, and it is the sentence the item asked for.* The trampoline
  context stops being `&out` for both `Service<S>` and `Client<S>`. It is the
  USER'S HANDLER now, carried by value in the `void*` the registration already
  has — which is phase-456 W1's "the arena carries the callback's capture" for
  the case where the capture is one word and the slot already fits it. No new
  ABI entry point, no Rust arena change. `service.hpp`'s move-constructor
  warning is deleted in the commit that removes its subject, and `client.hpp`'s
  identical one with it; a callback-style service and client are MOVABLE.

  *The measurement the shape had to answer first* — what a caller actually
  INVOKES on each entity, across `examples/`, `tests/`, `book/` and `packages/`,
  the same census W2 ran for subscriptions:

  | entity, path | sites | what is invoked on it |
  | --- | --- | --- |
  | `Service<S>` **dispatch**, out-ref | 1 | **NOTHING** |
  | `Service<S>` **dispatch**, `::SharedPtr` | 2 | **NOTHING** |
  | `Service<S>` poll, out-ref | 5 | `take_request`, `send_response` |
  | `Service<S>` poll, `::SharedPtr` | 1 | **NOTHING** (a probe; the factory exists to be `->take_request`'d) |
  | `Client<S>` **dispatch**, out-ref | 2 | **`async_send_request`** (1 site), nothing (1) |
  | `Client<S>` **dispatch**, `::SharedPtr` | 1 | **NOTHING** |
  | `Client<S>` future, out-ref | 5 | `send_request`, `wait_for_service` |
  | `ActionServer<A>` | 3 | `publish_feedback` `complete_goal` `accept_goal` `succeed` `abort` `canceled` `send_cancel_reply` `try_recv_goal_request` `try_recv_cancel_request` `set_goal_callback` `set_cancel_callback` `set_accepted_callback` |
  | `ActionClient<A>` | 8 | `send_goal` `send_goal_async` `get_result` `get_result_async` `get_result_future` `cancel_goal` `send_cancel_request` `send_get_result_request` `try_recv_feedback` `feedback_stream` `wait_for_action_server` `set_callbacks` `poll` |
  | **the ported templates** (`examples/templates/`) | **0** | there is no service, client or action in the ported corpus at all |

  W2's finding HOLDS for a dispatch service — stored and dropped, a keep-alive —
  and does NOT hold uniformly. Three different answers, and each changes the
  design:

  1. **A dispatch service is a keep-alive.** Nothing is called on it. The handle
     shape follows exactly as it did for `Subscription<M>`.
  2. **A dispatch client has one live verb.** `async_send_request` is measured
     at `examples/native/cpp/service-client-callback/src/main.cpp:95`, and it
     needs `{executor_, handle_id_}` and nothing else — two words, the same two
     `SubscriptionHandle<M>` carries. So a `ClientHandle<S>` is the same shape
     with a method on it, not an empty one. Saying "same as the subscription"
     would have been wrong by exactly one verb.
  3. **An action is not a handle candidate at all** — see below.

  *The `&out` hazard was cheaper to remove than the phase doc assumed, and the
  reason is a measured one.* The SFINAE guard on both callback-style factories
  admits only a plain function pointer (`void(*)(const Request&, Response&)` /
  `void(*)(const Response&)`), so the entire dispatch state the C++ object held
  for the arena was ONE word. It did not need W1's `register_capturing` entry
  point; it needed the `void* context` slot that had been there since
  Phase 189.M3.3.e to carry the handler instead of the address of the object
  holding the handler. `nros::detail::fn_to_context` /
  `fn_from_context` (`callback_context.hpp`) is that carrier, and it copies the
  object representation rather than `reinterpret_cast`ing, for a reason that was
  MEASURED rather than assumed: the cast is conditionally-supported, and on
  gcc 12.3 it is **silent** under `-Wall -Wextra -Wpedantic -Werror` (it needs
  `-Wconditionally-supported`; clang needs `-Wc++98-compat-pedantic`), so it
  would have shipped unremarked on a target where it does not hold. Both forms
  emit the same single `movq` at `-O2`.

  *Three dead members per class went with it, and they were dead by
  construction.* `TypedServiceFnWithCtx` / `user_fn_ctx_` / `user_ctx_` on
  `Service<S>`, and the `TypedResponseFnWithCtx` trio on `Client<S>`, were
  written to `nullptr` at exactly one site each and read only by an `else if`
  the SFINAE guard made unreachable. A handler that wants context binds it at
  compile time — `nros::bind_service<Svc, C, &C::method>` — which is the shape
  the one in-tree component server already uses.

  *Cost, measured on x86-64:* `sizeof(rclcpp::Service<S>)` 576 → **552**,
  `sizeof(rclcpp::Client<S>)` 584 → **560**. Three pointers each, and the
  registration grows by nothing — the handler replaces the object pointer in a
  slot that already existed.

  *What this item did NOT do, and why — the alias cannot flip yet.* `W2` could
  make `Subscription<M>::SharedPtr` a handle because upstream has no returning
  poll subscription, so that factory is callback-ONLY. `Service<S>` is not in
  that position: `Node::create_service<S>(name, qos)` — no callback — also
  returns `Service<S>::SharedPtr`, and its whole purpose is
  `service->take_request(...)`, as `node.hpp`'s own comment on it says. One
  alias cannot be both a handle and a pointer to a poll object. So flipping
  `Service<S>::SharedPtr` is blocked on the poll returning factory changing its
  return type or going away — a decision about the POLL path, which this phase
  says it does not touch. Recorded here rather than attempted.

  *Two follow-ups this item found and did not take:*
  - `nros.hpp`'s returning callback-style `create_service` / `create_client`
    still `owned_entities.push_back(s)`. That push was LOAD-BEARING while the
    arena held `&*s` — dropping the caller's pointer would have dangled it — and
    it is now pure retention. One line each, in a file another work item owns.
  - Nothing statically refuses a future re-registration of `&out`. The structural
    pressure is that the fields such a trampoline would read are deleted, so
    reinstating it is a visible act rather than a one-word edit; a gate over
    `service.hpp`/`client.hpp`'s register calls would make it a failure instead.

  *Acceptance, met:* the compile-probe sweep is unchanged (42 PASS / 14 FAIL,
  identical set); `bind_service.cpp` now MOVES a registered callback-style
  service and client and sends on the moved client, which is the operation the
  deleted warning forbade; `check-cpp-{freestanding-includes,capability-layout,
  freestanding-mechanisms,subscription-bound-supplied,ffi-error-mapping}`,
  `check-ffi-struct-mirrors`, `check-unsafe-census` and `api-parity --check` all
  green.

* **W3b [cpp, core] — actions, measured and deliberately separate.** The audit
  says an action is a different kind of thing from a service, on three counts,
  and "same audit, one kind at a time" does not survive any of them:

  1. **It is not a keep-alive.** 12 verbs on the server, 13 on the client, in
     the table above. Every one of them is reached on the object.
  2. **The C++ object IS the entity.** `nros_cpp_action_server_register` is
     handed `out.storage_` — the `CppActionServerLayout` living inside the C++
     object — not a context pointer, and Phase 87.6's own comment says the
     name buffers live there too. `owned.hpp` already calls this out: it is
     "the one type in nros-cpp that registers its storage address externally".
  3. **Its move is a working mechanism, not a hazard.** `relocate` followed by
     `install_callbacks()` re-registers the trampolines with the new `this`.
     There is no warning here to delete, because the problem was solved rather
     than documented.

  So an action becomes a handle only by moving `CppActionServerLayout` into the
  arena and giving all ~25 verbs arena-side entry points — a core Rust change of
  a different order from W3, and one that wants W8's single registration
  function underneath it. Not started.

* **W4 [cpp] — publishers. DECIDED: `Owned<T>` stays, and an arena slot is
  refused.** No dispatch, so no arena slot exists today. `nros::Owned<T>` covers
  them, and the RMW handle relocates, so the entity can live in the ported
  file's own member. Whether a publisher should get an arena slot anyway, for
  uniformity, was the open question. It is answered NO, and not on grounds of
  size — the arm that looked merely redundant turns out to cost correctness.

  *What a publisher holds, measured.* `sizeof(rclcpp::Publisher<M>)` is **872
  bytes and INDEPENDENT of `M`** — identical for a 512-byte-bound
  `std_msgs/String`, an 8-byte `Int32` and a 65 552-byte `Image`. It decomposes
  as `NROS_PUBLISHER_SIZE` 608 (the `RmwPublisher` handle) + `topic_name_` 256 +
  one `bool` + 7 padding. **No part of it is a transmit buffer sized from the
  type's bound**: serialisation goes into the backend's outbound buffer, and
  `publish_streamed` stages on the stack.

  That is why this phase's central size argument does not reach a publisher.
  The 888-byte `Subscription<M>` whose `storage_` was unused on the dispatch
  path, and the 4 672-byte `Client<int>` holding a reply buffer, are objects
  carrying state the arena already keeps or state derived from `M`. A publisher
  carries neither. Moving it to the arena would relocate 872 bytes, not remove
  them.

  *Does the Rust side have a publisher arena slot?* **No, and the absence is
  structural rather than an omission.** `EntryKind` is
  `{Subscription, Service, ServiceClient, Timer, ActionServer, ActionClient,
  GuardCondition}` — seven kinds, every one of them something the executor
  DISPATCHES to; `arena.rs` contains the word "publisher" exactly once, inside a
  comment about a symptom. Both Rust creation paths —
  `Node::create_publisher_with_qos` (`executor/node.rs`) and the context form
  (`spin.rs:3525`) — return `EmbeddedPublisher<M>` **by value** to the caller,
  which owns it and drops it.

  So the phase's governing principle is ALREADY satisfied here. "Entity lifetime
  is defined by a Rust data structure" — for a publisher that structure is
  `EmbeddedPublisher<M>`, a caller-owned value with a `Drop`, and
  `nros::Owned<Publisher<M>>` is a faithful C++ mirror of exactly it. Giving the
  C++ publisher an arena slot would make the C++ lifetime model DIVERGE from the
  Rust one, which is the opposite of what this phase is for.

  *Why the refusal is about correctness, not tidiness.* The arena is a bump
  allocator: `arena_used` only grows, and nothing anywhere sets an entry slot
  back to `None`. **There is no removal path** — which is precisely why
  `SubscriptionHandle<M>` offers no `cancel()` and says so. A subscription can
  live with that, because a registration that fires forever is what a
  dispatch subscription IS. A publisher cannot: `~Publisher()` calls
  `nros_cpp_publisher_destroy` today, and `Owned<T>::reset()` destroys now. An
  arena publisher would silently turn `pub_.reset()` and scope exit into no-ops
  holding a live RMW publisher for the executor's lifetime — a regression
  against upstream rclcpp (where the last reference destroys) and against our
  own Rust API.

  *What the corpus does with a publisher — the opposite of W2's finding.* W2
  measured that the ported templates call **nothing** on a subscription. For
  publishers, outside `nros-cpp/include`: **46 method calls**, of which
  `publish` is 41, `publish_raw` 2, `loan` 1, `assert_liveliness` 1, `is_valid`
  1, and `publish_streamed` / the two QoS-event setters 0. Fifteen go through
  `->`, and **twelve of those fifteen are ported template or example node
  bodies** — seven under `examples/templates/` (`cpp-port-minimal-publisher`,
  `rclcpp-compat-smoke`, `workspace-shadowing`, `local-msg-package` ×4) and five
  embedded `examples/*/cpp/talker`. The remaining three are this API's own
  compile probes.

  A publisher handle must therefore DEREFERENCE to something with the publish
  API. `Owned<T>` has `operator->` returning the entity directly;
  `SubscriptionHandle<M>` deliberately has none because there is nothing to
  dereference. An arena handle would have to re-export nine methods as
  forwarders, each doing an arena lookup, to deliver an API the caller already
  reaches by pointer. That is the sense in which this is "the only place
  `Owned<T>` is load-bearing" — and the load is bearing toward keeping it.

  *A correction W4 found and made.* `nros.hpp`'s returning `create_publisher`
  pushes the cell into `owned_entities` under the comment *"the arena stores
  `&entity` as its dispatch context and there is no unregister"*. For a
  publisher that sentence is false in both halves — nothing registers it and the
  arena stores nothing of it. The retention is still correct (the returned
  pointer must outlive the full-expression, and upstream's node owns its
  publishers too), so only the stated reason changed. A rationale copied from
  the dispatch entities is exactly the kind of comment that makes the next
  reader believe a publisher is an arena entity.

  *What W5 inherits, stated so the flip is a one-line change.*
  `Publisher<M>::SharedPtr` becomes `nros::Owned<Publisher<M>>`;
  `ported_create_publisher_freestanding_probe.cpp` and
  `ros2_api_adoption.cpp:149` invert there, not here.
  `owned_publisher_ported_shape.cpp` (added by W4) already compiles the ported
  member pattern — default-construct from `nullptr`, move-assign, `->publish`,
  `reset()` — against `Owned<Publisher<M>>` under C++14 and C++17, so W5 is
  flipping to a shape that is already proven rather than discovering it.

  `ConstSharedPtr` is the one thing that cannot be spelled the obvious way.
  **Measured: `Owned<const T>` declares cleanly and is ill-formed on first move
  or `reset()`** — the two operations a member performs — because both assign
  through `value_`. That half-legality is worse than a refusal, so `owned.hpp`
  now `static_assert`s against `Owned<const T>` and names the resolution:
  `ConstSharedPtr` is the SAME type as `SharedPtr`, for `Owned<T>` the same
  reason `SubscriptionHandle<M>` gives — a const/mutable distinction over a
  handle presupposes shared ownership, which a sole owner does not have, and
  `const Owned<T>&` already yields the `const T*` view.

  *One measured follow-up, stated and NOT taken.* `topic_name_` is 256 of the
  872 bytes (29 %) and exists to save a runtime hop in
  `Publisher<M>::get_topic_name()`, which has **zero call sites in the tree**
  outside its own definition. Deleting the cache — making the accessor a runtime
  hop — would be the largest single saving available on this type, three times
  what any arena move could offer. It is not W4's: `get_topic_name()` is
  upstream API a porter may reach for, so this is an implementation change to a
  live accessor, and `Subscription<M>` carries the identical 256-byte cache, so
  it is a class fix and not a publisher fix.

  *Acceptance, met:* the open question is answered with the measurements above
  rather than by implementing the cheaper arm; `owned.hpp` and `publisher.hpp`
  state why a publisher is not an arena entity, at the two places a reader
  asking the question will look; the `Owned<const T>` trap is a compile error
  naming its resolution; the compile-probe sweep is 43 PASS / 14 FAIL against a
  42 / 14 baseline, the one addition being W4's own probe.

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

Two further work items, **W7** and **W8**, are stated in the next section
rather than here, because each is derived from a finding that arrived with a
rebase and reads as nonsense without it.

## What changed on `main` while this phase was being opened (2026-09-13)

Rebased over 104 commits. One of them matters a great deal, and it is the same
disease this phase treats, diagnosed one layer down.

**phase-454 W5 (`ac9316aea`, issue 1319) found that a subscription has FIVE
registration paths**, and had to enumerate them because the executor was pricing
every subscription at its type's bound while the runtime slot depends on which
path it took:

| path | what claims it |
| --- | --- |
| `c_typed_hint` | C or C++, typed, `rx_size_bound<M>` supplied |
| `rust_typed_descriptors` | Rust, typed, descriptor-carrying backend |
| `rust_typed_in_place` | Rust, typed, in-place backend (zenoh, XRCE) |
| `rust_typed_schemaless` | Rust, typed, schemaless buffering backend |
| `c_raw_no_hint` | **C or C++, raw, no hint** |

Two of those five are the C family, and they exist only because the C++ API has
both a hint-passing and a hint-less way to register the same subscription. That
is precisely the boilerplate this phase is about, and it has a measured cost:
`claims_closure_buffer()` is true for `c_raw_no_hint`, so that row is priced at
`RX_BUF` rather than at the type's bound.

**And the descriptor writer knows it cannot tell them apart.** Its own comment:

> A C/C++ entry that registers typed supplies `rx_size_bound<M>`; the raw
> no-hint row is a property of an individual call site, not of the image, and
> nothing this writer reads distinguishes them. The typed hint is therefore what
> a C/C++ entry is CREDITED with.

So a C++ image is credited with `c_typed_hint` whether or not its call sites
earn it, and a site that takes the no-hint row is UNDER-sized. That is a latent
mis-size the writer documents rather than hides.

**W2 makes the credit true for one call site.** The returning
`create_subscription` now always passes `rx_buffer_capacity<M>::value`, so it is
genuinely `c_typed_hint`. It was before too — the old `create_subscription_raw`
call passed the same value — so this is not a regression either way, and the
sizing classification is unchanged by W2.

**What is still live**, and it is a new work item rather than a claim:
`component.hpp`'s `create_subscription_raw` takes `size_t rx_bytes = 0`, so any
caller that omits it registers with no hint while the image is credited with
one. The default is the defect.

* **W7 [cpp, core] — the C/C++ registration rows collapse from two to one.
  LANDED.** Make the bound non-optional at every C++ registration site, so
  `c_raw_no_hint` becomes unreachable from C++ and the descriptor's credit stops
  being an assumption. `rx_buffer_capacity<M>` is available wherever the message
  type is, which is every typed site; a genuinely type-erased raw site is the
  one case that has to keep the row, and it should have to say so.

  *What was actually there, measured.* **Five** sites reached the arena with a
  hint of 0, and every one of them had the message type `M` in scope as a
  template parameter: `Node::create_subscription` (callback form),
  its callback-group sibling, `create_subscription_with_info`,
  `create_subscription_validated` and `Node::create_subscription_in_group`.
  Four of the five wrote no 0 at all — they started from
  `nros_cpp_subscription_default_options()` and set only `sched_context`, so the
  omission was invisible at the call site. The fifth inherited it from
  `create_subscription_raw`'s `size_t rx_bytes = 0` default parameter.

  `size_bound.hpp`'s own header comment had been asserting the opposite since
  phase-408: *"Every C++ subscribe path fills `rx_buffer_hint` from it."* That
  sentence is true now; it was aspirational when it was written.

  *The shape.* `rx_bytes` loses its default at `create_subscription_raw`, so a
  caller has to say which row it is taking. `nros::rx_bound_unknown` is the
  `c_raw_no_hint` row spelled out loud — the value is still 0, and naming it
  changes nothing at runtime and everything about what a grep can tell apart.
  The tree has exactly one site that legitimately passes it,
  `bind_subscription_raw`, whose callback takes bytes and whose type arrives as
  a NAME.

  *The gate.* `check-cpp-subscription-bound-supplied`, fast lane (`check cpp` is
  `build-serial`, which no merge-gating event runs — issues 1225, 1226, 1331).
  It requires every arena registration in these headers to state a bound whose
  right-hand side says where the number came from, and it admits
  `rx_bound_unknown` only from a function with no message type parameter in
  scope. That is a structural rule rather than an authored allowlist, so the one
  legitimate site is admitted by its shape and a second one would have to earn
  it the same way. Four mutation cases in the selftest, plus a negative control
  asserting the unmutated tree is clean.

  *What is still not earned, stated rather than claimed.* The descriptor's
  credit is now an assumption over a GREPPABLE set instead of five silent sites,
  not a fact. Consumer code can still pass `options = NULL` with the type in
  scope, and **seven C example listeners do** — issue 1376, with the remedy
  (`nros_cpp_subscription_register_hinted`, which takes the QoS as an argument
  and so serves the two sites with a custom profile) and the reason it is its
  own change: it touches seven example leaves in four workspaces, so acceptance
  there is a fixture build and a re-measure, not a header edit.

  *Acceptance, met:* no C++ subscription registration defaults its bound to 0;
  `c_raw_no_hint` is unreachable from the C++ API without naming it; the seven
  compile probes that exercise these headers compile byte-identically to before
  (swept against a `HEAD` worktree — the 14 that fail are the expected-failure
  probes and fail identically on both trees).
  *Related:* issue 1340 is the sibling one row over — `rust_typed_in_place` is
  priced at the bound while claiming no region at all, worth ~9.7 KiB per
  subscription. Issue 1376 is the C consumer half of this item.

* **W8 [core, cpp, abi] — one registration function, and the C/C++ side calls
  it.** W7 collapses the two C-family rows into one. This item asks the question
  W7's answer makes obvious: why is there a language axis at all?

  *The finding.* The five `RegistrationPath` rows decompose into three real
  properties and one that is not a property of the registration:

  | axis | values | decided by |
  | --- | --- | --- |
  | is the type's bound known? | yes / no | the CALL SITE, via the hint |
  | does the backend dispatch in place? | yes / no | `supports_process_in_place()` |
  | is the schema reachable? | yes / no | the backend's descriptor support |
  | *who called* | *Rust / C* | *nothing about the subscription* |

  Two of the five rows — `c_typed_hint` and `rust_typed_descriptors` — describe
  the same registration and differ only in the fourth. The other C row,
  `c_raw_no_hint`, is the first axis answered "no", which is what W7 makes
  unreachable from C++.

  *The cost of the fake axis is measured, and it is not bookkeeping.* The C
  registration path never consults `supports_process_in_place()` — only
  `register_subscription_buffered_on` does (`spin.rs:4908`), and it returns
  through `SubInplaceEntry` before any slot size is computed. So **every C and
  C++ subscription on zenoh or XRCE allocates a receive region the backend does
  not need**, at the same order as issue 1340's ~9.7 KiB per subscription. The
  irony is that the C callback is the BETTER candidate for in-place dispatch:
  `RawSubscriptionCallback` is already `(const uint8_t*, size_t, void*)`, a
  borrowed-bytes signature, where the Rust typed path has to produce an owned
  `&M` from somewhere.

  *The shape.* One `register_subscription_on` taking an argument struct rather
  than thirteen entry points taking positional parameters
  (`spin.rs` currently has 13 `register_subscription_*` /
  `add_arena_subscription_*` functions, ~1 100 lines before the C-validated
  tail). The struct carries what the axes above need: node, topic, type name and
  hash, QoS, group, the bound as a value rather than a `usize` defaulting to 0,
  the delivery shape (typed / raw / borrowed / with-info / validated), and the
  callback with its optional capture. The function consults
  `supports_process_in_place()` ONCE, for every caller, in every language.

  C and C++ then call the same function the Rust API calls, which is this
  phase's premise applied to the last place it does not hold.

  *What it costs, measured before committing to it.* Unification routes the Rust
  typed path through an indirect call where it is monomorphised today. Two
  probes, because a host number is weak evidence for an embedded project:

  | target | monomorphised | type-erased | delta |
  | --- | --- | --- | --- |
  | x86_64, `rustc -O -C lto=fat -C codegen-units=1`, 20M iterations, run 1 | 1.302 ns | 1.305 ns | +0.003 ns |
  | run 2 | 1.220 ns | 1.206 ns | **−0.014 ns** |
  | run 3 | 1.194 ns | 1.196 ns | +0.002 ns |
  | cortex-m3, `arm-none-eabi-g++ -Os -ffreestanding -mcpu=cortex-m3 -mthumb` | 25 insns | 8 + 25 insns | **+8 insns** |

  On the host the effect is 30× smaller than run-to-run variance (1.194–1.302)
  and one run came out negative, so there is no measurable cost there. On
  cortex-m3 the erased dispatch is 8 instructions because it TAIL-CALLS
  (`bx r3`) and the work moves into the separate callback; 4 of the 8 are
  argument shuffling. Against a deserialize-and-sink that is already 25
  instructions, the honest figure is **+8 instructions per dispatch**.

  Three limits on those numbers, stated rather than left for a reader to find:
  the ARM figure is a STATIC instruction count, so the pipeline refill an
  indirect branch costs is not in it; the probe payload is 16 bytes of trivial
  copy, so a real message makes the relative cost smaller, not larger; and the
  host probe measures dispatch alone, with no RMW beneath it. If the decision
  ever looks close, the QEMU harness gives cycles. It does not look close.

  *Reproducing the probes.* They are throwaway files under `tmp/` rather than
  tracked fixtures: a Rust `bench.rs` calling a monomorphised
  `FnMut(&Msg)` and a `fn(*const u8, usize, *mut c_void)` through a
  `black_box`ed function pointer over the same 16-byte message, and an ARM
  `arm.cpp` of the same two shapes read back with
  `arm-none-eabi-objdump -d`. Anyone re-deciding this should rebuild them rather
  than trust the table.

  *Acceptance.*
  - `RegistrationPath` loses its language axis: 5 rows become 3, and
    `claims_closure_buffer()` is answerable from the registration's arguments
    rather than from who called.
  - `supports_process_in_place()` has exactly one consulting site, and a C or
    C++ subscription on zenoh or XRCE claims no receive region. Measured on
    `contract-monitor-sub`, the same entry phase-454 W5 measured, so the before
    and after are comparable.
  - W1's `CALLBACK_CAPTURE_BYTES` stops being an ABI constant the two sides must
    agree on and becomes a runtime length, which removes the failure mode its
    own doc comment describes (`BufferTooSmall` reached only when the two sides
    disagree about a number).
  - The 13 entry points become one plus whatever thin wrappers the call sites
    actually want; the count is in the commit message.
  - `just check abi-bindings` green with the regenerated `generated.rs`
    committed (RFC-0054).

  *Sequencing.* After W7, which is a one-line change per call site and lands the
  descriptor's credit immediately; W8 is a core refactor and should not hold it
  up. Independent of W3 and W4 — those are C++-side shape, this is the seam
  beneath them — but doing W8 first would let W3 and W4 be written against one
  function instead of five.

  *Open question, recorded rather than decided.* Whether the Rust typed path
  keeps a monomorphising `#[inline]` wrapper for a caller who has measured that
  it needs one. The default answer is no — one function is the point, and 8
  cortex-m3 instructions is the price — but the wrapper is cheap to add later if
  a real workload ever produces a number that argues for it.

  *Related:* issue 1319 (the five rows), issue 1340 (`rust_typed_in_place` is
  priced at the bound while claiming no region — the sibling over-statement one
  row over, which this item's in-place unification touches directly).

Two smaller ones, noted so a reader does not rediscover them:

* `11bbf4ec9` (another session) added a `static_assert` pinning
  `decltype(rate.period())`, which is the follow-up issue 1331 asked for. It and
  this branch's own fix coexist; the rebase was clean.
* `check-unsafe-census` is new and caught W1/W2's eight new `unsafe` sites. The
  growth is enumerated and argued in its own commit rather than absorbed, which
  is what that gate exists to force.

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
