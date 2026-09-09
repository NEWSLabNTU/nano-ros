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
  *Acceptance:* an rclcpp-shaped node body — derived `rclcpp::Node`, a
  capturing-lambda subscription callback, and `Publisher<M>::SharedPtr` /
  `Subscription<M>::SharedPtr` / `TimerBase::SharedPtr` members — compiles in
  all three configurations (`arm-none-eabi -ffreestanding`, the ThreadX
  `-nostdinc++` shim, hosted), and an over-budget capture fails with a
  `static_assert` naming the knob. A prototype already does; this is that
  prototype made real.

* **W4 [boards] — fix the two shim gaps.** The ThreadX `cxx-compat` shim's
  `<new>` omits the PLACEMENT forms, which are freestanding-guaranteed, so
  placement new does not compile against it at all; its `<type_traits>` is a
  58-line stub with `enable_if` but no `is_same` and no `decay`, and its
  `remove_reference` lives in `<utility>` instead. Independent of W3 because D3
  makes the API not depend on them — but both are traps for the next in-place
  construction, and one of them is a guaranteed facility.
  *Acceptance:* placement new compiles against the shim; a probe using the
  freestanding-guaranteed `<type_traits>` contents compiles.

* **W5 [cpp] — un-gate `NodeOptions` and `Rate`/`WallRate`.** 22 of
  `NodeOptions`' 23 members are `static_assert`-only and need no `std`; `Rate`'s
  `Rate(double)`, `sleep()` and `reset()` are pure integer arithmetic. Replace
  the two genuinely-`std` members with `Span<StringView>` and `nros::Duration`.

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
