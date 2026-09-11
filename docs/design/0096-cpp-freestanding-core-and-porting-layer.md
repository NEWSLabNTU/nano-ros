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
