# Phase 476 — freestanding spellings for the gated C++ overloads

**Status (2026-10-02). Opened.** Carries the half of
[phase-456](archived/phase-456-cpp-api-is-a-handle-over-the-rust-arena.md) W6
that was measured unreachable when that phase closed. Same design as 456 —
[RFC-0096 D9 revision 3](../design/0096-cpp-freestanding-core-and-porting-layer.md)
— plus [RFC-0089](../design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)'s ported-source
ergonomics, which is the surface these overloads exist to serve.

## The goal, and why it was blocked

phase-456 W6 made both C++ capability gates STRUCTURAL: a row in either baseline
is now a hard failure rather than a ratchet. Its acceptance asked for more —

> zero `NROS_CPP_HAS_*`, zero `NROS_CPP_STD`, zero `NROS_CPP_NODE_HOSTED`

— and that count could not be forced to zero, because every remaining use gates
SURFACE that ported code calls. Deleting the surface breaks the ported corpus;
moving it somewhere the gate does not look reaches zero by not looking, which
W6 itself names as worse than a ratchet.

So this phase does not delete gated surface. It gives each gated family a
**freestanding spelling** first, so the hosted overload becomes a convenience
over a freestanding one rather than the only way to say the thing — and only
then retires the gate.

## Where the count stands — re-measured 2026-10-02

Occurrences over `packages/api/nros-cpp/include`, word-bounded
(`git grep -ohw <macro> -- packages/api/nros-cpp/include | wc -l`). Word bounds
matter: they exclude the `NROS_CPP_STD_DETECT_HPP` include guard and the
`-DNROS_CPP_STD` flag named in prose, which is why phase-456 W6's table read 59
for a count this method reads as 57 at the same base.

| macro | uses | what still needs it |
| --- | --- | --- |
| `NROS_CPP_HAS_SHARED_PTR` | 10 | `Timer` aliases `std::shared_ptr` (`timer.hpp:59`, the ONE alias gate); `Node::SharedPtr` inside `NROS_CPP_NODE_HOSTED`; the rest are the definition site, the hosted conjunction and prose |
| `NROS_CPP_HAS_STD_STRING` | 9 | `get_logger(const std::string&)`; `FixedString`/`HeapString`'s `std::string` interop |
| `NROS_CPP_HAS_STD_CHRONO` | 8 | `create_wall_timer` / `create_timer`'s duration overloads; `Rate`'s `std::chrono` constructor |
| `NROS_CPP_HAS_STD_FUNCTION` | 6 | `detail::WallTimer`'s type-erasure cell; the `NROS_CPP_NODE_HOSTED` conjunction |
| `NROS_CPP_HAS_STD_VECTOR` | 4 | the `NROS_CPP_NODE_HOSTED` conjunction |
| `NROS_CPP_HAS_STD_SSTREAM` | 4 | the `RCLCPP_*_STREAM` family |
| `NROS_CPP_NODE_HOSTED` | 21 | derived from four of the above |
| `NROS_CPP_STD` | 58 | the consumer-facing opt-in, which nothing that ships defines |

Every `NROS_CPP_HAS_*` row and `NROS_CPP_NODE_HOSTED` is unchanged since
phase-456 measured them on 2026-09-25. `NROS_CPP_STD` moved by ONE: `c848ea5c99`
(2026-10-01, issues 1256/1564) gated a `<cstdio>` include in `log.hpp:35` the
same way `main.hpp:45` already does. That is a legitimate hosted-only include,
not a regression against this goal — but it is the reason the count has to be
re-measured at the start of every work item rather than carried.

## What is ALREADY freestanding — measured before writing the work items

The first draft of this phase listed "introduce a freestanding duration type" as
W1. Reading the headers refuted it: `nros::Duration` exists
(`nros/duration.hpp:69`), and two of the three chrono-gated families already use
it as their primary spelling, with the `std::chrono` overload as conversion sugar:

* `Rate` — `Rate(double)` and `Rate(::nros::Duration)` are freestanding; only
  `Rate(std::chrono::duration<…>)` is gated (`nros.hpp:1325`).
* `rclcpp::create_timer(node, clock, period, cb)` — the `::nros::Duration`
  overload is the real one; the chrono overload (`nros.hpp:921`) converts and
  delegates to it.

The one chrono-gated site with NO freestanding twin is
`Node::create_wall_timer(period, cb)` (`nros.hpp:660`, declared `node.hpp:913`)
— and it is hosted for TWO reasons, not one: its chrono parameter and its
`std::shared_ptr<Timer>` return. A `Duration` overload alone cannot free it; it
needs a freestanding timer handle to return. So the chrono work is not its own
item — it is the last step of the timer item.

## Work items

* **W0 [core, abi] — entry lifetime: a generation per slot, an owner per entry,
  and node-scoped release.** Added 2026-10-05, ahead of W2, because W2 cannot
  be done safely without it. Measured on `main` at that date:

  - Today a C++ timer is safe only because `rclcpp::Node` keeps a
    `detail::WallTimer` cell in `owned_entities`, and `~nros::Timer` cancels
    the slot when the node goes. W2 deletes that cell. Nothing else stops a
    callback whose capture holds `this` from firing after its node is gone:
    `nros_cpp_node_destroy` is a no-op (`nros-cpp/src/lib.rs`), and an arena
    entry does not record which node registered it.
  - Subscriptions already have that defect. phase-456 W1 moved their capture
    into the arena, and nothing has cancelled them on node destruction since.
  - Releasing a slot is unsafe while handles are copyable (issue 1667). A
    released slot goes to the very next registration (`next_entry_slot`), and
    `HandleId` is a bare index, so a stale copy reaches whatever took the slot.
  - A capture is a separate arena region (`stow_capture`), and
    `release_entry` gives back only the entry's recorded region
    (`CallbackMeta::arena_len`). Releasing a capturing entry therefore leaks its
    capture.
  - `CallbackMeta` has no spare tail padding on a 32-bit target (24 of 24
    bytes), so neither a generation nor an owner can ride inside it the way
    issue 1631 put `arena_len` there.

  The shape:
  1. A new per-slot region, `slot_tags: [SlotTag; cbs]`, carved like
     `sched_context_bindings` and placed by `nros-executor-layout`. A `SlotTag`
     holds a `generation: u16`, bumped on every release, and an `owner: u8`
     (node index + 1, 0 = none).
  2. `HandleId` carries the generation it was issued with, encoded in the same
     `usize` (slot in the low 16 bits, generation above), so no C/C++ ABI type
     changes. Every lookup validates the generation. A stale handle answers
     "not found", never the slot's next occupant. Generations start at 1, so a
     site that still reads `HandleId.0` as an index fails on its first use
     rather than on the first slot reuse.
  3. A registration made for a node records that node. `Executor::release_node`
     releases every entry the node owns, and `nros_cpp_node_destroy` calls it.
  4. A capture is released with its entry: it becomes part of the region the
     entry records, or is recorded beside it, so a release leaks nothing.

  *Acceptance:* issue 1667's stale-handle test (release, register a DIFFERENT
  kind into the reused slot, call through the stale copy) fails loudly and
  never dispatches. Destroying a C++ node stops its subscriptions and timers.
  A create/release loop of capturing entries holds `arena_used` flat. The
  backing growth is stated in bytes per callback slot, and every stated
  executor backing size still builds.

  **Landed 2026-10-05.** What it took, beyond the shape above:

  - `HandleId`'s field is private now. The compiler then listed every site
    that read the packed value as an index: 15 in `nros-node`, 3 in `nros`,
    13 in `nros-c`, 12 in `nros-cpp`, and the unit tests. Each now asks for
    `slot()` (an index), `to_raw()`/`from_raw()` (an FFI value), or goes
    through `Executor::resolve_handle`.
  - Generation 0 is `HandleId::for_owned_slot`: a slot named by its UNIQUE
    holder, accepted while it is live. Action servers and clients and nros-c's
    `nros_service_t`/`nros_client_t` use it. Their owning object is destroyed
    on release, so no copy outlives it; this is the "sound by contract" case
    issue 1667 described for actions. Every handle a registration issues
    carries a generation of at least 1, so a copyable handle can never take
    this path.
  - `issue_owned_handle` (nros-cpp) records a node as owner only for entries
    whose C++ handle is copyable: dispatch subscriptions, service servers and
    clients. Never actions: they release their own entry by slot, and a
    node-scoped release first would let that later self-release reach the
    slot's next occupant.
  - Growth: `SlotTag` is 4 bytes, so 4 slots × 4 B = 16 B on the default
    sizing. The two boards that state a backing (`threadx-linux`,
    `threadx-qemu-riscv64`) moved from 11,069 to 11,071 words. `node-std-tests`
    found that, and its own `cargo check` probe gives the number: 11,070 is
    refused, 11,071 accepted.
  - `check-cpp-destroy-shape` gained a third shape, `EXECUTOR`, for a destroy
    that drops no storage and releases through the executor
    (`nros_cpp_node_destroy`). It has two new self-test cases.

  Evidence: 7 unit tests in `executor/tests.rs`, including the 1667 case for
  both a different kind and the same kind reusing the slot. The capture case
  runs a 200-cycle loop at depth 1 and depth 4 with `arena_used` pinned.
  `node_destroy_releases_entries_runtime` in `just check cpp` destroys node A,
  sees A's two subscriptions leave the stub backend and B's stay, and fails
  3/3 with the release disabled. `just ci gate` is green.

  NOT done here, and still issue 1667's: `SubscriptionHandle<M>::cancel()` and
  the service twins, and the guard-condition release. The generation they
  needed exists now.

* **W1 [cpp] — string spellings.** `get_logger(const std::string&)` and the
  `FixedString`/`HeapString` interop. `nros::FixedString` exists
  (`fixed_string.hpp:39`); the work is making it, or a `const char*`, the primary
  spelling at every gated signature. *Acceptance:* `NROS_CPP_HAS_STD_STRING`
  gates only interop conversions, never a signature that is the only way to call
  something.

  **Landed 2026-10-06.** Measured first: `get_logger(const char*)` was already
  the primary spelling, and the `FixedString`/`HeapString` `std::string` members
  were already interop (copy-and-delegate). What was NOT freestanding was the
  value-returning dispatch verbs. `create_subscription`, `create_service` and
  `create_client` took only a `std::string` key, although their results have
  been freestanding handles since phase-456. Each now has a `const char*`
  overload outside the hosted block, carrying the body the `std::string`
  overload used to have, and the `std::string` overloads forward to it. The
  service and client constraints use a new STL-free
  `nros::tr::is_convertible` instead of `std::is_convertible`. A string literal
  binds the `const char*` overload exactly, so a ported call reaches the
  freestanding form on a hosted target too. `rclcpp_node_freestanding_surface.cpp`
  instantiates all six on its `-nostdinc++` arm.

  What still keeps `std::string` in the hosted conjunction is W3's: the
  `NodeOptions` constructors and storage, and the `std::string`-keyed forwarders
  themselves.

* **W2 [core, cpp] — timers: callback captured in the arena, a freestanding
  handle, and the duration overload.** phase-456 W1 put a subscription's capture
  in the arena; timers never got that treatment, so `Timer` still needs
  `std::function` (`detail::WallTimer`'s type-erasure cell) and aliases
  `std::shared_ptr` (`timer.hpp:59`, the one remaining alias gate). This is core
  Rust work and the largest item here. Its last step is
  `create_wall_timer(::nros::Duration, cb)` returning the freestanding handle, at
  which point the chrono overload becomes sugar like `Rate`'s and
  `create_timer`'s already are. *Acceptance:* `timer.hpp` names
  `NROS_CPP_HAS_SHARED_PTR` and `NROS_CPP_HAS_STD_FUNCTION` zero times, and
  `NROS_CPP_HAS_STD_CHRONO` gates only conversion overloads.

  The arena now has a removal path (`9768795b1d`, issue 1496 resolution 2) — a
  bounded one, 8 released regions, coalescing, first-fit — so unlike phase-456,
  a timer that moves into the arena can be destroyed for real. Wire it; do not
  repeat 456's "no removal path" reasoning, which that commit retired.

  **Landed 2026-10-05**, on W0. Acceptance as measured:

  - `timer.hpp` names `NROS_CPP_HAS_SHARED_PTR`, `NROS_CPP_HAS_STD_FUNCTION`
    and `NROS_CPP_HAS_STD_CHRONO` zero times, and no longer includes
    `std_detect.hpp`.
  - `NROS_CPP_HAS_STD_CHRONO` gates exactly the three conversion overloads:
    `Node::create_wall_timer(chrono)`, `rclcpp::create_timer(chrono)` and
    `Rate(chrono)`.
  - Counts (word-bounded, over `packages/api/nros-cpp/include`):
    `NROS_CPP_HAS_SHARED_PTR` 10 -> 8, `NROS_CPP_HAS_STD_FUNCTION` 6 -> 3,
    `NROS_CPP_NODE_HOSTED` 21 -> 19; the rest unchanged.

  Shape:
  - Executor: `register_timer_c_capturing`, whose capture rides in the timer
    entry's trailing region.
  - FFI: `nros_cpp_timer_create_capturing` (node-owned, clock-typed) and
    `nros_cpp_timer_release`.
  - C++ handle: `nros::TimerHandle`, two words, which
    `Timer::SharedPtr`/`ConstSharedPtr`/`UniquePtr` all name. Its
    `operator->` reaches a separate `nros::TimerOps`, so `timer_->reset()`
    (restart) and `timer_.reset()` (release) stay distinct.
  - C++ verbs: `create_wall_timer(nros::Duration, cb)` and
    `rclcpp::create_timer` are freestanding. `detail::WallTimer`,
    `NodeHosted::owned_entities` and `Node::own_entity` are deleted.

  Found and fixed on the way: a callable byte-copied into the arena was never
  destroyed, while the registering call destroyed ITS copy right after the
  copy. So a capture that owned anything (a `std::shared_ptr`, a
  `std::string`) was destroyed while the arena still dispatched through it.
  This was live for SUBSCRIPTIONS since phase-456 W1. The fix:
  - `CaptureGuard` destroys the arena's copy when its entry is dropped.
  - `InplaceFn::relinquish` lets the caller forget its byte-moved copy.
  - Both the timer and the subscription registrations pass a destroy hook
    (`capture_drop`).

  Evidence:
  - 3 nros-node unit tests: dispatch from the arena's copy, capture release
    over 200 cycles, drop hook exactly once.
  - `arena_capture_lifetime_runtime` in `just check cpp`. It covers the
    capture-alive count, `->cancel`/`->reset`, `.reset()` destroying the
    capture once, a stale copy failing without touching the timer that took its
    slot, node destruction releasing timers and a subscription's capture.
    Disabling the destroy hook fails 3 checks; dropping the subscription's
    `relinquish` fails 2.
  - `rclcpp_node_freestanding_surface.cpp` now instantiates the value-returning
    timer verbs on its `-nostdinc++` arm.

  Cost, stated: an explicit `std::shared_ptr<rclcpp::TimerBase>` member no
  longer binds, which is the same edit phase-456 W2 recorded for
  subscriptions. No template in the tree spells it; ledgered at
  `cpp:TimerHandle`.

* **W3 [cpp] — `Node::SharedPtr` and the `NROS_CPP_NODE_HOSTED` block.** The
  whole block is spelled in `std::string` / `std::vector` / `std::function`, so
  it is sequenced after W1 and W2. *Acceptance:* `NROS_CPP_NODE_HOSTED` is
  derived from nothing, and is deleted.

  **Landed 2026-10-06.** `NROS_CPP_NODE_HOSTED` occurs 0 times in
  `packages/api/nros-cpp/include` (21 at the phase's start). After W1 and W2,
  measured, nothing behind it needed `std::vector` or `std::function`. What it
  gated was three things:
  - the `std::string`-keyed forwarders, now behind `NROS_CPP_HAS_STD_STRING`;
  - `Node::SharedPtr` / `shared_from_this` and the `SharedPtr`-taking
    `spin` / `spin_some` / `spin_until_future_complete`, now behind
    `NROS_CPP_HAS_SHARED_PTR`;
  - members that need no capability at all: `get_node_options`,
    `initialized`, `nros_node`, and the `(name, options)` /
    `(name, ns, options)` constructors. These are now unconditional, with
    `const char*` forms.

  `detail::NodeHosted` and `Node::hosted_` are deleted. Their one remaining
  member was `rclcpp::NodeOptions`, a type with no data members (every
  accessor is a REFUSE-LOUD template), so `get_node_options()` returns a
  constant. `sizeof(Node)` drops by one pointer on every target, and
  `check-cpp-capability-layout` stays OK.

  `NROS_CPP_HAS_STD_VECTOR` and `NROS_CPP_HAS_STD_FUNCTION` gated nothing
  afterwards and are deleted. Their headers stay included under
  `NROS_CPP_STD` for ported files that use `std::bind` / `std::vector`
  without including them. RFC-0096's nine-gate table carries a dated update.

  Counts now (word-bounded, `packages/api/nros-cpp/include`):

  | macro | count |
  | --- | --- |
  | `NROS_CPP_HAS_SHARED_PTR` | 10 |
  | `NROS_CPP_HAS_STD_STRING` | 21 |
  | `NROS_CPP_HAS_STD_CHRONO` | 8 |
  | `NROS_CPP_HAS_STD_FUNCTION` | 0 |
  | `NROS_CPP_HAS_STD_VECTOR` | 1 (prose in `options.hpp`) |
  | `NROS_CPP_HAS_STD_SSTREAM` | 4 |
  | `NROS_CPP_NODE_HOSTED` | 0 |

  `SHARED_PTR` and `STD_STRING` went UP: one conjunction became a gate per
  region, and every region it now guards is interop over a freestanding form.
  That is the "hosted-only family" W5 has to enumerate rather than count.

* **W4 [cpp] — the `RCLCPP_*_STREAM` family.** Stream logging needs a
  formatter. Decide between a freestanding minimal formatter and declaring the
  family hosted-only by design — and if the latter, the decision is written
  here and the gate's refusal message names it, because "hosted-only on
  purpose" and "not yet ported" read the same in a count.

* **W5 [ci] — the gates assert zero.** Only after W1–W4: both gates assert the
  constant phase-456 W6 could not, with the mutation that reintroduces a `std`
  type in a public signature still in their selftests. `NROS_CPP_STD` stays as
  the consumer opt-in if any surface remains hosted by design (W4); the
  acceptance then reads "zero outside the documented hosted-only family", with
  that family enumerated rather than counted.

## What this phase must not do

Reach zero by deleting surface the ported corpus calls, or by moving it out of
`packages/api/nros-cpp/include` where the gate looks. Each work item names the
freestanding spelling it ADDS before the gate it RETIRES.
