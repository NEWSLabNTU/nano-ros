# RFC-0096 — The C++ core is freestanding by construction; porting is a separate hosted layer

**Status:** Draft (2026-09-09)

Restores RFC-0018's freestanding constraint, which the implementation has been
violating since phase-417. Amends RFC-0089 (its clause 1 is re-armed with a
mechanism; three of its statements are corrected). Reverses one decision of
phase-427. Supersedes the deferred phase-438 W2, which was the right instinct
aimed one layer too low.

Home phase: to be opened. Prior phases: 417 (ROS 2 API adoption), 427 (one node
type), 438 (the std surface as an opt-in), 426 (parameters, Rust SSoT).

## The decision

> **The C++ core API is freestanding by construction: no `std` type appears in
> any signature, alias, base class or data member of any core entity, and the
> layout of every core type is identical on every target. Compiling ported
> rclcpp source is a SEPARATE surface that is hosted by declaration, provided as
> its own header set and its own CMake target, and honestly absent on embedded.**

The capability macros — all nine gates — are deleted. The split stops being a
per-translation-unit preprocessor accident and becomes a per-target choice a
build makes once.

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

### Porting real rclcpp source inherently requires the standard library

This refutes the obvious escape — "give the porting layer a freestanding
`SharedPtr` and the whole API can be freestanding". It cannot:

* **7 of 9 handle members in the ported corpus spell `std::shared_ptr<rclcpp::X>`
  directly**, not `X::SharedPtr`. Redefining our alias never reaches them.
* Two templates `std::make_shared` a type we do not own
  (`diagnostic_updater::Updater`), so `<memory>` is in the porting file
  regardless of anything we do.
* `local-msg-package` is compiled **twice** — under nano-ros and under genuine
  ROS 2 Humble via `just colcon-parity` — so its spelling cannot diverge from
  upstream's at all.

**This is the load-bearing fact of the design.** The porting surface is not
freestanding-able. Trying to make it so is what produced nine gates and a
shipping layout hazard. Accepting that it is hosted is what lets the core be
clean.

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

### D1 — The core is freestanding by construction, not by subtraction

No `std` type in any signature, alias, base class or data member of any core
entity. Not gated: **absent**. The core compiles identically with and without a
C++ standard library on the include path, and `sizeof` of every core type is a
constant of the target's ABI, never of a probe.

The test is mechanical and already has a home: `check-cpp-freestanding-includes`
becomes a statement about the core headers with no baseline, and
`check-cpp-capability-layout`'s divergence list becomes structurally empty
rather than a shrink-only ratchet.

### D2 — Porting is a separate, hosted layer

The `rclcpp::` spellings that require `std` live in their own header set and
their own CMake target, enabled by declaration. A build either asks for the
porting layer or does not; there is no third state in which the compiler decides.

RFC-0089's compile-or-conform rule keeps its full force **scoped to this layer**,
which is the only layer that can honestly carry it. Clause 1 is satisfied because
the layer is absent where the constraints bind, and that absence is a
declaration rather than an accident.

### D3 — `rclcpp::Node` is a hosted adapter over the freestanding core node

This is the part that reverses a decision, so it is stated plainly.

C++ has no way to add a data member to a class from another header. A single
node type with hosted-only members is a single type with two layouts — which is
precisely the defect. Therefore the porting layer's `rclcpp::Node` is a distinct
hosted type that derives from or holds the core node, and the core node's layout
never varies.

phase-427 decided "one node type". **That decision holds where it was aimed** —
`ComponentNode` still goes, and the core has exactly one node type, which was the
stated intent ("the Node here is always linked as part of the final image; there
is no distinction like ROS 2 on Linux"). What changes is that the *porting
adapter* is not folded into it. `void* hosted_` was the artefact of trying, and
it goes with the merge.

### D4 — The refusal precedent already exists and is extended

The project already refuses an upstream ownership shape rather than emulating
it: upstream's `void h(const std::shared_ptr<Request>, std::shared_ptr<Response>)`
service callback sits in the tree as a **required-to-fail** compile, because it
costs a per-request heap allocation. `ported_create_publisher_freestanding_probe.cpp`
is a second, asserting the ported factory does not exist freestanding.

D2 generalises that: on a freestanding target the porting spellings are absent,
and absent is a compile error naming the missing name — the loud direction, and
exactly what RFC-0089 calls mechanical.

### D5 — Three things get deleted rather than ported

* **`std_compat.hpp` — 275 lines, dead in every shipping configuration.** It is
  entirely behind `#ifdef NROS_CPP_STD`, included from exactly one place which is
  itself behind the same guard, and nothing that ships defines it. It carries the
  tree's highest `std::` count (71) and contributes 16 of the 50 bucket-(a)
  entities: twelve `const std::string&` forwarders and five
  `std::chrono::milliseconds` forwarders that `.c_str()` / `.count()` into
  functions which already exist ungated. It is a third, orphaned vocabulary
  predating the phase-427 merge — neither the core API nor the porting surface.
  Deleting it costs the shipping product nothing.
* **The nine gates**, replaced by one target-level choice.
* **`NROS_CPP_NODE_HOSTED`**, whose only job was to name the conjunction.

### D6 — Two types are over-gated and become freestanding as they are

Measured, and worth stating because they cost nothing to fix:

* **`rclcpp::NodeOptions`** is gated on STD_STRING ∧ STD_VECTOR, yet 22 of its 23
  members are `static_assert`-only and need no `std` at all. Only `arguments()`
  touches `std::vector<std::string>` — and it is REFUSE-LOUD already, so the
  parameter type is decorative.
* **`rclcpp::Rate` / `WallRate`** are gated on STD_CHRONO, yet `Rate(double hz)`,
  `sleep()` and `reset()` are pure integer arithmetic. Only the duration
  constructor and `period()` touch `<chrono>`, and `nros::Duration` already
  covers both.

### D7 — `Timer::closure_` and `GuardCondition::closure_` become unconditional

The tree has already recorded the rule — *state hides behind a pointer only when
it exceeds a pointer* — and not landed it. The member is 8 bytes either way, so
making it unconditional removes all five divergent subjects at a cost of zero
bytes on the types that matter, and ends the px4 mixed-layout exposure
immediately. This is worth landing ahead of the rest of the RFC.

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

**For an embedded user:** none, today. No embedded C++ image has the hosted
surface now. What changes is that the absence becomes a declaration they can
read rather than a property of their toolchain file.

**For a NuttX user:** the API narrows to the freestanding core. This is the one
real behaviour change in the RFC, and it is the point: NuttX was getting the
hosted API by accident, and RFC-0018's constraints apply there as much as on
FreeRTOS. A NuttX build that genuinely wants the porting layer asks for it.

**For a native user:** none. The porting layer is available; nothing that ships
uses the hosted node API today outside the porting templates.

**For a porter:** unchanged in the normal case. `NrosRclcppCompat.cmake` already
force-includes the compat headers per target, so it is the natural place for the
layer to be selected, and a ported project keeps needing no manual flag.

**Out-of-tree consumers relying on discovery** get a compile error naming a
missing overload. There is no deprecation path — nothing can warn on a macro that
stops being defined — so this needs a changelog entry and a book line.

## Work items

To be cut into a phase. Ordered so that each step is independently green.

* **W1 — `Timer` / `GuardCondition` `closure_` unconditional.** Ends the shipping
  px4 mixed-layout exposure and empties the layout ratchet's `diverges` list.
  Independent of everything else; land first.
  *Acceptance:* `.config/cpp-capability-layout-baseline.txt` has no `diverges`
  row; `sizeof` of all five subjects is identical in all nine configurations.
* **W2 — delete `std_compat.hpp`.** Dead in every shipping configuration.
  *Acceptance:* the tree builds; `just check cpp` green; 71 `std::` occurrences
  gone.
* **W3 — un-gate `NodeOptions` and `Rate`/`WallRate`** (D6), replacing the two
  genuinely-`std` members with `Span<StringView>` and `nros::Duration`.
* **W4 — bucket (a): delete the 50 removable entities**, each of which has an
  ungated sibling or is body-only. No consumer loses a capability.
* **W5 — bucket (b): replace the 27** with the named freestanding equivalents.
  Includes the one gap the census found: `ComponentNode` has no `Seq<T,N>`
  parameter overload, so its `std::vector` form has no sibling to fall back to.
* **W6 — split the porting layer out** (D2, D3): its own headers, its own target,
  `rclcpp::Node` as a hosted adapter, the nine gates deleted.
* **W7 — the gates become structural.** `check-cpp-freestanding-includes` loses
  its baseline; `check-cpp-capability-layout` asserts a constant rather than
  ratcheting a list. A `std` type reaching a core signature becomes a build
  failure, not a review question.

## What this RFC does not do

* It does not remove the porting surface. Compiling upstream source unmodified is
  RFC-0089's goal and it survives intact — it moves to a layer that can honestly
  provide it.
* It does not give nano-ros a freestanding `shared_ptr`. The census says nothing
  in the tree needs shared ownership; inventing a refcounted handle would be
  building a mechanism for a requirement that does not exist.
* It does not settle `RCLCPP_*_STREAM`. A `FixedString<N>` + `operator<<` builder
  is plausible but does not exist, and a `<<` chain over arbitrary user types is
  not fully recoverable without a hosted `ostream`. It stays in the porting layer
  until someone needs otherwise.
* It does not touch the C API or the Rust core.

## Open questions

1. **Does `rclcpp::Node` derive from or hold the core node?** Deriving preserves
   `class X : public rclcpp::Node` for ported code, which the whole corpus uses.
   Holding is cleaner but breaks every ported file. Deriving is almost certainly
   right; it needs one measurement (the core node is non-polymorphic, so the
   derived-to-base conversion the compat CMake's `dynamic_pointer_cast` performs
   should be a static upcast needing no RTTI — inferred, not compiled).
2. **Does the porting layer live in `packages/api/nros-cpp-port/` or in
   `cmake/compat/`?** The latter already exists and is already per-target; the
   former is a clearer statement. Not load-bearing for the design.
3. **Is NuttX's missing `-ffreestanding` deliberate?** It should be answered
   before W6 narrows that target's API. If NuttX genuinely supports the hosted
   surface, it may want the porting layer enabled by default — which is a
   supported configuration under this RFC, just a stated one.
