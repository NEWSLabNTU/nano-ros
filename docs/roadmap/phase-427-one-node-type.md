# Phase 427 — one node type, named `rclcpp::Node`, compiling freestanding

**Status (2026-09-05). Planned.** Implements RFC-0089 §"The node API, proposed
under the governing principle". Preconditions are met by phase-417: the
`pump()` blocker is gone, `check-cpp-capability-layout` measures the layout
rule, and the `-nostdinc++` lane can see a freestanding regression.

## What this is

Three C++ node shapes collapse to one type:

| today | after |
| --- | --- |
| `nros::Node` — out-ref creation, freestanding, 103 files | `rclcpp::Node` |
| `rclcpp::Node` — hosted, `shared_ptr`, derivable | the same type, hosted members |
| `nros::ComponentNode` — derivable, entry-supplied handle, 5 dirs | DELETED; its handle becomes a constructor |

`nros::Node` survives only as a deprecated alias so the remaining call sites are
optional to migrate rather than a flag day.

## Work items

* **W1 [cpp] — the out-of-line hosted block.** `void* hosted_` replaces every
  hosted-only member; `detail::NodeHosted` is allocated lazily on the first
  hosted-shape call and never on a freestanding target.
  *Acceptance:* `check-cpp-capability-layout` passes with the hosted members
  present, and `sizeof(rclcpp::Node)` is identical with and without
  `-DNROS_CPP_STD`.

* **W2 [cpp] — construction.** `Node(const char*)` + `init()` + `ok()`
  freestanding; upstream's `std::string`/`NodeOptions` constructors hosted.
  *Acceptance:* the `-nostdinc++` lane compiles a TU that constructs a node and
  creates a publisher; a hosted TU compiles upstream's constructor verbatim.

* **W3 [cpp] — one name, two signatures.** The out-ref `create_*` family and
  the `shared_ptr` family coexist as overloads on the one type;
  `create_wall_timer` gains the member-binding template overload, retiring
  `bind_timer`.
  *Acceptance:* a component binds a member function with no allocation; a
  ported file's `create_publisher<M>("chatter", 10)` compiles hosted and FAILS
  TO COMPILE freestanding, with a diagnostic naming the out-ref overload.

* **W4 [cpp] — `ComponentNode` deleted.** Its 5 directories move to the one
  type. RFC-0044 is amended, not deleted — its Q2 boot-failure reasoning becomes
  `ok()`'s.
  *Acceptance (amended 2026-09-09):* zero `ComponentNode` in the tree; the
  RFC-0047 subnode packages build and run — **as callback-group packages, which
  is what they are**; **one of them builds for a freestanding target**, which is
  the test of whether the merged type still fits.

  The first version of this item also required that "RFC-0047's
  several-named-nodes survives as a documented ours-only capability on it".
  **That clause is struck: the capability does not exist and the citation was
  wrong.** RFC-0047 is callback groups; `ComponentNode` owns exactly one
  `Node node_` (`component_node.hpp:734`); both subnode packages open with "ONE
  ComponentNode with TWO callback groups". Several named nodes per IMAGE is real
  and belongs to the executor under **RFC-0046** (`Executor::create_node` /
  `node_builder`), one node per component. Preserving a capability that is not
  there would have added a constructor and a ledger row for nothing — RFC-0089
  "The several-named-nodes capability, corrected" carries the measurement.

* **W5 [cpp] — `get_logger()` follows ROS 2.** The `"nros.compat"` sentinel is
  replaced by a logger named for the node (RFC-0089 decision 1).
  *Acceptance:* two nodes in one image emit records under distinct logger names.

* **W6 [loudness] — the two items the design creates.** `[[nodiscard]]` on
  `Result` (`NROS_NODISCARD` for C++14) so a discarded `rclcpp::init(argc,
  argv);` warns; and a story for a hand-written `main` that never checks
  `Node::ok()`.
  *Acceptance:* a TU that discards `init()`'s result fails a `-D warnings`
  lane. The `ok()` half may end as documentation — if so, say so in the book
  rather than leaving it implied.

* **W7 [migration] — `nros::Node` deprecated, then deleted.** Alias for one
  release with `NROS_DEPRECATED_MSG`, then removed.

## What stays invented — REVISED after review (2026-09-05)

The first version of this table had four entries. Reviewed against the recorded
upstream surface, **two were mislabelled** (RFC-0089 §"Review of the invented
parts"):

| item | verdict |
| --- | --- |
| `rclcpp::Timer` | **invention, kept flat — the `TimerBase` alias is WITHDRAWN.** `TimerBase` is a name that promises children, and aliasing a concrete class to it sells a taxonomy we do not have. Studied: upstream's hierarchy exists so its executor can hold a type-erased handle while the timer stores its functor inline. Neither reason survives — our `Timer` is a HANDLE and the callback lives in the arena as a raw fn ptr, so the executor never holds a C++ timer and a base would carry a vtable NO dispatch uses. And `GenericTimer`'s clock parameter corresponds to ROS-time/simulated-time timers, which we do not have: `create_timer` takes no clock and the executor schedules on one monotonic `nros_platform_clock_ns`. Inventing our own one-leaf hierarchy would reproduce the same empty promise. `create_wall_timer` already carries the accurate verb. |
| `rclcpp::spin_once(timeout_ms)` | **invention, kept.** Upstream's verb is ALREADY ported — `spin_some(node)` exists here with upstream's drain-what-is-ready semantics as a 0-timeout spin. Ours is a blocking wait with a budget, which rclcpp has no verb for and an RTOS task needs. |
| `Node::init` / `Node::ok` | kept. `ok()` matches `rclcpp::ok()`'s own spelling; `init()` is the channel a `-fno-exceptions` target needs instead of a throwing constructor. |
| out-ref `create_*` | **not an invention.** It shares upstream's name with a changed signature, and the signature is forced: the arena stores `&entity` as its dispatch context and has NO unregister, so a value-returning form would move the object and dangle the context on the first callback. |

The `rclpy.spin_once` collision recorded earlier was **overstated**. rclpy's
signature is `(node, timeout_sec)`, so a user carrying that habit writes
`spin_once(node, 0.1)` in C++ and gets no matching overload — mechanical, not
silent. The ledger row records the sibling for the reader; it is not a reason to
rename.

## Superseded first-pass table

### (first pass, kept for the diff)

Tabulated in RFC-0089; repeated here because it is the part a reviewer should
argue with. Each needs a ledger row with `disposition: extension`, and each is
watched by the collision gate.

| name | why upstream has nothing | collision risk |
| --- | --- | --- |
| `rclcpp::Timer` | ours is a HANDLE; upstream's `TimerBase`/`WallTimer` are a virtual hierarchy, and we have no vtable budget | **real** — `Timer` is absent upstream today while both siblings are present |
| `rclcpp::spin_once(timeout_ms)` | upstream's one-cycle verb is `spin_some(node)` — no timeout, no return | **real** — `rclpy.spin_once` exists with `(node, timeout_sec)` |
| `Node::init` / `Node::ok` | upstream throws; a `-fno-exceptions` target needs a channel | low |
| the out-ref `create_*` family | caller-owned storage has no upstream counterpart | none — it shares upstream's NAME, so it is a ported signature, not an invention |

* **W9 [rust] — `Context` on every target, and the rclrs `init` family.**
  RFC-0089 "Context and `init`, settled". `init.rs` comes out from behind
  `env`: the feature gates only the process-environment reader, and a
  freestanding constructor reads the baked constants (`NROS_LOCATOR`,
  `NROS_DOMAIN_ID`) the entry macros already compute — the macros then call it
  instead of assembling `ExecutorConfig` by hand, so the baked shape has one
  source. `InitOptions { domain_id }` with `new()`/`with_domain_id()`;
  `Context::default_from_env()` (= `init()`), `Context::from_env(InitOptions)`,
  and hosted-only `Context::new(args, InitOptions)` refusing `--ros-args` with
  `REFUSE_INIT_ARGS`. Ledger: `rust:Context::default_from_env` `rename` ->
  `adopt`, `from_env`/`new` -> `adopt-bounded`, `rust:InitOptions` ->
  `adopt-bounded`; `rust:init` stays as the C++-symmetric anchor.
  *Acceptance:* a freestanding entry (one Zephyr west leaf) builds with
  `Context::default_from_env()` and no `ExecutorConfig` literal; hosted tests
  cover the domain override and the `--ros-args` refusal; `just check
  api-parity` green with the rows flipped.

* **W10 [rust] — the executor opens a session, the node is named after.**
  `Context::create_executor()` (`alloc`) and `create_executor_in(backing)`
  (ours-only, no-alloc); `Executor::create_node(name)`; `ExecutorConfig::
  node_name` deprecated for one release. This is the Rust half of "one node
  type": today the executor IS the node it was configured with, which is why a
  ported `executor.create_node("talker")` has nowhere to go. **RFC-0046's**
  several-named-nodes-per-image (the `node_builder(name)` funnel) is the existing
  capability this lands on — not RFC-0047, which is callback groups; see RFC-0089
  "The several-named-nodes capability, corrected".
  *Acceptance (amended 2026-09-09 — MEASURED, and it is not two):* the rclrs
  talker tutorial ports with **six** edits, in the breakdown below;
  `rust:Context::create_executor` and `rust:Executor::create_node` carry
  `adopt-bounded`/`adopt`.

  | kind | n | what |
  | --- | ---: | --- |
  | import | 2 | the crate each name comes from. Any port has these. |
  | error type | 1 | the `main` signature. Possible AT ALL only because this wave gave `NodeError` a `Display` impl. |
  | mutability | 1 | `let mut node`, which rustc names. |
  | **predicted** | **2** | the two `?`s RFC-0089 named. These are W10's own, and they are the only two of the six that are. |

  Held as two files diffed at BUILD time
  (`packages/testing/nros-tests/tests/rclrs_talker_port.rs`), so the count is a
  fact about the same bytes that compile, not a claim in prose.

  A **seventh** difference is on the same line as one of the `?`s and therefore
  costs no extra edit, but it is a real divergence and is recorded rather than
  absorbed: the port writes `spin_blocking` because `Executor::spin` is taken
  here by `spin(Duration) -> !`, the body of an RTOS task. Moving upstream's
  `spin(SpinOptions)` onto that name is a later wave — **open item, W10 does not
  own it.**

## Not in scope

* Parameters — phase-426. W4 touches the facades but does not depend on it.
* The C API's node shape — phase-428's sweep decides whether it follows.
* Migrating the 409 remaining `nros::` call sites; the alias makes that
  optional and incremental.


## W8 [error channel] — one rule, and `[[nodiscard]]`

The review surfaced that we ship two error channels with no stated rule.
`Result` (no value) and `Expected<T>` (value or error) both live in
`result.hpp`, which — measured — carries ZERO capability gates and parses under
the ThreadX `-nostdinc++` shim, so both reach every target.

The rule, now in RFC-0089: ours-only fallible operations return `Result`, or
`Expected<T>` when they produce a value; a state query that cannot fail returns
`bool`; **a PORTED api keeps upstream's channel even when that is `bool`** —
`rclcpp::Node::get_parameter` returns `bool` upstream, so ours does. Making that
uniform would be a preference recorded as a divergence, which RFC-0036 forbids.

**Settled shape.** `Expected<T>` is RENAMED to `Result<T>`, and today's
value-less `Result` becomes `Result<void>` with `Result` as the alias — one
template, one name, instead of two unrelated types a reader must learn.
`Expected<T>` survives as a deprecated alias for one release. Where upstream
would THROW, the counterpart returns `Result<T>`; where upstream's channel is
already `bool` (`Node::get_parameter`), ours stays `bool`.

*Acceptance:* `Result` and `Result<T>` carry `[[nodiscard]]` (`NROS_NODISCARD`
on C++14); a TU that discards `rclcpp::init(argc, argv)` fails a `-D warnings`
lane. `result.hpp` still carries ZERO capability gates and still parses under
the ThreadX `-nostdinc++` shim — the rename must not introduce a gate, and the
header lane is what proves it. This subsumes W6's first item.

### W8 LANDED 2026-09-07 — with two corrections this section had wrong

Both are measurements, and both change what the acceptance could say.

**1. The template cannot be called `Result`.** "One template, one name" is not
expressible in C++: an identifier in a scope names a class, a class template, or
an alias, never two. Measured on gcc 12.3 and clang 14 —
`template <typename T = void> class Result; Result f();` is *"invalid use of
template-name 'Result' without an argument list"* (c++14) and *"deduced class
type 'Result' in function return type"* (c++17); a class and a template sharing
the name is *"class template 'Result' redeclared as non-template"*; an alias and
a template sharing it is *"redeclared as different kind of entity"*.

So the STRUCTURE landed in full — one template, the value-less case IS its
`void` specialization, `Result` is an alias for that specialization, two
unrelated types are gone — and only the template's spelling differs: it is
**`ResultOf<T>`**. `Result` kept the bare name because 1100+ call sites and the
whole public API write it for the void case; requiring `Result<>` there would
have been a flag day for a spelling that reads worse. RFC-0089's section is
corrected in place.

`Expected<T>` survives for one release as a `[[deprecated]]` **class** template
that converts from `ResultOf<T>`, not as a deprecated alias template: measured,
`[[deprecated]]` on an alias template warns on gcc and is SILENT on clang, both
standards, and a deprecation half our users never see is just an alias.

**2. `rclcpp::init(argc, argv)` cannot be the probe.** It returns `void` here —
deliberately, by this section's own ported-channel rule — so a TU discarding it
compiles clean and would have made the gate vacuous. The case the attribute
actually catches is the WIDENING: `nros::init()` returns a `Result` where
`rclcpp::init()` returns nothing. `result_nodiscard_probe.cpp` discards both
halves of the channel and the lane requires two `unused-result` diagnostics.

What it caught on its first run, which is the argument for it: `rclcpp::shutdown()`
was discarding `nros::shutdown()`'s `Result` and answering `true`
unconditionally — a fini that failed reported success.

`NROS_NODISCARD` lives in `result.hpp` beside the types it marks (nothing else
is marked, and every other header reaches that one). It gates no member;
`::nros::Result` and `::nros::ResultOf<int>` joined
`check-cpp-capability-layout`'s type list, so `sizeof` invariance is measured
rather than asserted, hosted and freestanding. The header still carries its
original two `NROS_CPP_STD` sites and no third, and still parses under the
ThreadX `-nostdinc++` shim.

## Measured before starting (2026-09-07) — two findings that change the order

Both are measurements, not readings. Each contradicts something this document
asserted, so the original wording is kept above and corrected here.

### Finding 1 — W1's acceptance cannot currently fail (issue 1204)

W1 accepts on *"`check-cpp-capability-layout` passes with the hosted members
present, and `sizeof(rclcpp::Node)` is identical with and without
`-DNROS_CPP_STD`"*. Both halves are satisfiable by a broken implementation.

The gate forces each capability macro **on** against a hosted baseline where the
headers already self-define every one of them (`client.hpp:23-32` under
`__has_include(<memory>)`, and the same block in publisher / service /
subscription / polling_subscription). Forcing them on is a no-op, so the gate
compares the baseline against itself seven times:

```
rclcpp::Node baseline = 3752   and all seven forced measurements = 3752
```

Proven by mutation — but not by the first mutation tried, and the correction
matters. Injecting a member gated on `NROS_CPP_HAS_SHARED_PTR` into
`rclcpp::Node` is TAUTOLOGICAL: that type is itself inside
`#if defined(NROS_CPP_HAS_SHARED_PTR) && ...` (`nros.hpp:447`), so the inner
`#ifdef` holds wherever the type exists. Every configuration reports 3760 —
uniform, no divergence, and the gate was right to pass it.

The honest demonstration is `::nros::Node`, which sits under no capability
guard. With a gated `double` added there, the old gate reports
`OK — no layout depends on a probe`, exit 0, while its synthetic selftest passes
in the same run. The rebuilt gate reports
`FAIL: sizeof(::nros::Node) differs hosted vs -nostdinc++ freestanding — 200 vs 192`,
exit 1. Both mutations reverted; tree byte-identical. Filed as issue 1204.

**Consequence: a W0 precedes W1.** The macros are genuinely off only under
`-nostdinc++` against a shim with no `<memory>`, and the `cpp` lane already has
two such configurations (`just/check/lanes.just:708-747`). The gate must measure
there. The blanket `continue` on an unmeasurable type must also become a
per-type policy: "the type does not exist in this configuration" is not the same
as "not a layout question", and for a type that is supposed to exist everywhere
it is the failure.

### Finding 2 — the merge is blocked on phase-426, and "Not in scope" was wrong

This document says: *"Parameters — phase-426. W4 touches the facades but does not
depend on it."* Measured, W1 cannot be done without that decision, because the
parameter store is not hosted-only and so W1's `void* hosted_` does not move it.

```
::nros::Node                        =    192 bytes
rclcpp::Node                        =  3 752      (192 + ParameterServer<16>)
::nros::ComponentNode               = 55 776      (192 + ParameterServer<256,8,4096>)
::nros::ParameterServer<16>         =  3 512
::nros::ParameterServer<256,8,4096> = 55 352
```

One type means one store size, and `nros::Node` is the type embedded in every
freestanding image — 211 files, 299 references. Each naive merge is a defect:

| take | consequence |
| --- | --- |
| `ComponentNode`'s store | every node grows to ~55 KB — the regression the phase-392/394 memory campaign exists to prevent |
| `rclcpp::Node`'s store | `ComponentNode`'s derivers silently drop from 256 declared parameters to 16 |
| neither (RFC-0089 decision 3) | correct, and it needs the Rust-side setter phase-426 owns |

RFC-0089 decision 3 already chose the third: *"The merged node's parameters are
the Rust store"*, both inline stores becoming forwarders. The FFI supports that
for `declare_parameter` and the four typed getters
(`nros_cpp_declare_param`, `nros_cpp_get_param_{integer,double,bool,string}`),
and supports nothing else — there is no `nros_cpp_set_param` and no
`set_parameter` on the Rust executor, confirmed again here. `has_parameter` has
no FFI either.

So the merged node can forward declare and get today; `set_parameter<T>` and
`has_parameter` have nothing to forward to. That is a `refuse-loud` disposition
until phase-426 lands the setter, which is the disposition machinery working as
designed rather than a workaround. Note `ComponentNode` has no `set_parameter` at
all, so only `rclcpp::Node`'s two overloads (`nros.hpp:826`, `:845`) regress.

### Corrections to line references in this document

Measured against the tree, three citations here and in phase-417 have drifted:

* `adopt_launch_seed_` is `component_node.hpp:586-615`, not `:536-565`; its
  `#if defined(NROS_SYSTEM_PARAM_SERVICES)` opens at `:562`, not `:536`.
* The rclcpp-shaped parameter facade is `component_node.hpp:618-709`, not
  `:568-659`.
* phase-417 W2.b's stated blocker — *"`nros.hpp` does not include
  `component_node.hpp`"* — was fixed by W2.b itself; the include is at
  `nros.hpp:62`, unconditional. **SWEPT 2026-09-07.** Re-verified first by
  compiling a TU including only `<nros/nros.hpp>` and naming `ComponentNode`,
  `NodeHandle`, `nros::detail::report_component_failure` and the
  `NROS_SUBSCRIBE` / `NROS_COMPONENT` macros — it compiles, and the same TU
  against `node.hpp` alone fails, so it is the umbrella supplying them.
  Corrected in place (superseded text kept, per the docs convention) in
  `scripts/api-parity.py:199`, `docs/issues/README.md`, issue 0793,
  `api-parity-ledger/node.json` (`cpp:ComponentNode`),
  `api-parity-ledger/param.json`, phase-417 W2.b itself, and
  `archived/0795-*`. The count was NINE sites, not six: the param.json
  boilerplate carrying the false sentence sits in **six** rows
  (`declare_parameter`, `declare_parameters`, `get_parameter`,
  `get_parameter_or`, `get_parameters`, `has_parameter`), of which only the
  three that also carry the survival argument had been spotted, and the
  archived issue 0795 repeats it as a "same shape, different header" aside.

### One thing the design does not address: `shared_from_this`

`rclcpp::Node` derives from `std::enable_shared_from_this<Node>`
(`nros.hpp:540`), and two example templates call `shared_from_this()` to hand a
node to `diagnostic_updater`
(`examples/templates/rclcpp-compat-smoke/src/talker.cpp:48`,
`examples/templates/topic-state-monitor-port/src/topic_state_monitor.cpp:60`).

RFC-0089's layout sketch for the merged type has no base class, and the base
cannot simply be gated: a base that exists only where `<memory>` does changes
`sizeof` between two TUs of one image, which is precisely the mixed-layout
hazard (issues 0135, 0460) that the layout rule exists to prevent — px4 sets
`-DNROS_CPP_STD` on one module of a larger image on purpose. Dropping the base
also does not work by itself, because `std::make_shared<Derived>` populates the
weak reference through that base and nothing else does.

This needs a decision before W1 lands, and it is not recorded anywhere yet.

## Conciliated with phase-426 (2026-09-07) — the ordering, and W4's acceptance

Full reasoning in RFC-0089 §"Conciliating phase-426 and phase-427". The parts
that change this document:

**The governing reason is the owner's, and it is stronger than the one recorded
above.** In nano-ros a node is always linked into the final image; there is no
distinction like ROS 2 on Linux, and we keep ROS 2's shape. `ComponentNode`
versus `Node` encodes a LOADING-MODEL distinction that this system does not
have. That is why the count is one — not because upstream happens to lack a
counterpart.

**Deriving is not hosted-only, and this document inherited that error from the
RFC.** Measured, compiled to an object rather than parsed: a class deriving
from the node, holding a `Timer` member and binding a member function, builds
under `-std=c++14 -fno-exceptions -fno-rtti -ffreestanding -nostdinc++` against
the ThreadX shim, and `__is_polymorphic` is false for both base and derived.
Derivation needs no allocator, no exceptions and no vtable. What was hosted-only
was `rclcpp::Node` itself — the whole class sits inside `#if
defined(NROS_CPP_HAS_SHARED_PTR) && ...` — plus `ComponentNode`'s virtual
destructor. Neither is a property of deriving.

Consequence for W4: `nros_components_register_node`'s default `SHAPE rclcpp`
stays correct on every target, and cmake's comment calling `configure(Node&)`
"legacy" stops contradicting the RFC. The freestanding acceptance is met by the
DERIVED shape, which is the one users port.

**W1 is reordered behind phase-426.** The parameter store is not hosted-only, so
W1's `void* hosted_` does not move it, and one type means one store size: 192 B
/ 3 752 B / 55 776 B for the three types today. Every naive merge is a defect.
phase-426 W1–W4 land first — including its W4, which deletes both C++ stores —
and W1 then begins from a node with no parameter store. This document's
"Parameters … does not depend on it" is WITHDRAWN.

**W4's RFC-0047 acceptance criterion is deleted, not weakened.** "One component,
several named nodes" does not exist: `ComponentNode` holds one `Node` and takes
one name, RFC-0047 is about per-callback-group tiering inside one node, both
subnode packages are one node with two groups, and `entity_inventory.rs:604`
derives `NROS_EXECUTOR_MAX_NODES` from "one constructor is one node NAME". The
criterion was satisfiable only vacuously.

**W4 gains an acceptance it was missing.**
`packages/api/nros-cpp/tests/compile/declared_qos_depth.cpp` and its `_probe.cpp`
sibling are gates, not tests — the lane compiles the first clean, requires the
second to FAIL, and greps its diagnostic. "Zero `ComponentNode` in the tree" is
satisfied by a migration that leaves both compiling and proving nothing.
*Acceptance:* after migration the probe still fails, and still fails with a
diagnostic naming the declared depth and the topic; the lane's grep moves in the
same commit.

**W2/W3 gain three signatures.** The member-pointer fold needs four, not one:
the two subscription forms take NO out-ref, because the arena registers with
`ctx = self` and produces no C++ object. The blanket claim that "the arena
stores `&entity`" is true of subscriptions only — timers store the caller's
context and return an id, which is also why `ComponentNode`'s `timers_` pool
disappears rather than moving.

**W5 has a type question, not just a behaviour question.**
`ComponentNode::get_logger()` returns `const void*` (what the `NROS_LOG_*`
macros consume); `rclcpp::Node::get_logger()` returns a `rclcpp::Logger`. One
accessor cannot return both, and decision 1 settles only the name it carries.

**Three things need explicit homes** or they vanish with the header: the Zephyr
placement-`new` shim (`component_node.hpp:78-87`), `check_declared_depth`
(public, and with no ledger row the parity gate will not notice it go), and
`declare_parameter<std::vector<T>>`, which has no FFI to forward to.
`NROS_COMPONENT(Class)` needs none — zero invocations, and the entry
placement-news the class directly instead of calling its factory.

## W1 moves to phase-438 (2026-09-08)

W1 — "the out-of-line hosted block", `void* hosted_` replacing every hosted-only
member — is a consequence of the C++ std surface being DISCOVERED from the
toolchain rather than requested, not of the node merge. It relocates to
phase-438 W4, and phase-438 lands before this phase and before phase-426.

The reason, in one line: hosted-only MEMBERS exist because the hosted API is a
different SHAPE of the same class, and that is only true because
`NROS_CPP_HAS_*` are auto-detected. Once the surface is an opt-in, the hosted
half is additive methods over an unconditional layout, and W1 is what falls out
rather than what has to be engineered.

Two more of this phase's problems go with it. `rclcpp::Node`'s absence on
freestanding targets is that same guard (`nros.hpp:447`) and nothing else; and
`shared_from_this` stops being a layout question — the recorded decision and its
cost are unchanged, but it becomes a hosted-only method over a `weak_ptr` behind
`hosted_` rather than a base class that moves everyone's members.

Remaining here after the move: W2 (construction), W3 (one name, two signatures —
now four, see the conciliation section), W4 (`ComponentNode` deleted), W5
(`get_logger`), W7 (the `nros::Node` alias). W0 and W8 are landed.
