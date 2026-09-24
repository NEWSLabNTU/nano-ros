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
  moves out (candidate B). LANDED 2026-09-18.** W2's destination, and a separate
  item because it is a rename of a widely-used type rather than a change to one
  factory.

  *The premise this item was opened on is WRONG, and it is worth writing down
  because the conclusion survives it.* The item said: "Upstream rclcpp has NO
  poll-style subscription — its taking goes through a `WaitSet`. So `take()` /
  `take_serialized()` / … on `Subscription<M>` are an nros extension wearing an
  upstream name." Measured against the cached upstream surface
  (`docs/reference/api-surface/rclcpp.json`, Humble):
  `rclcpp::Subscription::take(ROSMessageType&, MessageInfo&)` exists, with a
  second `take(TakeT&, MessageInfo&)` overload, and
  `SubscriptionBase::take_serialized(SerializedMessage&, MessageInfo&)` exists.
  They are upstream METHOD NAMES on that exact class, which is why phase-379 W6
  renamed our `try_recv` to `take` in the first place and why
  `cpp:Subscription::take` was a `divergence` (name matched, second parameter
  differed) rather than an extension.

  What upstream does NOT have is a subscriber the CALLER owns. Every
  `rclcpp::Subscription` belongs to the node; its `take` is reached on a
  node-owned entity that a `WaitSet` has reported ready. So the split is right
  and the reason is the OWNERSHIP, not the name — and it costs parity on two
  names rather than repairing it. `cpp:Subscription::take` and
  `cpp:Subscription::take_serialized` are theirs-only rows now, each carrying a
  `provides` pointing at `nros::PollSubscription`, each saying so.

  *What made the split worth that cost is a second measured thing, and it is not
  the one the item argued.* `Subscription<M>` served both owners behind a
  `callback_mode_` flag, and `storage_` — `NROS_SUBSCRIBER_SIZE`, 656 bytes — is
  filled only by the poll creator. On the dispatch path it stayed
  value-initialized, `initialized_` was true, and `take()` handed those zero
  bytes to `nros_cpp_subscription_take_serialized`, whose first act is
  `&mut *(storage as *mut RmwSubscriber)` (`packages/api/nros-cpp/src/
  subscription.rs:693`). `nros.hpp` described that call as answering
  `NotInitialized`; nothing on that path ever checked. The three QoS-event
  setters had the same shape. So the method set was not merely useless on half
  the objects — it was a call into an unfilled struct, and the split is what
  makes it unwritable rather than discouraged.

  *What landed.* `nros::PollSubscription<M>` in `nros/polling_subscription.hpp`,
  beside `PollingSubscription<M>` (which holds one): the taking API, `View` +
  `try_borrow`, `take_sequence`, the seven `[[deprecated]]` `try_recv*`
  forwarders, `stream()`, `get_topic_name`, `is_valid`, the destructor (now
  unconditional), the relocating moves, the three status-event setters, and the
  two out-ref poll creators plus the value-returning `nros::create_subscription`
  factory. `rclcpp::Subscription<M>` keeps the nested handle aliases, the
  handler typedefs, the trampolines, `has_sched_handle` / `sched_handle_id`,
  `get_topic_name` and `is_valid` — and loses `storage_`, `stream_` and
  `callback_mode_` with the API that read them. Measured on this host's config (`NROS_SUBSCRIBER_SIZE` 656), with a
  16-byte stub message: `sizeof(Subscription<M>)` **984 → 304**, and the poll
  half is **936**. The dispatch object is what a ported file declares one of
  per subscription; 256 of its 304 bytes are the topic name the four creators
  now fill.
  `SubscriptionHandle<M>` gains `element_type = Subscription<M>`, which is W2's
  stated cost repaid.

  Two defects fell out of the move rather than being sought.
  `get_topic_name()` answered `""` for every callback-style subscription in the
  tree — only the poll creator had ever written the field — and the four arena
  creators now fill it. And `SubscriptionOptions::sched_context`'s
  unreachability on the poll overload (ledgered by phase-428 W5 as a guard whose
  second conjunct was constant-false) is structural now: the poll type has no
  handle field for the guard to read.

  *Blast radius, measured after the fact:* 8 example poll declarations (5 C++
  listeners, the native safety-listener, 2 PX4 modules), 5 compile probes
  (`receive_verb_aliases`, `receive_deprecation_probe`, `rx_size_bound`,
  `serialization_format`, `ros2_api_adoption`), 2 book lines, and 24 parity
  ledger rows. None in the ported templates, as predicted — they hold
  `::SharedPtr` and call nothing.

  *Acceptance, met:* the 56-probe compile sweep is unchanged at 42 PASS / 14
  FAIL, same set; `check-cpp-subscription-bound-supplied` OK; `api-parity.py
  --check --require-disposition` green over all three languages with the ledger
  moved. `ros2_api_adoption.cpp` carries the structural half as two
  `static_assert`s over a `has_take<T>` detector — the dispatch type must NOT
  have `take`, the poll type must — plus the `element_type` identity, with the
  first mutation-tested.

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

* **W5 [cpp, examples] — THE FLIP, and it is ATOMIC with the corpus. LANDED
  2026-09-21 for publishers and services, RE-APPLIED 2026-09-24 over 362
  commits of `main`; CLIENTS, TIMERS AND `Node` DID NOT MOVE, and the reasons
  are below rather than deferred silently.** The
  moment `X::SharedPtr` stops being `std::shared_ptr`, every file spelling the
  pointer type explicitly stops compiling, and the tree cannot be green in
  between. This is why phase-442's "W9 is deliberately last" does not survive:
  measured blast radius outside `nros-cpp/include` —

  | | doc, as written | RE-MEASURED after W2b/W3/W4 |
  | --- | --- | --- |
  | files outside `nros-cpp/include` | 15 | **22** |
  | `::SharedPtr` uses (these KEEP working — the alias changes meaning) | 64 | **87** |
  | explicit `std::shared_ptr<rclcpp::X>` / `make_shared` spellings to change | ~30 | **23**, in 10 files |

  All three numbers had drifted; the ones on the right are the tree as it stands
  after this phase's four waves. Gated blocks: **519 lines in `nros.hpp`** (39 %
  of the file) and **292 in `node.hpp`**, in 19 blocks, dominated by
  `NROS_CPP_HAS_SHARED_PTR` (17 uses) — which is what this item removes.

  **Probes that INVERT — four, not two.** The doc listed
  `ported_create_publisher_freestanding_probe.cpp` (asserts a ported
  `create_publisher` FAILS freestanding) and `ros2_api_adoption.cpp:149`
  (`static_assert`s `Publisher<M>::SharedPtr` IS
  `std::shared_ptr<Publisher<M>>`). W4 found two more: `ros2_api_adoption.cpp:267`
  (`= std::make_shared<Publisher<StringMsg>>()`) and
  `one_node_type_ours_only_names.cpp:88,95`, both asserting the `shared_ptr`
  return type of the hosted factory.

  *The binding constraint is unchanged and satisfiable:* `local-msg-package`
  compiles under real ROS 2 Humble via `just colcon-parity`, and `::SharedPtr`
  is valid under both, so those members move TO the alias rather than away. Five
  of the 23 breaking spellings are in that leaf; three more are in
  `cmake/compat/diagnostic-updater/`, which the doc did not mention.

  ### Three decisions W5 inherits, settled here

  **1. `Publisher<M>::SharedPtr` becomes `nros::Owned<Publisher<M>>`**, not a
  handle. W4 measured the case and refused the arena slot: the arena is a bump
  allocator with no removal path, so an arena publisher would make `reset()` and
  scope exit no-ops holding a live RMW publisher for the executor's lifetime —
  a regression against upstream rclcpp AND against our own Rust API, where
  `EmbeddedPublisher<M>` has a `Drop`. `Owned<T>` mirrors that Rust lifetime;
  an arena slot would diverge from it.

  **2. `Publisher<M>::UniquePtr` collapses into `SharedPtr`.** `Owned<T>` IS
  unique ownership — move-only, one owner, destroys on scope exit — so a
  separate unique alias would be a second spelling of one type. Measured: the
  alias has **zero uses in the tree** outside its own definition and the one
  probe that asserts its current shape (`ros2_api_adoption.cpp:155`), so nothing
  is ported away from. `ConstSharedPtr` is already the same type by W4's
  reasoning, which made `Owned<const T>` a hard compile error naming the
  resolution.

  **3. `Service<S>::SharedPtr` follows W2b's precedent, not W2's.** W3 found the
  blocker: `Node::create_service<S>(name, qos)` with no callback returns the
  same alias and exists to be `->take_request()`'d, so one alias cannot be both
  a handle and a pointer to a poll object — a collision `Subscription<M>` never
  had, because upstream requires a callback there and nros has no returning poll
  factory for it. W2b already solved this shape for subscriptions by moving the
  poll half out to `PollSubscription<M>`. The same move — a `PollService<S>`
  holding the taking API, with `Service<S>` meaning the dispatch entity — is
  what unblocks it, and it is the consistent answer rather than a new one. It is
  a poll-path change, which this phase said it would not make; that sentence in
  "What this phase does not do" is now narrower than the phase, and W2b is where
  it stopped being true.

  ### What LANDED, and the three things the item said that measurement did not

  All three settled decisions landed as written. `Publisher<M>::SharedPtr` is
  `nros::Owned<Publisher<M>>`, with `ConstSharedPtr` and `UniquePtr` collapsed
  into it; `nros::PollService<S>` (`nros/polling_service.hpp`) carries the
  taking API and the caller-owned `RmwServiceServer`, `rclcpp::Service<S>` is
  bookkeeping over `{initialized_, handle_id_, executor_, service_name_}` (see
  "Three features of `main`" below for why the last two are there), and
  `Service<S>::SharedPtr` is
  `nros::ServiceHandle<S>` (`nros/service_handle.hpp`) — the same two-word shape
  W2 gave subscriptions. `PollingSubscription<M>`'s three aliases went to
  `Owned<T>` in the same pass, because they were `std::shared_ptr` for no reason
  the split left standing. Every alias is UNCONDITIONAL now: four
  `#ifdef NROS_CPP_HAS_SHARED_PTR` blocks are gone, along with the
  `std_detect.hpp` include in `publisher.hpp` and `polling_subscription.hpp`.

  **1. The 23-spelling / 22-file blast radius was measured against a flip of
  FIVE entity families, not three.** Of the 23, only nine actually break under
  the three settled decisions: four `std::shared_ptr<rclcpp::Publisher<…>>`
  members in `local-msg-package`, one each in `rclcpp-compat-smoke` and
  `workspace-shadowing`, one in the book, and the two `ros2_api_adoption.cpp`
  publisher `static_assert`s. The other fourteen are `Node` (7) and `TimerBase`
  / `Timer` (7) spellings, which compile unchanged because those aliases did not
  move. They were rewritten to `X::SharedPtr` anyway — that is the hygiene the
  item asked for and it makes any later flip a one-line change — but a reader
  should not expect 23 compile errors from reverting this commit.

  **2. `ported_create_publisher_freestanding_probe.cpp` inverting is NOT a
  consequence of the alias, and the item's framing hid a second change.**
  Flipping the return type does not make that line compile freestanding: the
  hosted overload is keyed on `const std::string&`, so with the return type
  fixed the gate simply moves from the return to the ARGUMENT and the probe goes
  on failing for a reason nobody wrote down. Making it invert took a second,
  unlisted change — a `const char*`-keyed `create_publisher` declared OUTSIDE
  `NROS_CPP_NODE_HOSTED`, with the `std::string` forms kept as hosted
  forwarders. A string literal binds the `const char*` overload exactly, so a
  ported call reaches it on every target and the hosted one still serves a
  caller holding a `std::string`. Verified by compiling the probe
  `-ffreestanding -nostdinc++` against the ThreadX shim; the lane in
  `just/check/lanes.just` flipped from expected-failure to expected-success in
  the same commit, and its error text now names the two ways to break it.

  **3. A publisher's node co-ownership is GONE, and that is a behaviour change
  the three decisions imply without saying.** `nros.hpp`'s returning
  `create_publisher` used to `make_shared` and push the cell into
  `hosted().owned_entities`. `Owned<T>` is sole ownership, so there is no second
  reference to keep. For the member pattern the corpus uses this is identical to
  upstream (the node is the last owner either way), and for
  `node->create_publisher<M>(…)->publish(m);` the temporary still outlives the
  full-expression. What no longer happens is a publisher outliving its own
  handle. Recorded at `cpp:Node::create_publisher` in the ledger rather than left
  for someone to discover.

  *What did NOT move, with the reason each is its own item:*

  - **`Client<S>::SharedPtr`.** W3 measured the right shape (a `ClientHandle<S>`
    carrying `async_send_request`), but the FUTURE-style
    `create_client<S>(name, qos)` returns the same alias and is drained with
    `send_request` / `wait_for_service` — the identical collision the poll
    `create_service` was, needing the identical remedy (a `PollClient<S>`). It
    is a second split, not a line of this one.
  - **`Timer::SharedPtr`.** Not blocked by an alias at all: `create_wall_timer`
    returns a `shared_ptr` ALIASING into a heap `detail::WallTimer` cell that
    holds the callback's `std::function`. Flipping it needs W1's
    capture-in-the-arena treatment for timers first, which is core Rust work.
  - **`Node::SharedPtr`.** `std::make_shared<rclcpp::Node>("talker")` is the
    ported `main`, and the whole `NROS_CPP_NODE_HOSTED` block is spelled in
    `std::string` / `std::vector` / `std::function`. That is W6's subject, not
    an alias flip.

  ### Three features of `main` this had to be re-implemented ON TOP of, 2026-09-24

  While this branch was being written, `main` added three methods to the very
  class W5 takes apart, and a rebase cannot carry them: they read members the
  restructure deletes. Each was placed on the half that can actually answer it,
  which is a decision and not a merge resolution.

  **`get_service_name()` (phase-444) — BOTH halves, and each keeps its own
  `service_name_`.** The name is copied C++-side at create from the argument the
  caller passed, because the runtime takes it and drops it, and there is no FFI
  that reads a name back out of either an `RmwServiceServer` or an arena entry.
  So a shared copy would have to live somewhere neither half owns. This is
  exactly the shape W2b already settled for `PollSubscription::get_publisher_count`
  — restored on both halves because it reads `topic_name_`, which each half
  keeps — and the answer comes out the same way for the same reason. The
  dispatch half is therefore NOT `{handle_id, initialized}`: `service_name_` is
  256 bytes on an object the item called bookkeeping, and the item's sentence
  was narrowed rather than the member dropped, because `entity_name_accessors.cpp`
  takes `&rclcpp::Service<S>::get_service_name` and phase-444 deleted the
  `cpp:Service::get_service_name` ledger row when the capability SHIPPED.

  **`get_request_subscription_actual_qos()` /
  `get_response_publisher_actual_qos()` (issue 1437) — BOTH halves, and here
  the service comes out the OTHER WAY from the subscription.** W2b had to leave
  `get_actual_qos` off the dispatch `Subscription<M>`: it holds only
  `sched_handle_id_`, so reaching the arena would have meant inventing a
  signature that takes an executor, and that refusal is ledgered at
  `cpp:Subscription::get_actual_qos` precisely so nobody invents one inside a
  merge. A service has no such problem, and the difference is one member: issue
  1437 already gave the dispatch service `executor_` beside `handle_id_`, and
  `nros_cpp_service_server_get_actual_qos` serves BOTH roads by construction —
  `storage` non-NULL for the owner, `(executor, handle_id)` for the arena entry.
  So both halves answer on upstream's no-argument spelling with no weakening,
  the `callback_mode_ ? nullptr : storage_` branch issue 1437 needed inside one
  class is gone from both (each passes its one arm unconditionally), and **no
  gap row is owed**. What WAS added is three ledger rows on the poll half —
  `cpp:PollService::{get_service_name,get_request_subscription_actual_qos,
  get_response_publisher_actual_qos}` — because those are upstream names on an
  ours-only type and would otherwise have inherited `cpp:PollService`'s verdict
  silently. Two of them also needed a `KEY_OVERRIDES` topic entry, the same
  shape `try_recv_request` needed: the accessors spell the ENDPOINT in the verb,
  so `subscription` and `publisher` are literally in the names and pubsub claims
  them.

  *Verification (2026-09-24, in a worktree with no submodules):* the compile
  sweep is 46 PASS / 15 FAIL, IDENTICAL to the base commit's, and a finer
  four-configuration matrix (c++14/c++17 × with/without `NROS_CPP_STD`) over all
  61 probes changes exactly ONE cell —
  `ported_create_publisher_freestanding_probe` going `F P F P` -> `P P P P`,
  which is the inversion this item exists to produce. The ported line also
  compiles `-ffreestanding -nostdinc++` against the ThreadX shim. `just check
  api-parity` green; `just check cpp-fmt` green; `just check fast` is 351 of 353,
  the two failures being `capability-conditionals` and `xrce-vendored-versions`,
  each of which reports "submodule not checked out — run `just setup-worktree`"
  and neither of which this commit touches. NOT run, and not claimed: `just ci`
  and anything that links an RMW backend, because a worktree has no submodules.

* **W6 [ci] — the gates become structural. HALF LANDED, and the other half is
  not reachable yet — measured, not estimated.** phase-442 W10, inherited:
  `check-cpp-freestanding-includes` loses its baseline;
  `check-cpp-capability-layout` asserts a constant rather than ratcheting.
  *Acceptance as written:* zero `NROS_CPP_HAS_*`, zero `NROS_CPP_STD`, zero
  `NROS_CPP_NODE_HOSTED`; both gates fail on a mutation that reintroduces a
  `std` type in a public signature, and the mutation is in the selftest.

  **What landed.** Both baseline files stop being ratchets. A row of any kind is
  now a hard failure, and the three kinds survive only as the vocabulary of the
  refusal message. The argument for making it a constant rather than leaving an
  empty ratchet is in the baseline file itself and is worth repeating: an empty
  slot is not tolerance for something measured, it is a place to put the NEXT
  violation — and this rule has no legitimate exception, because two TUs of one
  image may disagree about a capability macro (px4 sets `-DNROS_CPP_STD` on a
  single module of a larger image), link anyway, and write an object through one
  layout while reading it through the other. Issue 0135 is that bug, shipped.
  Both gates carry the required mutation in their selftests — **7 cases** for
  `check-cpp-capability-layout` and **15** for `check-cpp-freestanding-includes`,
  including "a row in the baseline file is refused" on each, and verified the
  only way a refusal can be: by putting a row in the tracked file and watching
  each gate exit 1.

  Two things the freestanding gate gains beside the constant. Its mutation runs
  against a REAL header rather than a snippet — a copy of `publisher.hpp` with a
  `std::string` returned from a public signature and the ungated `<string>` that
  needs, with the UNMUTATED copy asserted clean first, because a gate that
  already fires on a faithful copy proves nothing when it fires on a mutant. And
  `scripts/lib/grep-q.sh` is no longer sourced: its two call sites were the
  baseline lookup and the stale-entry sweep, both of which went with the ratchet,
  and a sourced helper nobody calls is a claim about the file that is not true.

  **What did not, and why the count is not being forced to zero.** Re-measured on
  this base, over `packages/api/nros-cpp/include`, after W5:

  | macro | uses | what still needs it |
  | --- | --- | --- |
  | `NROS_CPP_HAS_SHARED_PTR` | 12 | `Client<S>` and `Timer` still alias `std::shared_ptr`; `Node::SharedPtr` does too, inside `NROS_CPP_NODE_HOSTED` |
  | `NROS_CPP_HAS_STD_STRING` | 9 | `get_logger(const std::string&)`, and `FixedString`/`HeapString`'s `std::string` interop |
  | `NROS_CPP_HAS_STD_CHRONO` | 8 | `create_wall_timer` / `create_timer`'s duration overloads and `Rate`'s `std::chrono` constructor |
  | `NROS_CPP_HAS_STD_FUNCTION` | 6 | `detail::WallTimer`'s type-erasure cell, plus the `NROS_CPP_NODE_HOSTED` conjunction |
  | `NROS_CPP_HAS_STD_VECTOR` | 4 | the `NROS_CPP_NODE_HOSTED` conjunction |
  | `NROS_CPP_HAS_STD_SSTREAM` | 4 | the `RCLCPP_*_STREAM` family |
  | `NROS_CPP_STD` | 59 | the consumer-facing opt-in, which nothing that ships defines |
  | `NROS_CPP_NODE_HOSTED` | 21 | derived from four of the above |

  *Read the numbers exactly.* They are OCCURRENCES, not lines: `git grep -c`
  counts lines and disagrees. The six `NROS_CPP_HAS_*` rows are 43 occurrences on
  41 lines, and a bare `git grep -c NROS_CPP_HAS_` reports 41 because two of those
  lines are prose naming the family rather than a macro. `NROS_CPP_STD` is **59**,
  not the 62 a substring grep gives: three of those hits are
  `NROS_CPP_STD_DETECT_HPP`, `std_detect.hpp`'s own include guard, which is not a
  use of the capability macro at all.

  **What W5 moved, measured against `ac7ff02a1^`.** `NROS_CPP_HAS_SHARED_PTR`
  went 17 → 12 and `NROS_CPP_NODE_HOSTED` 20 → 21; every other macro is
  unchanged, `NROS_CPP_STD` included. No macro reached zero. Three FILES did:
  `publisher.hpp`, `service.hpp` and `polling_subscription.hpp` now name
  `NROS_CPP_HAS_SHARED_PTR` zero times, which is the alias flip showing up
  exactly where W5 said it would and nowhere else.

  The remainder is the three items W5's own "what did NOT move" section already
  names, and this is the count of what each costs.
  `Client<S>::SharedPtr` needs a `ClientHandle<S>` plus the `PollClient<S>`
  split, the identical remedy the poll `create_service` collision needed;
  `Timer::SharedPtr` needs W1's capture-in-the-arena treatment for timers, which
  is core Rust work; `Node::SharedPtr` is the whole `NROS_CPP_NODE_HOSTED` block,
  spelled in `std::string` / `std::vector` / `std::function`. The
  `std::string` / `std::vector` / `std::chrono` overloads are a different surface
  again — the ported-source ergonomics RFC-0089 adopts on purpose — and removing
  their gate means giving each a freestanding spelling (`nros::FixedString`, a
  duration type), which is its own phase.

  So W6's zero is blocked on work that does not exist yet, and asserting a
  constant of zero today would mean either deleting surface that ported code uses
  or moving it somewhere the gate does not look. **A gate that reaches zero by not
  looking is worse than a ratchet**, which is the whole reason this item exists.
  The structural half is what W6 delivers; the count stays measured and stated
  here until those items land.

  *Follow-ups this creates:* `Client<S>` as a handle with one verb, with its poll
  half split off (W3's measurement is the input); timer callbacks captured in the
  arena, which is what unblocks `Timer::SharedPtr`; a freestanding spelling for
  the three STL-typed overload families.

  *Verification (2026-09-25, in a worktree with no submodules):* both gates run
  green and both refuse an appended baseline row (exit 1, message naming the row
  and the gated form). The compile sweep over the 61 `tests/compile` probes is
  **46 PASS / 15 FAIL before and after, cell for cell identical** — this commit
  touches no header, so that is the control it should be. `just check fast` is
  351 of 353, the two failures being `capability-conditionals` and
  `xrce-vendored-versions`, each reporting "submodule not checked out" and neither
  touched here. NOT run and not claimed: `just ci` and anything linking an RMW
  backend, because a worktree has no submodules.

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

* It does not change how the poll-style path WORKS. Caller storage is correct
  there — nothing dispatches, so nothing holds the address, and no poll
  operation changed behaviour in this phase.

  **It did move where that path LIVES, and this sentence used to deny it.** W2b
  took the taking API out of `Subscription<M>` into `nros::PollSubscription<M>`,
  because one class cannot be both the thing the arena owns and the thing the
  caller owns. W5 does the same for services, for the reason W3 measured: the
  returning poll `create_service` shares an alias with the dispatch one, so the
  alias cannot become a handle while the class serves both. The distinction
  worth keeping is between changing a path's SEMANTICS, which this phase does
  not do, and separating two owners that shared a name, which is the phase.
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
