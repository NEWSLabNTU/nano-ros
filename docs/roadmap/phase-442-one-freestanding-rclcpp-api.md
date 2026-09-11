# phase-442 — one freestanding `rclcpp` API, identical on every platform

**Status (2026-09-09). Opened.** Home phase for
[RFC-0096](../design/0096-cpp-freestanding-core-and-porting-layer.md). Absorbs
what remains of phase-438 and reverses one work item of phase-427. Implements
the constraint RFC-0018 has stated since it was written and the implementation
has been violating since phase-417.

## The goal, in the owner's words

> The API must be consistent on all platforms. The user should be platform
> agnostic. Hence the freestanding rule applies to the API and all platforms.
> We'll only have the `rclcpp` module and it's the home. There is no compat
> layer. My point is that the user can copy ROS 2 code and get a drop-in
> replacement.

## Why now

The API's shape is currently decided by a toolchain flag. Measured on the
toolchains this project ships:

| toolchain | all six capability macros |
| --- | --- |
| NuttX armv7a | **ON** |
| FreeRTOS armcm3 | off |
| ThreadX rv64 | off |

The entire difference is that `armv7a-nuttx-eabi.cmake:47` omits
`-ffreestanding` and `arm-freertos-armcm3.cmake:45` carries it. Both files were
created in one commit; only FreeRTOS got the flag; neither NuttX file has ever
had it. Same product, two API shapes, same allocator story.

And one member behind a seventh, uncounted gate makes five types diverge in
`sizeof`, in a configuration that ships: `examples/px4/cpp/bridge/…` sets
`-DNROS_CPP_STD=1` on **one module** of a larger image, linking a 32-byte
`Timer` against 24-byte ones. Issue 0135's class, live.

## Work items

Ordered so each step is independently green. W0 and W1 are worth doing before
the rest and do not depend on each other.

* **W0 [measure] — size the two knobs before building the mechanisms.**
  Neither freestanding replacement can be designed from first principles; both
  carry a compile-time budget, and a budget guessed wrong is either a wall of
  `static_assert`s or silent `.bss` growth in every image.
  - Every callback in the tree and in the porting corpus, sized with
    `sizeof(decltype(lambda))` and reported as a DISTRIBUTION, not a maximum.
    The census found every capture is `[this]`, `[state]`, `[obj, method]`,
    `[this, state]` or `[]`, so the default should cover the distribution with
    one pointer of headroom rather than the widest thing anyone might write.
  - What `X::SharedPtr` must be to satisfy every use site, including
    `diagnostic_updater`'s by-value parameter — specifically whether a copyable
    non-owning handle suffices.
  - `.bss` / `.text` delta on a real image at several candidate capacities via
    `just mem-report`, so the default is chosen against a number.
  *Acceptance:* a table of measured capture sizes; a chosen default with the
  image-size cost of that choice stated; the knob named, so the `static_assert`
  has something to point at.

* **W1 [cpp] — `Timer::closure_` and `GuardCondition::closure_` unconditional.**
  Ends the shipping px4 mixed-layout exposure. The member is 8 bytes either way,
  so this costs nothing on the types that matter. Independent of everything else.
  *Acceptance:* `.config/cpp-capability-layout-baseline.txt` has no `diverges`
  row; `sizeof` of all five affected subjects identical in all nine
  configurations the layout gate measures.

* **W2 [cpp] — delete `std_compat.hpp`.** 275 lines, 71 `std::` occurrences,
  entirely behind `#ifdef NROS_CPP_STD`, included from exactly one place itself
  behind the same guard, and nothing that ships defines that macro. Dead in
  every shipping configuration; a third orphaned vocabulary predating the
  phase-427 merge.
  *Acceptance:* the tree builds; `just check cpp` green.

* **W3 [cpp] — the two freestanding mechanisms**, sized by W0: a handle type
  behind the `X::SharedPtr` aliases, and a fixed-capacity inplace callable,
  both carrying the API's own minimal traits (RFC-0096 D3).
  **The MECHANISMS landed 2026-09-12** — `nros/traits.hpp` (`nros::tr`),
  `nros/handle.hpp` (`nros::Handle<T>`, one pointer, copyable, converts to
  `Handle<const T>`) and `nros/inplace_fn.hpp`
  (`nros::InplaceFn<Sig, Cap = NROS_CPP_CALLBACK_CAPACITY>`), gated by
  `check-cpp-freestanding-mechanisms` on the fast line: three toolchain arms
  and an over-budget probe whose DIAGNOSTIC TEXT is checked, not merely its
  failure. **Putting them BEHIND the `X::SharedPtr` aliases and the callback
  parameters is W8's job**, because it is the same edit — one API — and doing
  half of it would leave the tree with two spellings of a handle, which is the
  state this phase exists to end.
  *Acceptance:* an rclcpp-shaped node body — derived `rclcpp::Node`, a
  capturing-lambda subscription callback, and `Publisher<M>::SharedPtr` /
  `Subscription<M>::SharedPtr` / `TimerBase::SharedPtr` members — compiles in
  all three configurations (`arm-none-eabi -ffreestanding`, the ThreadX
  `-nostdinc++` shim, hosted), and an over-budget capture fails with a
  `static_assert` naming the knob. A prototype already does; this is that
  prototype made real.

* **W4 [boards] — fix the two shim gaps. LANDED 2026-09-11.** The ThreadX `cxx-compat` shim's
  `<new>` omits the PLACEMENT forms, which are freestanding-guaranteed, so
  placement new does not compile against it at all; its `<type_traits>` is a
  58-line stub with `enable_if` but no `is_same` and no `decay`, and its
  `remove_reference` lives in `<utility>` instead. Independent of W3 because D3
  makes the API not depend on them — but both are traps for the next in-place
  construction, and one of them is a guaranteed facility.
  *Acceptance:* placement new compiles against the shim; a probe using the
  freestanding-guaranteed `<type_traits>` contents compiles. **Met, and the
  reach widened twice on measurement.** The gaps were not two but three:
  Zephyr's own minimal libcpp `<new>` is a stub that declares `nothrow_t` and
  nothing else, so a Zephyr C++ build could not do placement new either —
  `zephyr/cxx-compat/new` now supplies the forms, layered with `include_next`
  and guarded on `__GLIBCXX__` / `_LIBCPP_VERSION` so it never replaces a real
  library's. And the trait widening is MIRRORED into `zephyr/cxx-compat`,
  whose own comment says it mirrors the board shim: two fallbacks that are one
  implementation under two names, where a trait added to one and not the other
  is a difference between boards nothing measures. `remove_reference` moved
  from `<utility>` to `<type_traits>` in both, with `<utility>` including it,
  so there is one definition and it is where the standard puts it.
  Gated by `check-cxx-compat-shim-facilities` on the FAST line — its sibling
  `cxx-compat-shim-coverage` asks whether every `std::` C-library name a source
  spells is exported, which is a different question and cannot see a facility
  nobody has written code against yet. It MEASURES by compiling a probe rather
  than scanning text, because `decay` written as
  `remove_cv_t<remove_reference_t<T>>` is right for scalars and wrong for
  exactly the function and array types a callback signature uses, and runs two
  negative controls on the normal path.
  **And one thing the fix BROKE, caught before it merged.** `component.hpp`
  carried its own hand-rolled placement `operator new` behind Zephyr's include
  guard (issue 1317's workaround), so the moment the shim supplied the real
  ones the two collided — `redefinition of 'void* operator new(size_t, void*)'`
  on every Zephyr C++ component build. The local workaround is deleted, which
  is the class fix 1317 could not make from inside one header, and the gate
  gained a CONSUMER probe: it now compiles `nros/component.hpp` against the
  Zephyr shim, not only a synthetic TU. The synthetic probe stayed green
  through the whole collision, which is the argument for the second probe.

* **W5 [cpp] — un-gate `NodeOptions` and `Rate`/`WallRate`. LANDED
  2026-09-12.** 22 of `NodeOptions`' 23 members are `static_assert`-only and
  need no `std`; `Rate`'s `Rate(double)`, `sleep()` and `reset()` are pure
  integer arithmetic. Replace the two genuinely-`std` members with
  `Span<StringView>` and `nros::Duration`.
  *Landed as:* `Rate::period()` returns `nros::Duration`, and `Rate` gains a
  `Rate(nros::Duration)` constructor as the always-available spelling.
  `NodeOptions::arguments()` takes a DEDUCED parameter rather than
  `const std::vector<std::string>&` — which is strictly better for a porting
  user as well as freestanding-clean, because the same call site binds and a
  caller passing anything else still gets the migration message instead of "no
  matching function"; the refusal was what the parameter type was decorating.
  The six `hosted-only` / `std-only` rows are gone from
  `.config/cpp-capability-layout-baseline.txt`, measured: NodeOptions 1, Rate
  16, WallRate 16, identical on all three arms.
  *Two departures from the RFC's suggestion, both measured.* The getter returns
  `const char* const*` (argv's own shape, null here) rather than
  `Span<StringView>`: `graph.hpp:105` had already ruled that pulling `span.hpp`
  into these headers would newly expose `Span` / `StringView` / `LeSpan` on the
  public C++ surface as a side effect, and doing it costs **16 unledgered
  API-parity items**. That classification is someone's decision, not a
  consequence of un-gating this class. And `Rate`'s `std::chrono` CONSTRUCTOR
  stays behind `NROS_CPP_HAS_STD_CHRONO` — a gate on a METHOD, which the
  layout rule permits since `sizeof(Rate)` is `int64_t` + `uint64_t`
  everywhere, but W8's acceptance is zero `NROS_CPP_HAS_*`, so it is W8's to
  resolve. Recorded rather than left, because a gated method is exactly the
  residue that reads as finished work.

* **W6 [cpp] — delete the 50 removable entities.** Each has an ungated
  freestanding sibling already, or is body-only. No consumer loses a capability.

* **W7 [cpp] — replace the 27 with in-tree freestanding equivalents**
  (`Span`, `StringView`, `FixedString`, `Seq`, `Duration`). Includes the one gap
  the census found: `ComponentNode` has no `Seq<T,N>` parameter overload, so its
  `std::vector` form has no sibling to fall back to and one must be added.

* **W8 [cpp] — the remaining `rclcpp::` surface moves onto the mechanisms**, and
  the nine gates and `cmake/compat/` are deleted. This is where the API becomes
  one API.
  *Acceptance:* zero `NROS_CPP_HAS_*`, zero `NROS_CPP_STD`, zero
  `NROS_CPP_NODE_HOSTED` in the tree; the per-header parse loop at `fail=0` on
  every toolchain; the FreeRTOS, Zephyr, NuttX and ThreadX C++ builds green.

* **W9 [examples] — examples and fixtures follow.** The explicit
  `std::shared_ptr<rclcpp::X>` members in our own porting templates become the
  nested alias. DELIBERATELY LAST: this corpus is what made RFC-0096's first
  draft reach for a hosted layer, and it is ours to change rather than a
  constraint to design around.
  *Constraint that does not move:* `local-msg-package` compiles under genuine
  ROS 2 Humble via `just colcon-parity`, so the spelling at the use site cannot
  diverge from upstream's.

* **W10 [ci] — the gates become structural.**
  `check-cpp-freestanding-includes` loses its baseline;
  `check-cpp-capability-layout` asserts a constant rather than ratcheting a
  list. A `std` type reaching any public signature becomes a build failure
  rather than a review question.
  *Acceptance:* both gates fail on a mutation that reintroduces a `std` type in
  a public signature, and the mutation is in the selftest.

## What this phase does not do

* It does not remove the ROS 2 API. Compiling upstream source is RFC-0089's
  goal; it stops being conditional on the toolchain.
* It does not build a refcounted smart pointer. Shared ownership is fiction
  tree-wide (RFC-0096, census 2), and building a mechanism for a requirement
  that does not exist is inventing work.
* It does not promise an arbitrary third-party ROS 2 package becomes
  freestanding. The claim is scoped to code written against the rclcpp API.
* It does not touch the C API or the Rust core.

## Relationship to the phases it absorbs

**phase-438** — W0, W1 and W2 landed (the `#elif`/`#else` gate fix, the
consolidation into `std_detect.hpp`, and the docs). Its W2 as originally
written — delete the discovery arm so the surface is REQUESTED — was deferred
after issue 1240 fixed the same breakage with the `__STDC_HOSTED__ &&
__has_include` conjunction. RFC-0096 supersedes it: the request had to select a
LAYER, and now it selects nothing because there is only one API. What remains of
phase-438 is absorbed here.

**phase-427** — W1 is reversed. Its "one node type" holds where it was aimed:
`ComponentNode` goes and the core has exactly one node type. What does not
survive is `void* hosted_`, which was the artefact of trying to give one type
two shapes. W2–W7 should be re-read against RFC-0096 before being implemented,
since several assume the hosted/freestanding split this phase removes.
