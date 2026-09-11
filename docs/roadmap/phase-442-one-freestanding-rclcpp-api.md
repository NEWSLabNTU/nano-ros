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
  has something to point at. **Met 2026-09-11 — the answers are in
  "W0's measurements" below.**

* **W1 [cpp] — `Timer::closure_` and `GuardCondition::closure_` unconditional.
  LANDED 2026-09-11.** Ends the shipping px4 mixed-layout exposure. The member
  is 8 bytes either way, so this costs nothing on the types that matter.
  Independent of everything else.
  *Acceptance:* `.config/cpp-capability-layout-baseline.txt` has no `diverges`
  row; `sizeof` of all five affected subjects identical in all nine
  configurations the layout gate measures. **Met** — Timer 32, GuardCondition
  40, `NodeWithTimers<4>` 360 on every arm, baseline's five `diverges` rows
  deleted, issue 1225 archived. The member is `void*` at a block headed by
  `detail::HostedBlockBase`, lifted out of `node.hpp` into
  `nros/hosted_block.hpp` so the three owners share one spelling.

* **W2 [cpp] — delete `std_compat.hpp`. LANDED 2026-09-11.** 275 lines, 71 `std::` occurrences,
  entirely behind `#ifdef NROS_CPP_STD`, included from exactly one place itself
  behind the same guard, and nothing that ships defines that macro. Dead in
  every shipping configuration; a third orphaned vocabulary predating the
  phase-427 merge.
  *Acceptance:* the tree builds; `just check cpp` green. **Met**, and the
  delete surfaced two things worth recording. `nros::create_publisher`'s free
  factory has a return-type divergence from `rclcpp::create_publisher` —
  `ResultOf<Publisher<M>>` against `std::shared_ptr<Publisher<MessageT>>`,
  both halves forced by RFC-0018 — which the deleted overload's differing
  ARITY had been masking from `--require-disposition`; it carries a ledger row
  now. And `check-api-parity` stayed green over **ten ledger rows describing
  deleted entities**, because the ledger ratchets in one direction only: a
  difference with no row fails, a row with no difference does not. Filed as
  issue 1323; the ten were removed and four surviving rows amended by hand.

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
  **W0 measured the trap the workaround falls into:** declaring the placement
  form by hand is not a one-liner, because its first parameter must be the
  implementation's `size_t`. `inline void* operator new(unsigned long, void*)`
  is accepted on x86_64 and riscv64 and REJECTED on cortex-m3 —
  `'operator new' takes type 'size_t' ('unsigned int') as first parameter` —
  so a hand-rolled declaration is right on two of our three toolchains and
  silently wrong on the embedded one. `__SIZE_TYPE__` is the portable spelling;
  shipping the guaranteed `<new>` is the real fix.

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

* **W6 [cpp] — delete the 50 removable entities. LANDED 2026-09-12.** Each has
  an ungated freestanding sibling already, or is body-only. No consumer loses a
  capability.
  *What it removed, counted as SITES rather than as the RFC's entity buckets,
  because sites are what a grep can re-measure.* 27 `std::` uses across eight
  headers, plus the `component.hpp` placement-new workaround that W4's class fix
  made redundant:
  - 13 `std::move` -> `nros::tr::forward_rvalue`;
  - 3 `std::forward` -> `nros::tr::relay`, which is a SEPARATE helper and not a
    synonym: forwarding preserves an lvalue as an lvalue where a move hands the
    callee an rvalue it may gut, and collapsing the two is the classic way a
    forwarding wrapper steals from its caller;
  - 14 `std::size_t` / `std::uint8_t` / `std::int32_t` / `std::uint32_t` ->
    the bare spellings the rest of this API already writes. `transport.hpp` is
    now entirely `std`-free;
  - `result.hpp`'s `<utility>` include, verified unused by removing it and
    recompiling rather than by reading.
  *What it deliberately did NOT remove:* the hosted diagnostics (`fprintf`,
  `fopen`, `getenv`, `ostringstream`, `abort`, 13 sites), which are genuinely
  body-only and already carry a freestanding `#else` -- `result.hpp`'s
  `NROS_TRY_LOG` is the pattern. Those are not the phase's target; a `std` call
  inside a hosted-only body changes no signature and no layout.
  *Acceptance:* all three arms compile (hosted, hosted with `-DNROS_CPP_STD=1`,
  `-nostdinc++` freestanding against the ThreadX shim), `just check fast` green.
  Remaining after this pass, and each is W7's or W8's by construction:
  `shared_ptr`/`unique_ptr`/`make_shared` 38, `string`/`vector`/
  `initializer_list` ~55, `chrono` 9, `function` 2.

* **W7 [cpp] — replace the 27 with in-tree freestanding equivalents**
  (`Span`, `StringView`, `FixedString`, `Seq`, `Duration`). Includes the one gap
  the census found: `ComponentNode` has no `Seq<T,N>` parameter overload, so its
  `std::vector` form has no sibling to fall back to and one must be added.
  **MET BY MEASUREMENT 2026-09-12, and the measurement is the deliverable**, so
  this is recorded rather than claimed as work done.

  After W6, the remaining 134 `std::` sites in `nros-cpp/include` split like
  this — a comment-stripping scan that tracks `#if` nesting, so a site inside a
  capability gate is counted as gated:

  | | sites |
  | --- | --- |
  | behind `NROS_CPP_STD` / `NROS_CPP_HAS_*` / `NROS_CPP_NODE_HOSTED` | **129** |
  | ungated | **5** |

  And all five ungated are legitimate:

  * `log.hpp:237-238` — string LITERALS inside a diagnostic message, not uses.
  * `nros.hpp:319,363` — `std::abort()`, which `[compliance]` puts in the
    freestanding subset and both shims export.
  * `parameter.hpp:85` — `std::initializer_list`, whose header is
    freestanding-guaranteed because it is core language support for braced
    initialisation, not a library convenience.

  So **the freestanding API is already `std`-free**, and there is nothing on
  that path left to replace. The census's 27 "replaceable" already have their
  siblings, checked pair by pair in `parameter.hpp`: `declare_parameter` has
  `Seq<T,N>` at :274 beside `std::vector<T>` at :284, `get_parameter` :322
  beside :350, `set_parameter` :372 beside :378, and the string form's
  freestanding sibling is the `char* out, size_t max_len` overload at :308. The
  gap the census predicted — a missing `Seq` form — does not exist; and
  `ComponentNode`, which the census named as the holder of it, was deleted by
  phase-427 W7.

  *What this hands W8.* The remaining work is not "replace 27 types" but
  "collapse 129 gated sites into one ungated API", using RFC-0096 D8's deduced
  parameter and W3's two mechanisms. That is a different and larger job than
  W7's text describes, and naming it here is the point of recording the
  measurement.

* **W8 [cpp] — the remaining `rclcpp::` surface moves onto the mechanisms**, and
  the nine gates and `cmake/compat/` are deleted. This is where the API becomes
  one API.
  **BLOCKED on one decision, and the block is deliberate (2026-09-12).** W7's
  measurement says the job is to collapse **129 gated sites** into one ungated
  API. Two of the three things that needs are done — RFC-0096 D8 settles how a
  name parameter is typed, W3 built the handle and the inplace callable. The
  third is not: **where does the entity a returned handle points AT live?**
  Today `create_publisher<M>(topic, qos)` does `std::make_shared` and pushes
  into `owned_entities`, and the census established that models ADDRESS
  STABILITY rather than shared ownership — but address stability is a real
  requirement, because the executor arena holds a raw dispatch pointer and has
  no unregister path. RFC-0096 D9 states the three candidates and the measured
  sizes that make the choice consequential (`Client<int>` is 4 672 bytes against
  `Timer`'s 32, so a uniform worst-case pool is wrong by an order of magnitude).
  The direction that fits this repository is entity counts DERIVED the way
  phase-412 already derives `NROS_CPP_EXECUTOR_STORAGE_SIZE`, and it needs a
  `just mem-report` before/after rather than a decision at the keyboard.
  *Nothing mechanical in W8 should start before that is answered*: the rest is
  substitution, and substitution on top of an unanswered lifetime question is
  how a use-after-free ships.
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

## W0's measurements

Three questions, measured rather than argued. Reproduce with the probes under
`tmp/w0/` (gitignored scratch — the commands are here so the next person can
re-run them, not the files).

### 1. Capture sizes — a distribution, and it is narrow

Census first: `git ls-files "*.cpp" "*.hpp"` minus `third-party/`, 308 files,
every capturing lambda.

| capture | sites | what it is |
| --- | --- | --- |
| `[this]` | 7 | the dominant rclcpp idiom |
| `[&]` | 10 | **none of them an rclcpp callback** — `std::thread` bodies and local string helpers in the Cyclone backend |
| `[state]` | 1 | a raw `TopicState*`, `topic-state-monitor-port` |
| `[this, state]` | 1 | same file |
| `[this, &state]` | 1 | same file |
| `[obj, method]` | 1 | `diagnostic_updater`'s `add(name, obj, method)` |

Then `sizeof(decltype(lambda))` for each shape, on all three toolchains the
project ships:

| capture | hosted x86_64 | arm-none-eabi 13.2 cortex-m3 | riscv64 ThreadX |
| --- | --- | --- | --- |
| `[]` | 1 | 1 | 1 |
| `[this]` | 8 | 4 | 8 |
| `[state]` | 8 | 4 | 8 |
| `[this, state]` | 16 | 8 | 16 |
| **`[obj, method]`** | **24** | **12** | **24** |
| `sizeof(void*)` | 8 | 4 | 8 |
| pointer-to-member-function | 16 | 8 | 16 |

The widest shape in the corpus is `[obj, method]`, and it is wide for a reason
worth stating: a pointer-to-member-function is TWO words on the Itanium ABI (a
function address plus a this-adjustment), so that one capture costs three
pointers where every other costs one or two. **Every measured capture is a
whole number of pointers**, which is why the knob is spelled in pointers below
and not in bytes — a byte count would be right on one word size and wrong on
the other.

### 2. `X::SharedPtr` — a copyable non-owning handle suffices, and is already what ships

The corpus's use sites are members (`rclcpp::Publisher<M>::SharedPtr pub_;` and
the explicit `std::shared_ptr<rclcpp::Publisher<M>>` spelling, which W9 folds
into the alias) plus exactly one by-value pass, `diagnostic_updater::Updater`'s
`Updater(std::shared_ptr<rclcpp::Node> node, double)`. No `weak_ptr`, no
`use_count()`, no `reset()`, nothing that observes a reference count, anywhere
outside the API's own headers.

The decisive measurement is that shared ownership is already fiction at the one
site where it looked real. `node.hpp`'s `shared_from_this()` returns

```cpp
::std::shared_ptr<Node>(::std::shared_ptr<void>(), this);
```

— the aliasing constructor with an EMPTY owner. It observes the node and does
not extend its lifetime. So what the corpus is already compiling against is a
copyable non-owning handle wearing `shared_ptr`'s spelling, and the handle W3
builds has to reproduce copy, assign, `->`, `*`, default-construct and a null
test. Nothing needs a control block.

### 3. The capacity knob, and what it costs

**`NROS_CPP_CALLBACK_CAPACITY`, default `4 * sizeof(void*)`** — 16 bytes on a
32-bit target, 32 on a 64-bit one. That is the widest measured capture
(`[obj, method]`, three pointers) plus the one pointer of headroom this work
item asked for, and it is expressed in pointers so one default is correct on
both word sizes.

`.bss` measured on eight registered callbacks — the scale of a realistic node,
a timer plus a few subscriptions plus a service — at `-Os`, freestanding, no
allocator:

| capacity | arm-none-eabi cortex-m3 `.bss` | per callback | riscv64 `.bss` | per callback |
| --- | --- | --- | --- | --- |
| 8 | 132 | 16 | — | — |
| 12 | 132 | 16 | — | — |
| **16** | **196** | **24** | 196 | 24 |
| 24 | 260 | 32 | **260** | **32** |
| **32** | 324 | 40 | **324** | **40** |
| 48 | 452 | 56 | 452 | 56 |
| 64 | — | — | 580 | 72 |

Per callback the storage is `align8(Cap) + 8`: the capacity, rounded up for
alignment, plus the invoker pointer. Two things the table settles that
arithmetic alone would not:

* **`.text` is flat.** 256 -> 276 bytes across capacities 8 to 48 on
  cortex-m3, 324 -> 338 across 16 to 64 on riscv64. Capacity is a `.bss` knob;
  choosing generously does not grow code.
* **A capacity below two pointers buys nothing on 32-bit.** Cap 8 and cap 12
  both measure 16 bytes per callback, because the storage is
  `alignas(long long)` — which it must be, since a capture may contain a
  `double` or a `long long` even though none in the corpus does.

At the chosen default the eight callbacks cost **192 bytes on 32-bit, 320 on
64-bit**. For scale, the C++ config a shipping Zephyr image generates declares
`NROS_CPP_EXECUTOR_STORAGE_SIZE 89352`, so this is ~0.2-0.36% of the executor's
own static footprint.

The honest comparison is against what the freestanding path stores TODAY, which
is the `(void (*)(void*), void* ctx)` pair — two pointers, 8 bytes on 32-bit and
16 on 64-bit. So the mechanism's real delta per entity with a callback is **+16
bytes on 32-bit and +24 on 64-bit**, and what it buys is that the capturing
lambda a ported file writes compiles there at all.

### The over-budget failure is a compile error naming the knob

Measured, not intended:

```
tmp/w0/overbudget.cpp:24:33: error: static assertion failed:
callback capture too large -- raise NROS_CPP_CALLBACK_CAPACITY
```

### How to re-run

```sh
# capture sizes, all three toolchains (report comes out as compile errors)
g++ -std=c++17 -fsyntax-only tmp/w0/capture_sizes.cpp
~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1/bin/arm-none-eabi-g++ \
    -std=c++14 -ffreestanding -fno-exceptions -fno-rtti -mcpu=cortex-m3 -mthumb \
    -fsyntax-only tmp/w0/capture_sizes.cpp
riscv64-unknown-elf-g++ -std=c++14 -ffreestanding -fno-exceptions -fno-rtti \
    -fsyntax-only tmp/w0/capture_sizes.cpp

# .bss per capacity
for cap in 8 12 16 24 32 48; do ... -DNROS_W0_CAP=$cap -c tmp/w0/bss_cost.cpp ...; done
```

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
