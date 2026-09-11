# RFC-0096 — One freestanding `rclcpp` API, identical on every platform

**Status:** Draft (2026-09-09), revised the same day — see "Revision 2".

Restores RFC-0018's freestanding constraint, which the implementation has been
violating since phase-417. Amends RFC-0089 (its clause 1 is re-armed with a
mechanism; three of its statements are corrected). Reverses one decision of
phase-427. Supersedes the deferred phase-438 W2, which was the right instinct
aimed one layer too low. **Deletes the compat layer rather than formalising it.**

Home phase: [phase-442](../roadmap/phase-442-one-freestanding-rclcpp-api.md). Prior phases: 417 (ROS 2 API adoption), 427 (one node
type), 438 (the std surface as an opt-in), 426 (parameters, Rust SSoT).

## The decision

> **There is ONE C++ API. It lives in `rclcpp::`, it is freestanding, and it is
> byte-for-byte the same API on every platform — Zephyr, FreeRTOS, NuttX,
> ThreadX and native alike. No `std` type appears in any signature, alias, base
> class or data member. There is no compat layer, no porting layer, and no
> capability macro. A user copies ROS 2 source in and it is a drop-in
> replacement.**

The user is platform-agnostic. Nothing they write, and nothing they read in the
documentation, should depend on which target their image is for.

## Revision 2 (2026-09-09) — the first draft proposed a hosted porting layer, and that was wrong

The first draft of this RFC accepted the freestanding core and then put the
`rclcpp::` spellings in a **separate hosted layer** with `rclcpp::Node` as a
hosted adapter. That is recorded here rather than deleted, because the reason it
was wrong is the useful part.

It rested on one census finding: *7 of 9 handle members in the ported corpus
spell `std::shared_ptr<rclcpp::X>` directly, not `X::SharedPtr`, so redefining
our alias never reaches them.* True — but that corpus is **our own porting
templates**, which we control and can rewrite. It is not a fact about upstream
ROS 2 code, whose house style is the nested alias.

And it applied the wrong success criterion. RFC-0089 does not ask that a ported
file compile byte-identically; it asks that porting be a **mechanical edit —
every difference is one the compiler points at, and the fix is local and
obvious.** Judged against "byte-identical, `std::` spellings included", a hosted
layer is forced. Judged against "mechanical", it is not needed at all, because
the two capabilities that seemed to require `std` have freestanding answers.

A design that gives one platform a different API from another has already failed
the user, whatever it does for the corpus.

## Why this RFC exists

Not because anyone proposed "freestanding, plus `std` where the compiler has
it". Nobody proposed it. It is what the tree drifted into, and the drift is
measurable.

### The defect, in one table

The six capability macros, measured on the toolchains this project ships, using
the real flags from `cmake/toolchain/*.cmake` and the pinned
`arm-none-eabi-gcc 13.2-nros1`:

| toolchain | SHARED_PTR | STD_STRING | STD_VECTOR | STD_FUNCTION | STD_CHRONO | STD_SSTREAM |
| --- | --- | --- | --- | --- | --- | --- |
| **NuttX armv7a** | **ON** | **ON** | **ON** | **ON** | **ON** | **ON** |
| FreeRTOS armcm3 | off | off | off | off | off | off |
| ThreadX rv64 | off | off | off | off | off | off |

**The same product has two different API shapes on two RTOSes.** On NuttX every
`std::shared_ptr`-returning `create_publisher` and every `std::function` timer
callback is live; on FreeRTOS none of them exist. The two targets have the same
allocator story, the same absence of exceptions, the same reasons for every
constraint in RFC-0018.

Nothing decided this. The entire difference is that
`cmake/toolchain/armv7a-nuttx-eabi.cmake:47` does not carry `-ffreestanding` and
`cmake/toolchain/arm-freertos-armcm3.cmake:45` does. A flag chosen for the C
compiler's benefit silently selects which C++ API the user gets.

That is the whole defect. Everything below is consequence.

### It is not one axis but nine

The census found nine distinct gates, not the six that get discussed:

| gate | spelling | sites |
| --- | --- | --- |
| the six named | `#if defined(NROS_CPP_STD) \|\| (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<H>))` | 15 defines |
| 7th — `NROS_CPP_STD` alone | `#ifdef NROS_CPP_STD` | 6 headers, whole-file in two |
| 8th — bare hostedness | `#if defined(NROS_CPP_STD) \|\| (__STDC_HOSTED__ + 0)` | 12 sites |
| 9th — composite | `NROS_CPP_NODE_HOSTED` = SHARED_PTR ∧ STD_STRING ∧ STD_VECTOR ∧ STD_FUNCTION | `node.hpp:161`, mirrored `nros.hpp:379` |

A design that rationalises only the six leaves the other three standing, and the
worst live defect is behind the **7th** (below). phase-438 W2 addressed the six.
That is why it was the right instinct at the wrong layer.

### The layout hazard is not theoretical — it is shipping

`.config/cpp-capability-layout-baseline.txt`, a shrink-only ratchet over 90
subjects measured in nine configurations, currently records five divergent
types:

```
::nros::ComponentNode    55784 -> 55848
::nros::GuardCondition      32 -> 40
::nros::Timer               24 -> 32
::rclcpp::Timer                        (alias of nros::Timer)
::rclcpp::TimerBase                    (alias of nros::Timer)
```

All five come from ONE member, behind the 7th gate:

```cpp
#ifdef NROS_CPP_STD
    ::std::unique_ptr<::std::function<void()>> closure_;   // timer.hpp:169
#endif                                                     // guard_condition.hpp:123
```

And the exposure is real: `examples/px4/cpp/bridge/.../CMakeLists.txt:123` sets
`-DNROS_CPP_STD=1` on **one module** of a larger image. That build genuinely
compiles a 32-byte `Timer` in one translation unit and a 24-byte `Timer` in the
rest, and links them together. This is issue 0135's class, in the shipped tree,
today.

`ComponentNode` inherits it eight times over — `Timer timers_[8]`.

### RFC-0018 already forbade all of it

RFC-0018 §"Freestanding C++" is unambiguous:

> **No STL containers**: no `std::string`, `std::vector`, `std::map`,
> `std::shared_ptr`, `std::function`

and on the std mode:

> the API can optionally expose convenience **overloads** … These are
> `#ifdef`-guarded and **never required**.

Both halves are violated. The std forms are not convenience overloads over a
freestanding API — for `rclcpp::Node`'s factories they are the ONLY form, so
they are required, and on NuttX they are what a user gets by default.

### RFC-0089 already ranked it, and the macros are how the ranking was bypassed

RFC-0089's governing principle is explicit and ordered:

> 1. **RTOS and bare-metal constraints are non-negotiable.** … no exceptions
>    (RFC-0018) … 2. **Within them, port upstream.**

A `std::shared_ptr`-returning factory as the only form is clause 2 overriding
clause 1. The reason nobody argued for that override is that nobody had to: the
capability macros make it happen per-compiler. Clause 1 was never repealed; it
was routed around by a preprocessor.

**This RFC does not change the principle. It gives clause 1 a mechanism, so that
overriding it requires a decision instead of a toolchain flag.**

## What the measurement establishes

Two censuses, both in the tree's own worktrees, both reproducible.

### Shared ownership is fiction, everywhere

This is the fact that makes a freestanding core reachable rather than merely
desirable.

`shared_from_this()` on `main` (`node.hpp:541-547`):

```cpp
::std::shared_ptr<Node> shared_from_this() {
    return ::std::shared_ptr<Node>(::std::shared_ptr<void>(), this);
}
```

The aliasing constructor with an **empty owner**: `use_count() == 0`. It observes
and does not own. The header documents this itself, and adds that upstream
throws `bad_weak_ptr` where this never throws.

`detail::NodeHosted::owned_entities` is `std::vector<std::shared_ptr<void>>`
with seven `push_back` sites, **no `erase`, no `clear`, no removal path**,
draining exactly once in `~Node()`. It is a node-scoped arena wearing a
`shared_ptr` face: it models address stability, not shared ownership.

Across the whole non-vendored tree: zero live `std::weak_ptr`, zero `.lock()`,
zero `use_count()`, zero custom deleters, zero `.reset()` on a smart pointer,
and **not one lambda captures a handle** — every capture is `[this]`, `[state]`
(a raw pointer), `[obj, method]` or `[]`.

Neither `shared_from_this()` call site is ever executed. Of the five porting
templates, only `cpp-port-minimal-publisher` runs; it does not call it.

**And no embedded C++ image has any of this at all.** Nothing that ships defines
`NROS_CPP_STD`, and the conjunction is false under `-ffreestanding`, so
`Node::SharedPtr`, `owned_entities` and every `shared_ptr` are native-and-NuttX
only. The freestanding core already exists. It is simply defined by subtraction
— by what a toolchain lacks — rather than declared.

### The generator already writes the freestanding shape

What every user's image looks like, from
`packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/entry.cpp.jinja`:

```cpp
// Static per-node storage (outlives the spin loop; no heap).
static ::nros::Node __nros_node_0;
static ::talker_pkg::TALKER __nros_comp_0;
```

`grep` over all codegen packs and all 49 goldens for
`shared_ptr|make_shared|SharedPtr|shared_from_this` returns **zero**. `Node` is
non-copyable; its move operations exist and no in-tree site uses them. Entities
are non-copyable value types with inline byte-array storage; `CallbackGroup` is
a `const char*` and nothing else.

### What the ported corpus spells, and why it does NOT force a hosted layer

This is the finding the first draft misread, so it is stated with its correction
attached.

* **7 of 9 handle members in the ported corpus spell `std::shared_ptr<rclcpp::X>`
  directly**, not `X::SharedPtr`. Redefining our alias does not reach them.
* Two templates `std::make_shared` a type we do not own
  (`diagnostic_updater::Updater`).
* `local-msg-package` is compiled **twice** — under nano-ros and under genuine
  ROS 2 Humble via `just colcon-parity` — so its spelling cannot diverge from
  upstream's.

The first draft read this as "porting is not freestanding-able" and reached for a
hosted layer. **The corpus is our own porting templates and our own
`diagnostic_updater` shim.** It is evidence about what we wrote, not about what
upstream ROS 2 code requires — upstream house style is the nested alias, which a
freestanding handle satisfies. Rewriting those members is W9, and it is the last
work item precisely because it is ours to change rather than a constraint.

The third bullet survives as a genuine constraint and is the sharpest one in the
tree: `local-msg-package` must keep compiling under real ROS 2 Humble, so
whatever `X::SharedPtr` becomes, the *spelling at the use site* cannot diverge
from upstream's. That is a constraint on the alias, not an argument for a second
surface.

### The core is 60 % already there, and most of the rest is cheap

127 entities carry `std` in their shape. Bucketed:

| bucket | count | meaning |
| --- | --- | --- |
| **(a) removable** | 50 | `std` only in a body, or an ungated freestanding sibling already does the job |
| **(b) replaceable** | 27 | a freestanding equivalent exists in this tree (`Span`, `StringView`, `FixedString`, `Seq`, `Duration`) or is plainly constructible |
| **(c) genuinely hard** | 50 | no equivalent exists — and every one is in the porting surface |

Bucket (c) is exactly two capabilities, and **18 of its 50 are pure spelling** —
`X::SharedPtr` / `ConstSharedPtr` / `UniquePtr` aliases that name a type rather
than manage a lifetime:

* **c-1, shared ownership** (40): the 15 hosted `Node` members, the 18 aliases,
  `owned_entities`, two `rclcpp::create_timer` overloads, `as_node_ref`, and the
  three `rclcpp::spin*` forms.
* **c-2, type-erased owning callbacks** (10): `Timer::closure_`,
  `GuardCondition::closure_`, the two heap callback cells, the four
  `std_compat` wrappers. The tree already has the **non-owning** half in three
  ungated freestanding forms — raw `void(*)(void*)` + ctx, typed function-pointer
  aliases, and the member-pointer-as-template-parameter `bind_*` family. What has
  no freestanding equivalent is the *capturing lambda*, which is what ported
  source writes.

Both capabilities are needed only by ported code. Neither is needed by the core.

## The design

### D1 — One API, freestanding, identical everywhere

`rclcpp::` is the home and the only vocabulary; RFC-0089 already settled that
`nros::` is phased out and that ours-only names take `rclcpp::` too. No `std`
type in any signature, alias, base or member. Not gated: **absent**. `sizeof` of
every public type is a constant of the target ABI and never of a probe, and the
header set a user reads is the same set on every platform.

This applies to **native too**. A native build gets the freestanding API, not a
richer one. That is the point of the rule: a user who develops on the host and
deploys to an RTOS must not discover the difference at deployment.

### D2 — The two hard capabilities have freestanding answers, and they are measured

The census said 50 entities were genuinely hard, in exactly two capabilities.
Both are constructible without `std`. Measured — a probe carrying an
rclcpp-shaped node body (`class MinimalPublisher : public rclcpp::Node`, a
capturing-lambda subscription callback, and `Publisher<M>::SharedPtr` /
`Subscription<M>::SharedPtr` / `TimerBase::SharedPtr` members) compiles clean in
all three configurations:

```
arm-none-eabi 13.2  -std=c++14 -ffreestanding -fno-exceptions -fno-rtti   rc=0
ThreadX 10-header shim, -nostdinc++                                       rc=0
hosted g++ -std=c++17                                                     rc=0
```

**Ownership → our own handle type.** The census established that shared
ownership is fiction throughout: `shared_from_this()` already returns an
empty-owner aliasing pointer, `owned_entities` is an append-only arena with no
removal path, and the tree has no live `weak_ptr`, no `.lock()`, no custom
deleter and no lambda capturing a handle. So `X::SharedPtr` becomes a nano-ros
handle — a name ported code already writes, backed by the lifetime model the
tree actually has. Nothing is emulated; a mechanism nobody uses is not built.

**Type-erased callbacks → a fixed-capacity inplace callable.** The tree already
has the non-owning half in three ungated freestanding forms (raw
`void(*)(void*)` + ctx, typed function-pointer aliases, and the
member-pointer-as-template-parameter `bind_*` family). What is missing is the
*capturing lambda*, which is what ported source writes. A callable with inline
storage and a compile-time capacity supplies it with no heap and no
`<functional>`; an over-large capture is a `static_assert`, i.e. a compiler-
visible mechanical edit, not a runtime surprise.

### D3 — The API carries its own minimal traits

Measured, and this is why the API must not lean on the standard library even for
metaprogramming: the ThreadX shim's `<type_traits>` is a **58-line stub** with
`enable_if`, `integral_constant` and `is_convertible` and **no `is_same`, no
`decay`**, while `remove_reference` lives in its `<utility>` instead. Its
`<new>` **omits the placement forms**, which are freestanding-guaranteed, so
placement new does not compile against it at all.

A freestanding API cannot assume a shim's shape. It carries the handful of
traits it needs. (Both shim gaps are ours and should also be fixed — placement
new especially, since it is guaranteed and its absence is a trap for any future
in-place construction.)

### D4 — No compat layer

`cmake/compat/` and the whole idea of a second surface go. There is nothing for
them to bridge once the one API is the ROS 2 API.

`std_compat.hpp` goes with them (D5 below) — 275 lines behind a macro nothing
that ships defines, a third orphaned vocabulary predating the phase-427 merge.

### D5 — What is NOT drop-in, enumerated

Honesty about the boundary is what makes the claim usable. Three things require
an edit, and every one is a compile error naming the exact site:

1. **An explicit `std::shared_ptr<rclcpp::X>` spelling** where upstream house
   style writes `rclcpp::X::SharedPtr`. The alias works; the explicit `std::`
   spelling cannot. This is the edit our own templates need, and it is why
   examples and fixtures are in scope later rather than now.
2. **`std::make_shared<MyNode>()` in `main`.** `main` is already not drop-in —
   RFC-0089 refuses `rclcpp::init(argc, argv)`'s two-argument form loudly today,
   so a ported `main` already requires attention. The node *class body*, which is
   the bulk of any port, is what must be drop-in and is.
3. **A lambda capture larger than the declared budget**, which is a
   `static_assert` naming the knob.
4. **Copying a PUBLISHER handle** (phase-442 W8). A dispatch entity's
   `X::SharedPtr` is `nros::Handle`, which copies like upstream's. A publisher
   has no arena slot, so its handle is `nros::Owned<T>` and is MOVE-ONLY; a copy
   is a compile error at the call site. The W0 census found no entity handle
   copied anywhere in this tree or in the porting corpus — handles are stored as
   members, and the one by-value pass is a NODE handle, which is
   `nros::Handle<Node>` and copyable — so this is on the list because it is a
   difference, not because it has cost anything measured.

What this does not promise: an arbitrary third-party ROS 2 package that uses
`std::string` internally will not become freestanding because our API is. The
claim is about code written against the rclcpp API, not about the whole ROS
ecosystem.

### D6 — Two types are over-gated and become freestanding as they are

Measured, and they cost nothing to fix:

* **`rclcpp::NodeOptions`** is gated on STD_STRING ∧ STD_VECTOR, yet 22 of its 23
  members are `static_assert`-only and need no `std` at all. Only `arguments()`
  touches `std::vector<std::string>` — and it is REFUSE-LOUD already, so the
  parameter type is decorative.
* **`rclcpp::Rate` / `WallRate`** are gated on STD_CHRONO, yet `Rate(double hz)`,
  `sleep()` and `reset()` are pure integer arithmetic. Only the duration
  constructor and `period()` touch `<chrono>`, and `nros::Duration` covers both.

### D7 — `Timer::closure_` and `GuardCondition::closure_` become unconditional

The tree has already recorded the rule — *state hides behind a pointer only when
it exceeds a pointer* — and not landed it. The member is 8 bytes either way, so
making it unconditional removes all five divergent subjects at zero cost and
ends the px4 mixed-layout exposure immediately. Independent of everything else;
land it first.

### D8 — A name parameter is DEDUCED, so `std::string` stays drop-in (amendment, 2026-09-12)

This decision was missing, and implementing W6 is what surfaced it. Stated here
because W7 and W8 cannot proceed without it.

**The gap.** Every hosted `create_*` in `nros.hpp` takes `const std::string&`,
which is upstream's spelling, and the whole family sits behind
`NROS_CPP_NODE_HOSTED`. W8's acceptance is zero `NROS_CPP_HAS_*` in the tree, so
those overloads cannot survive as gated methods. But D5 — "what is NOT drop-in,
enumerated" — does not list `std::string` arguments, and deleting the overloads
would silently add a fourth item to that list: every ported call site holding a
`std::string` name would need `.c_str()`.

Three candidate answers, and only one satisfies both halves of the goal:

| | freestanding-clean | `std::string` call site still compiles |
| --- | --- | --- |
| keep the overloads, gated | no | yes |
| delete them, require `.c_str()` | yes | **no** — a fourth D5 item |
| **deduce the parameter** | **yes** | **yes** |

**The decision: the parameter is deduced, and one overload set converts it.**

```cpp
namespace nros { namespace detail {
inline const char* as_c_str(const char* s) { return s; }
template <typename S> auto as_c_str(const S& s) -> decltype(s.c_str()) { return s.c_str(); }
}}

template <typename M, typename S>
Publisher<M>::SharedPtr create_publisher(const S& topic, const QoS& qos);
```

The header names no `std` type. A caller passing a string literal, a
`std::string`, our `FixedString`, our `HeapString`, or anything else with
`c_str()` all bind. This is the same move W5 already made for
`NodeOptions::arguments()`, generalised — and there it was measured to be
strictly better for a porting user as well, because the refusal could speak
where an overload-resolution failure used to happen first.

**Measured**, not reasoned — one probe, three configurations:

```
hosted g++ -std=c++17, caller passes std::string          rc=0
ThreadX shim -nostdinc++, no <string> reachable at all    rc=0
arm-none-eabi -std=c++14 -ffreestanding (32-bit)          rc=0
```

**One cost, stated.** An argument that is neither a `const char*` nor a type
with `c_str()` produces a diagnostic one frame inside the template rather than
at the call site:

```
error: no matching function for call to 'as_c_str(const NotAName&)'
note: required from here    <-- the call site, one frame up
```

`as_c_str` is an overload SET rather than a trait precisely to keep that to one
frame; W8 should add a `static_assert` in front of it so the first line names
the parameter instead.

**What this does not do.** It does not make a `std::string` RETURN drop-in —
`get_fully_qualified_name` and friends are a separate question, and they are
returns rather than parameters, so a caller writing `auto` is unaffected while a
caller writing `std::string s = ...` is not. That is W8's to enumerate.

### D9 — Entity STORAGE is W8's first decision, and it is an ABI question, not a C++ one (amendment, 2026-09-12; revised the same day)

The handle in D2 answers *what `X::SharedPtr` is*. It does not answer *what the
handle points at*, and the hosted path's current answer — `std::make_shared`
plus `detail::NodeHosted::owned_entities` — is one this design removes.

**Revision note.** The first version of D9 framed this as a C++ storage problem
and offered three C++-side answers. That framing was wrong, and the correction
came from the obvious question: *isn't the C++ API a thin wrapper over Rust,
with the entity lifetime owned by a Rust data structure?* That is the stated
intent, and measuring the ABI showed it is true of one entity kind out of eight.
The primary candidate below is the consequence.

#### What the C ABI actually does

Every `create` entry point in `nros_cpp_ffi.h`, classified:

| entry point | shape |
| --- | --- |
| `nros_cpp_timer_create` (and `_on_clock`, `_oneshot`, `_in_group`) | **arena** — returns `size_t *out_handle_id` |
| `publisher`, `subscription`, `service_server`, `service_client`, `action_server`, `action_client`, `guard_condition` | **caller storage** — takes `void *storage` |

The header states the caller-storage contract in as many words: *"`storage` must
point to a buffer of at least `NROS_PUBLISHER_SIZE` bytes… the `RmwPublisher`
handle is written directly into this buffer."*

So the C++ object is not a handle to a Rust-owned entity; it IS the entity's
memory:

```cpp
class Publisher {                        // 824 bytes
    alignas(8) uint8_t storage_[NROS_PUBLISHER_SIZE];   // the RmwPublisher lives HERE
    char topic_name_[PUBLISHER_TOPIC_NAME_MAX];
    bool initialized_;
};

class Timer {                            // 32 bytes -- the intended shape
    void* executor_;
    size_t handle_id_;
    bool initialized_;
    void* closure_;
};
```

And the sizes follow from the shape, not from the entities:

| entity | bytes | shape |
| --- | --- | --- |
| `Timer` | 32 | arena handle |
| `Service<int>` | 560 | caller storage |
| `Publisher<int>` | 824 | caller storage |
| `Subscription<int>` | 888 | caller storage |
| `Client<int>` | **4 672** | caller storage |

`Client<int>` is not large because a client is large. It is large because the
C++ object holds the reply buffer.

#### The requirement that survives either way

Whatever holds the bytes, **address stability is a contract, not a fiction.**
The census was right that `owned_entities` — a `std::vector<std::shared_ptr<void>>`
with eight `push_back` sites, no `erase`, no `clear`, one drain in `~Node()` —
models address stability rather than shared ownership. But the executor arena
stores a raw pointer as its dispatch context and has **no unregister path**, so
an entity destroyed while the arena still holds its address turns the next
`spin_once()` into a dispatch through freed memory. Removing `make_shared`
removes the mechanism; it does not remove the requirement.

#### The primary candidate: make the other seven look like the timer

Give the Rust side an arena for the type-erased `Rmw*` entities, sized by the
same derived counts that already produce `NROS_CPP_EXECUTOR_STORAGE_SIZE`. Every
C++ entity then becomes `{node_or_executor, handle_id}`.

This dissolves D9 rather than answering it:

* `nros::Handle<T>` is trivially correct, because C++ owns nothing to begin with;
* the lifetime is a Rust data structure's, which is what the layering intends;
* the sizing is already derived, so no new knob and no new authored number;
* `Client<int>` stops being 4 672 bytes of C++ object, which is what made every
  pooling answer look unaffordable.

**Why the obvious objection does not hold.** The natural reading is that the
arena's entries are generic and so cannot cross a C ABI — and the Rust-native
entries genuinely are (`SubInfoEntry<M, F, const RX_BUF: usize>`,
`SrvEntry<Svc, F, REQ_BUF, REPLY_BUF>`). But the C++ path does not use those. It
creates TYPE-ERASED `Rmw*` objects selected by `type_name` / `type_hash`
strings, and those are not generic. `nros_cpp_timer_create` is the existence
proof that an arena slot works on this path.

**Two things that must be settled before it is adopted, and neither is ours
alone.** First, whether caller storage was CHOSEN or accreted — RFC-0022's
nearest line is that C "can't size inline storage at runtime", which argues for
derived sizes and says nothing about which side holds the bytes. Second, the
cost: the entity bytes do not vanish, they move from the C++ object into the
arena, and the subtraction has to be MEASURED — issues 1145 and 1171 record
exactly this trap for the executor backing, where bytes leaving the allocator
arena and becoming linker-visible were reported as growth by everyone who
forgot to subtract what they replaced.

It is also not a C++ change: it touches the C ABI, the committed bindgen output
(RFC-0054), the Rust core, and arguably `nros-c`, which puts BOTH its publisher
and its timer in caller-declared structs and so is a third shape for one
concept. **Filed as issue 1335, against RFC-0022, because that is where the
answer belongs and it is larger than phase-442.**

#### The alternatives, if caller storage turns out to be deliberate

1. *Caller-owned storage, no pool.* Already ungated and already works —
   `Result create_publisher(Publisher<M>& out, const char* topic, const QoS&)`.
   Cheapest, and NOT drop-in: ported code writes
   `auto pub = node->create_publisher<M>(...)`, so making this the only form
   adds a fifth item to D5's list and changes the shape of every node body,
   which is the code this RFC promises is drop-in.
2. *A fixed pool per entity kind, as a node template parameter.*
   `nros::NodeWithTimers<N>` is this shape already (phase-427 W4). It puts
   entity counts in the type the USER writes, which upstream's `rclcpp::Node`
   does not have, so `: public rclcpp::NodeWith<1, 0, 1>` is not a mechanical
   edit of `: public rclcpp::Node`.
3. *Counts derived from the contract*, the way phase-412 already derives the
   executor storage size. Keeps the user's spelling identical to upstream's and
   costs them nothing to write; the heaviest of the three, and the closest to
   the primary candidate without the ABI change.

#### RESOLVED (revision 3, 2026-09-12): the Rust arena already owns the dispatch entities

The question was *where does a returned `X::SharedPtr` handle's entity live*, and
two revisions of this section looked for somewhere in C++ to put it — a pool,
then a Rust arena to be built. Both were the wrong question. The C/C++ API is a
thin wrapper over the Rust API, entity lifetime is a Rust data structure's, and
**for the rclcpp dispatch model the Rust side already owns the entity.** The
tree documents this at `packages/api/nros-cpp/src/subscription.rs`:

> arena (rclcpp dispatch model), as opposed to the poll-style
> `nros_cpp_subscription_create` above. **The arena owns the subscriber**; spin …

There are two ABI paths per entity, and only one of them is caller-storage:

| path | entry point | owner |
| --- | --- | --- |
| poll-style | `nros_cpp_subscription_create(…, void *storage)` | the caller |
| **rclcpp dispatch** | `nros_cpp_subscription_register(…, out_handle_id)` | **the arena** |

and the arena's C-facing entry already holds everything:

```rust
pub(crate) struct SubBufferedRawCEntry {
    pub(crate) handle: session::RmwSubscriber,      // the subscriber
    pub(crate) buffer: BufferStrategy,              // the rx buffer
    pub(crate) callback: RawSubscriptionCallback,
    pub(crate) context: *mut core::ffi::c_void,
}
```

**So `X::SharedPtr` for a dispatch entity is `nros::Handle` over
`{executor, handle_id}`** — exactly what `Timer` is today, and exactly what W3
already built. The C++ `Subscription<M>`'s 888 bytes of `storage_`, and the
heap `detail::SubscriptionCallback<M>` cell that today holds a `std::function`,
are a second copy of state the arena already keeps. They accumulated; they were
not decided.

Four things follow, and every one of them is a simplification:

* **No ABI addition, no new arena, no pool, no node template parameter.** The
  entry point exists and returns a handle id.
* **`X::SharedPtr` is COPYABLE**, because a handle is. That removes the
  move-only difference from D5 rather than adding one.
* **The "must not move after register" hazard loses its subject.**
  `service.hpp`'s warning — *"the arena holds `this` as the trampoline context…
  the move only transfers bookkeeping and leaves that pointer stale, so don't"*
  — is about a C++ object that, under this shape, does not exist.
* **`sizeof` stops being a design input.** The 4 672-byte `Client<int>` that made
  every pooling answer look unaffordable was the C++ object holding the reply
  buffer; as a handle it is two words.

**The one genuinely new piece.** `context` is a single pointer. That carries a
`[this]` capture — 7 of the 11 capture sites the W0 census found — and does not
carry `[obj, method]`, which is three pointers. So a capturing lambda needs
somewhere for its bytes, and by the same principle that somewhere is the
registration, in Rust. The mechanism is already there: the arena allocates
trailing bytes today for the rx buffer
(`arena_alloc_with_trailing::<SubBufferedRawCEntry>(trailing_bytes)`), so the
capture is the same allocation with a larger tail and `InplaceFn`'s invoker
becomes the `callback` field. That is the only Rust-side work this design needs.

**What `nros::Owned<T>` is for, then.** The entities with NO dispatch, where no
arena slot exists and the C++ object genuinely is the entity: publishers, and
the poll-style forms. It is a fallback for one kind, not the shape of the API,
and `nros/owned.hpp` says so at the top. Whether publishers should get an arena
slot too, for uniformity, is open and small.

**What remains per entity kind.** `nros_cpp_service_server_register` already
returns a handle id, so the slot exists — but it is handed `&out`, the C++
object, as its context. Moving that state into the arena entry the way
subscriptions would removes the back-reference. Same audit for the action forms.

Issue **1335** narrows accordingly: not "where should entity storage live", but
"the C++ API uses the poll path where it means the dispatch path, and carries
storage for both".

#### What the alternatives above are now for

They are the record of two wrong framings, kept because the question that
retired them — *why is the C++ API inventing storage when it is a thin wrapper
over a Rust API that already has it?* — is the one worth asking first next time.

#### W8 does not start until this is answered

The remaining 129 gated sites are substitution once it is. Substitution on top
of an unanswered lifetime question is how a use-after-free ships — and here the
failure mode is a raw arena pointer into freed memory, surfacing inside
`spin_once()` several frames from the cause.

## What this corrects in existing documents

Three statements are wrong in the tree today and are corrected here rather than
left for the next reader to trip over.

1. **RFC-0089 specifies `bind_shared`, which does not exist.** RFC-0089:1387-1392
   decides `shared_from_this()` survives "over a `std::weak_ptr<Node>` held in
   `detail::NodeHosted`, populated by an explicit `bind_shared(std::shared_ptr<Node>)`
   call". `grep` over the non-vendored tree hits only that RFC line. What shipped
   instead is the empty-owner aliasing pointer above.
2. **The capability predicate is over-tight on two headers.** `<memory>` and
   `<functional>` *do* compile under `arm-none-eabi -ffreestanding`; `<string>`,
   `<vector>`, `<chrono>` and `<sstream>` do not. FreeRTOS therefore loses the
   `SharedPtr` aliases and `std::function` callbacks for a reason that is not
   "the header is unusable". The predicate is a proxy for hostedness, and will
   remain one while `__STDC_HOSTED__` is a conjunct. Under this RFC that stops
   mattering, because nothing is probed.
3. **phase-438 W2's own framing.** It argued the surface should be REQUESTED
   rather than discovered, and it was right about that. It was wrong that
   requesting it is sufficient: measured, forcing `NROS_CPP_STD` on a
   `-ffreestanding` build reintroduces issue 1187's `#error` exactly. The
   request has to select a *layer*, not a macro — which is D2.

## Consequences

**For an embedded user:** none today, and a guarantee from here. No embedded C++
image has the hosted surface now; what changes is that its absence stops being a
property of their toolchain file.

**For a NuttX user:** the API narrows to the one API. NuttX has been getting the
hosted shape by accident — its toolchain file has never carried `-ffreestanding`,
in either NuttX variant, since both toolchain files were created in the same
commit and only FreeRTOS got the flag. RFC-0018's constraints apply there as much
as on FreeRTOS.

**For a native user:** the API narrows too, and that is deliberate rather than
collateral. A user who develops on the host and deploys to an RTOS must not
discover a difference at deployment. This is the clause the first draft did not
have.

**For a porter:** the node class body ports unedited. `main` needs the same
attention it already needs. Explicit `std::shared_ptr<...>` spellings become the
nested alias — one mechanical edit per member, named by the compiler.

**Out-of-tree consumers relying on the discovered macros** get a compile error
naming a missing overload. There is no deprecation path — nothing can warn on a
macro that stops being defined — so this needs a changelog entry and a book line.

## Work items

To be cut into a phase. Ordered so that each step is independently green.

* **W0 — MEASURE the two knobs before building the mechanisms.** Neither
  freestanding replacement can be designed from first principles: both carry a
  compile-time budget, and a budget guessed wrong is either a wall of
  `static_assert`s or silent `.bss` growth in every image.
  - *Capture sizes:* every callback in the tree and in the porting corpus —
    lambdas passed to `create_subscription` / `create_wall_timer` /
    `create_service` / `create_client` / guard conditions — sized with
    `sizeof(decltype(lambda))`, reported as a distribution rather than a
    maximum. The census already found every capture is `[this]`, `[state]`,
    `[obj, method]`, `[this, state]` or `[]`, so the distribution is expected to
    be narrow and the default should be the point that covers it with one
    pointer of headroom, not the widest thing anyone might write.
  - *Handle size and copyability:* what `X::SharedPtr` must be to satisfy every
    use site, including `diagnostic_updater`'s by-value parameter, and whether
    a copyable non-owning handle suffices (open question 3).
  - *Cost of the budget:* `.bss` and `.text` delta on a real image at several
    candidate capacities, via `just mem-report`, so the default is chosen
    against a number rather than a feeling.
  *Acceptance:* a table of measured capture sizes; a chosen default with the
  image-size cost of that choice stated; the knob named, so the `static_assert`
  has something to point at.

* **W1 — `Timer` / `GuardCondition` `closure_` unconditional.** Ends the shipping
  px4 mixed-layout exposure and empties the layout ratchet's `diverges` list.
  Independent of everything else; land first.
  *Acceptance:* `.config/cpp-capability-layout-baseline.txt` has no `diverges`
  row; `sizeof` of all five subjects identical in all nine configurations.
* **W2 — delete `std_compat.hpp`.** Dead in every shipping configuration.
* **W3 — the two freestanding mechanisms.** The handle type behind the
  `X::SharedPtr` aliases, and the fixed-capacity inplace callable, both carrying
  the API's own minimal traits (D3).
  *Acceptance:* the rclcpp-shaped probe compiles in all three configurations —
  `arm-none-eabi -ffreestanding`, the ThreadX `-nostdinc++` shim, and hosted —
  and an over-large capture fails with a `static_assert` naming the knob.
* **W4 — fix the two shim gaps.** Placement `operator new` in the ThreadX shim's
  `<new>`, and its `<type_traits>` stub. Independent of W3 because D3 makes the
  API not depend on them, but both are traps for the next in-place construction.
* **W5 — un-gate `NodeOptions` and `Rate`/`WallRate`** (D6).
* **W6 — bucket (a): delete the 50 removable entities**, each of which has an
  ungated sibling or is body-only. No consumer loses a capability.
* **W7 — bucket (b): replace the 27** with the named freestanding equivalents.
  Includes the one gap the census found: `ComponentNode` has no `Seq<T,N>`
  parameter overload, so its `std::vector` form has no sibling to fall back to.
* **W8 — bucket (c): the hosted `rclcpp::` surface moves onto the freestanding
  mechanisms**, and the nine gates and `cmake/compat/` are deleted.
* **W9 — examples and fixtures follow.** Explicit `std::shared_ptr<rclcpp::X>`
  members in our own porting templates become the nested alias. Deliberately
  last: the templates are the corpus that made the first draft reach for a hosted
  layer, and they are ours to change.
* **W10 — the gates become structural.** `check-cpp-freestanding-includes` loses
  its baseline; `check-cpp-capability-layout` asserts a constant rather than
  ratcheting a list. A `std` type reaching any public signature becomes a build
  failure, not a review question.

## What this RFC does not do

* It does not remove the ROS 2 API. Compiling upstream source is RFC-0089's goal
  and it survives intact — it stops being conditional on the toolchain.
* It does not give nano-ros a refcounted smart pointer. The census says nothing
  in the tree shares; building a mechanism for a requirement that does not exist
  would be inventing work.
* It does not promise that an arbitrary third-party ROS 2 package becomes
  freestanding. The claim is scoped to code written against the rclcpp API.
* It does not settle `RCLCPP_*_STREAM`. A `FixedString<N>` + `operator<<` builder
  is plausible and does not exist; a `<<` chain over arbitrary user types is not
  fully recoverable without a hosted `ostream`. The printf family is unaffected.
* It does not touch the C API or the Rust core.

## Open questions

1. ~~**What is the default inplace-callable capacity?**~~ **ANSWERED by W0,
   2026-09-11: `NROS_CPP_CALLBACK_CAPACITY`, default `4 * sizeof(void*)`** — 16
   bytes on a 32-bit target, 32 on a 64-bit one. Every capture in the tree and
   the porting corpus measures a whole number of pointers, and the widest is
   three (`[obj, method]`, where the pointer-to-member-function is itself two
   words on the Itanium ABI), so the default is the widest plus one pointer of
   headroom and is spelled in pointers rather than bytes. Cost, measured on
   eight registered callbacks: 192 bytes of `.bss` on cortex-m3, 320 on
   riscv64, with `.text` flat across every candidate capacity. The full tables
   are in phase-442's "W0's measurements".
2. **Is `NuttX`'s missing `-ffreestanding` deliberate?** Traced: both toolchain
   files were created in one commit, FreeRTOS got the flag, and neither NuttX
   file has ever carried it. That reads as an inconsistency rather than a
   decision, but intent and capability come apart here — NuttX ships a fuller
   libc than FreeRTOS. Under this RFC it stops changing the API either way; the
   flag should still be made deliberate.
3. ~~**Does `X::SharedPtr` need to be copyable?**~~ **ANSWERED by W0,
   2026-09-11: yes, and a copyable NON-OWNING handle is sufficient.** It is no
   longer inferred from a signature. The corpus stores handles as members and
   passes exactly one by value (`diagnostic_updater::Updater`'s constructor),
   and it contains no `weak_ptr`, no `use_count()` and no `reset()` outside our
   own headers. The decisive measurement is that shared ownership is already
   fiction at the one site where it looked real: `Node::shared_from_this()`
   returns `std::shared_ptr<Node>(std::shared_ptr<void>(), this)` — the
   aliasing constructor with an empty owner — so what the corpus compiles
   against today is already a non-owning handle wearing `shared_ptr`'s
   spelling. W3 must reproduce copy, assign, `->`, `*`, default-construct and a
   null test, and needs no control block.
