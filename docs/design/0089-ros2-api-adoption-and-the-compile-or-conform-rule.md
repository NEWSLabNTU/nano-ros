# RFC-0089 — ROS 2 API adoption, and the compile-or-conform rule

**Status:** Draft (2026-09-04)
**Amends / refines:** RFC-0036 (divergence catalog) — adds the DISPOSITION half:
a divergence must say what a porting user gets, not only why we differ.
RFC-0018 (C++ surface mirroring rclcpp) — states the condition under which the
mirror may take upstream's names.
**Motivated by:** phase-379's measurement of the three user APIs against rclc /
rclcpp / rclrs, and the intent to retire `rclcpp_compat.hpp` by making our own
names ROS 2's.
**Implements-tracked-by:** phase-417-ros2-api-adoption
**Governed by:** RFC-0019 / RFC-0020 (thin-wrapper discipline) — the Rust API
is the implementation source of truth; C and C++ delegate. This RFC does not
relax that, and §"Who implements an adopted name" states what it means for
adoption.
**Related:** RFC-0002 (one executor per RTOS task), RFC-0021 (blocking API
rules), RFC-0035 (RMW seam), RFC-0044 (component model); issues 1012, 1019, 1020.

## How to read this

Folded 2026-09-05. This RFC was written by accretion over the campaign, and the
order it was written in is not the order it should be read in — the governing
principle arrived after most of the rule it explains.

* **Part I** is the premise and the rule. The principle comes first because
  everything else is a consequence of it.
* **Part II** is the decisions, each with the date it was settled.
* **Part III** is the node and timer design.
* **Part IV** is argument order.
* **Part V** is what the parity measurement can and cannot show — read it before
  quoting any compatibility number.
* **The appendix** holds two superseded node-API drafts. They are kept because
  their MEASUREMENTS are still the evidence, and deleting them would delete the
  reasoning behind decisions that stand; their conclusions are not current.

Where a section is superseded, it says so at the top rather than being removed.

# Part I — The principle, and the rule that follows from it

## The governing principle (2026-09-05) — read this before the rule above

The compile-or-conform rule at the top of this RFC is a CONSEQUENCE. This is the
premise it follows from, stated after several sections had been written as if
the rule were the premise.

### The principle

> **Our constraints come first. Within them, port upstream. A name may be kept
> with a changed signature; a name may be invented where upstream has none. The
> success criterion is that porting a ROS 2 file is a MECHANICAL edit — every
> difference is one the compiler points at, and the fix is local and obvious.**

Three clauses, in strict priority order:

1. **RTOS and bare-metal constraints are non-negotiable.** No allocator on the
   dispatch path, no exceptions (RFC-0018), no RTTI, a fixed executor arena, no
   `dlopen`, `core`/`core+alloc` as the terminal state. An upstream shape that
   cannot be expressed under these is not adopted, however central it is
   upstream.
2. **Within them, port upstream.** Same name, same argument order, same
   semantics wherever the constraints permit. This is not a preference to be
   traded away for a nicer local design — RFC-0036 already forbids recording a
   preference as a divergence.
3. **Invent only where upstream has nothing.** A capability upstream lacks
   (caller-owned entity storage, `spin_once` with a budget, compile-time
   declared QoS) gets a name in `rclcpp::` and a ledger row saying it is ours.

This is **not a one-to-one mapping**, and it was never meant to be. A one-to-one
mapping would force clause 1 to yield to clause 2, which is backwards.

### What "mechanical" means, precisely

The user's edit must be *directed by the compiler*, never discovered at
runtime. That is the whole of the compile-or-conform rule, restated as a user
outcome:

| what differs | is it mechanical? | why |
| --- | --- | --- |
| name | yes — a rename | undeclared identifier |
| argument order or types | yes — reorder or restate | no matching overload |
| return type, **where the result is used** | yes | type mismatch at the use |
| **return type, where the result is discarded** | **NO** | nothing is said |
| behaviour behind an identical signature | **NO** | nothing is said, ever |

The last two rows are the reason the rule exists. The fourth is the subtle one
and it is live in this tree: **`rclcpp::init()` returns `void` upstream and
`Result` here.** At a call site that writes `rclcpp::init(argc, argv);` as a
statement, both compile, and the ported program silently stops checking an
error it never checked. That is a changed signature that the compiler does NOT
point at.

So clause 2's licence to change a signature carries an obligation:

> **A signature change is permitted when the compiler forces the edit. When it
> does not, the difference must be made loud by other means.**

For `init` the means is `[[nodiscard]]` on `Result` (or the
`NROS_NODISCARD` spelling for C++14 targets), which turns the discarded-result
case into a warning the `-D warnings` lanes already treat as an error.
`grep -n nodiscard` on `result.hpp` returns nothing today, so this is an
outstanding item rather than a description — filed against the node-merge work.

### How the four dispositions follow

`adopt`, `adopt-bounded`, `refuse-loud`, `absent` are not four independent
choices; they are what clause 1 does to a candidate from clause 2:

* the constraint permits it → **adopt**;
* the constraint permits a weaker form, and the weakening is visible → **adopt-bounded**;
* the constraint forbids it and the defect is knowable from the type →
  **refuse-loud** at compile time;
* the constraint forbids it and only the VALUE carries the defect → refuse-loud
  at the call, which is what `rclcpp::init(argc, argv)` does with `--ros-args`;
* upstream has it, we will not, and no ported file can reach it → **absent**.

### The one-directional consequence, restated

A ROS 2 file compiles here after a mechanical edit. A nano-ros file does not
compile against ROS 2 — it uses names and shapes upstream lacks. That asymmetry
is the principle working, not a gap in it: clause 1 outranks clause 2, so where
our constraints demand something upstream does not have, we have it and upstream
does not.

## The intent

nano-ros should be a near drop-in replacement for ROS 2 client code. The
end state is that our user API carries ROS 2's own names — `rclcpp::` for C++,
`rcl_`/`rclc_` for C, `rclrs::` for Rust — and `rclcpp_compat.hpp` is deleted,
because a shim exists only to bridge two spellings and there would be one.

This RFC is about what has to be true BEFORE that rename, and the rule that
decides, item by item, whether we may take a name at all.

## The hazard, stated once

Phase-379 W6 found `PollingSubscription::take()`. It had rclcpp's name, rclcpp's
signature, and the opposite contract: it drained to newest and returned true if a
value had ever arrived, so the idiomatic

```cpp
while (sub.take(msg)) { … }
```

terminates under rclcpp and spun forever here. **No compile error.** It was
deleted, and the deletion is recorded with the trap in the header.

That was one method found by accident. Adopting ROS 2's names across the whole
user API generalises exactly that hazard to every item whose semantics differ —
and 62 % of the rclcpp surface we do not cover is declined, meaning we do not
have those semantics.

A rename is therefore not a cosmetic change. It is the act of promising, for
every name taken, that the contract behind it holds.

And this is not hypothetical: the compat shim already ships two of them.
`ParametersQoS()` returns `QoS(10)` where `rmw_qos_profile_parameters` is
`KEEP_LAST, 1000` — a hundred-fold history difference under a name that claims
to be the ROS 2 profile, costing samples under load with nothing to read. Ten
`NodeOptions` setters store their argument in a private field that nothing
reads, and return `*this` so the idiomatic chained call compiles and configures
nothing; the header calls them "intentionally inert today" (`:125`), which is
the deferral this RFC is written to end. **A rename would inherit both, not
introduce them.**

## The rule

> **An upstream name may be adopted only if its observable contract is the same,
> or strictly weaker in a documented, non-inverting way. A contract that
> inverts, or that silently drops data or configuration, must fail to COMPILE.**
>
> Never compile and differ.

The rule is checkable against cases we already have, which is why it is stated
this way rather than as "match ROS 2 where possible":

| case | rule says | today |
| --- | --- | --- |
| `PollingSubscription::take()` — loop inverts | must not exist under that name | deleted (W6) |
| `RCLCPP_INFO_STREAM` — message discarded | must fail to compile | compiles, logs `""` (issue 1019) |
| `rclcpp::init(argc, argv)` — `--ros-args` dropped | must fail to compile, or honour them | compiles, drops them silently |
| `create_wall_timer` — period accuracy is the spin cadence | may be adopted, envelope documented | adopted, envelope undocumented |
| `Publisher::get_publisher_handle` — reaches past the API | absent is correct | absent (RFC-0035) |
| `ParametersQoS()` — history depth 10, upstream's is 1000 | must not carry that name at that value | **shipping** (`rclcpp_compat.hpp:117`) |
| `NodeOptions::use_intra_process_comms(true)` — stored, never read | must fail to compile | **shipping**, chainable, inert (`:125`) |

Note what the rule does NOT say. It does not require the implementation to
match; it requires the *contract a caller can observe* to match. A timer whose
period is quantised to the spin cadence is weaker, not inverted, and a caller
who is told so can act on it. A `take()` whose loop never terminates is not
weaker — it is a different function wearing the same name.

## Four dispositions

Every upstream item is exactly one of:

- **ADOPT** — same name, same contract.
- **ADOPT-BOUNDED** — same name and contract, weaker within an envelope stated
  in the doc comment. The envelope is part of the API, not a footnote.
- **REFUSE-LOUD** — we cannot have the contract, but the name is common enough
  that a user will reach for it. The name EXISTS as a deleted overload or a
  `static_assert`, whose message names the constraint and the nano-ros
  alternative.
- **ABSENT** — the name does not exist. Correct for rclcpp internals a user
  program never names.

REFUSE-LOUD is the disposition this RFC exists to introduce. Absence produces
`no member named 'create_generic_subscription'`, which is honest and teaches
nothing. A deleted overload produces the migration at the point of failure:

```cpp
// static_assert message, not prose in a book nobody opens
"rclcpp::Node::create_generic_subscription resolves the type by NAME at "
"runtime, which needs a dynamic loader and a heap. nano-ros resolves types "
"at build time. Use create_subscription<T>() with the concrete type, or "
"take_serialized() if you genuinely want bytes."
```

That costs one line and is the highest-leverage documentation in the design.

## The alias rule: where upstream HAS a name and we lack it, the alias is load-bearing (2026-09-09)

A rule that belongs beside the four dispositions, because it constrains which of
them a name may be given. It was learned by a gate going red, not by review.

> **For a name upstream HAS and we do not, a ported alias onto our nearest
> equivalent is LOAD-BEARING, not a courtesy — and it must NOT be deprecated.**
> "Do not take the name" is available only where upstream lacks the name too.

The reason is the campaign's flagship property, which is two-directional and
easy to state as if it were one-directional. `just colcon-parity` builds the
example templates against REAL rclcpp under a ROS 2 install, so a template must
compile **under upstream** as well as here. For any name in a template, the
useful set is the INTERSECTION of "compiles under real ROS 2" and "compiles
under nano-ros" — and an alias is what makes that set non-empty when the two
libraries spell the same concept differently.

Worked, and this is the case that produced the rule. Upstream has
`rclcpp::TimerBase` and has **no** `rclcpp::Timer` — measured against the
recorded surface (`docs/reference/api-surface/rclcpp.json`: `TimerBase`,
`WallTimer`, `GenericTimer` present; `Timer` absent). Ours is `rclcpp::Timer`
and we have no `TimerBase`. So for a node holding a timer:

| the member is written | under real ROS 2 | under nano-ros |
| --- | --- | --- |
| `rclcpp::TimerBase::SharedPtr timer_;` | compiles | compiles ONLY with the alias |
| `rclcpp::Timer timer_;` | **does not compile** | compiles |

Delete the alias and the intersection is EMPTY: no spelling of a timer member
satisfies both. That is not a lost convenience, it is the property gone.

Two corollaries, both about what a disposition may then say:

* **A deprecation is a promise to remove**, so deprecating such an alias
  promises to empty the intersection on a schedule. Deprecation is therefore
  forbidden here, unlike on a `nros::` transitional alias, where the promise is
  the point.
* **The direction matters and only one direction is free.** Where upstream lacks
  the name too (`spin_once`, `Node::init`), we may take whatever name we like —
  nothing of upstream's is being shadowed and no template loses a spelling.
  Where upstream HAS it, the name is not ours to decline.

`colcon-parity` is the gate that measures this, because it is the only lane that
compiles our text against upstream's headers. A rule about a two-directional
property needs a two-directional gate, and reasoning from our own headers alone
cannot see the row that is empty.

### This does not contradict "the one-directional consequence"

Part I says a nano-ros file does not compile against ROS 2, and calls that the
principle working. Both hold, because they are about different artifacts. A
nano-ros PROGRAM may use the out-ref `create_*` family and `spin_once` and will
not build upstream; that is clause 1 outranking clause 2 and it is fine. The
**example templates** are the one artifact held to both directions, deliberately
— they are the text a user copies to start from, and a starting text that only
builds here teaches upstream's users nothing. `colcon-parity` exists to hold
exactly that line, and this rule is what keeps a name available to it.

## The `_in` rule: an ours-only family may not sit under an upstream NAME (2026-09-09)

The sibling of the alias rule, and it points the other way. The alias rule is
about a name upstream HAS and we lack; this is about a family upstream lacks
that we were about to hang off a name upstream has.

> **A family with no upstream counterpart takes a different NAME, not merely a
> different signature.** C++ resolves a signature-only difference silently, so a
> ported line binds ours, compiles, and gets a different type, a different
> lifetime and a different failure channel with nothing said.

### The case that produced it, measured

On the merged node type (phase-427 W4) the two `create_publisher` forms become
overloads on ONE class:

```cpp
template <class M> ResultOf<Publisher<M>>       create_publisher(const char*,        const QoS&);  // ours
template <class M> std::shared_ptr<Publisher<M>> create_publisher(const std::string&, const QoS&);  // upstream's
```

A ported file writes `node->create_publisher<M>("chatter", 10)` and **binds
ours**. The reason is ordinary overload resolution and it is not close:
`const char[8]` → `const char*` is array-to-pointer decay, a standard conversion
(an lvalue transformation), while `const char[8]` → `std::string` needs
`std::string`'s converting constructor, a user-defined conversion. A standard
conversion sequence is a better conversion sequence than a user-defined one,
always, so ours wins with no ambiguity to report.

Measured rather than reasoned — a two-overload probe printing which one it
reached, on both compilers this tree gates with and both standards it builds at:

| | `-std=c++14` | `-std=c++17` |
| --- | --- | --- |
| gcc 11.4.0 | `OURS` | `OURS` |
| clang 14.0.0 | `OURS` | `OURS` |

Four for four, no diagnostic in any of them.

### Why this is the non-mechanical row, not a rename

Three things change under one spelling, and the governing principle's table
says the compiler points at none of them:

* **type** — a value, not a `std::shared_ptr`. A ported line usually writes
  `auto`, which absorbs it;
* **lifetime** — ours is a caller-owned cell whose address the arena records;
  upstream's is co-owned by the node through `owned_entities_` (see "The out-ref
  `create_*` family — forced by the arena");
* **failure channel** — ours is a `ResultOf<T>` the caller must inspect;
  upstream's path goes through `detail::require_created`, which aborts naming
  the verb.

That is "behaviour behind an identical signature" reached by a different route:
the signature is not identical, it is merely *convertible*, which for the caller
is the same thing.

**The out-ref family is NOT this hazard, and the difference is the point.**
`create_publisher(Publisher<M>&, const char*, const QoS&)` differs in ARITY, so
a ported two-argument call has no matching overload and the compiler names the
line. This RFC already calls that family "not an invention" for exactly that
reason. The hazard appears only where the two forms share an arity and differ in
a parameter type one implicitly converts to — which is what a value-returning
convenience form would have introduced.

### The spelling

**`_in`**, which is already in the tree rather than invented here:
`Node::create_timer_in`, `create_subscription_in` and `create_publisher_in`
(`packages/api/nros-cpp/include/nros/node.hpp:878`, `:909`, `:926`), where the
suffix marks the form that writes into caller-owned storage in a named callback
group. Under this rule the suffix generalises to the ours-only family as a
whole: caller-supplied storage, `Result` channel, no allocation — and the
callback group stays an argument of the overload that takes one, not part of
what the suffix means.

The cost is that a nano-ros program's creation verbs read differently from a
ported one's. That is the one-directional consequence again, paid where it is
visible instead of at a call site that compiled.

### Third application of one rule; the first two are recorded

This is not a new principle, it is the third place the campaign has reached it,
and stating that is what makes it a rule rather than three coincidences:

1. **The reordered C node initialiser** — Part IV, "The hazard, CORRECTED".
   C diagnoses an incompatible pointer argument as a WARNING by default, so a
   reorder is silent for exactly the out-of-tree callers who do not build with
   our flags. The conclusion there was already this one: *a C reorder must be
   accompanied by a RENAME, so a stale call fails on the identifier.*
2. **The clock-taking C timer verb** — phase-430 W1, shipped as
   `nros_timer_init_on_clock` rather than as a widened `nros_timer_init`
   (`packages/api/nros-c/include/nros/rcl_compat.h:389-398`). Its recorded
   reason is arity and the absence of C overloading, which is the same
   observation from the language that cannot hide it.
3. **This one**, which is the first time the mechanism is C++ overload
   resolution rather than C's permissiveness — and the first time the language
   actively picks ours.

The generalisation the three share: **a difference the language RESOLVES is a
difference the user never sees.** Whether it resolves by warning-not-error, by
arity, or by conversion rank does not change what the porting reader gets.

## A gate whose subject disappears passes vacuously (2026-09-09)

The campaign deletes things — `rclcpp_compat.hpp`, `pump()`, `ComponentNode`,
`nros::` as a home — and a deletion is the one edit that can turn a gate green
without anyone touching the gate.

> **When a deletion removes the file or symbol a gate keys on, the gate is
> retargeted IN THE SAME COMMIT, with a negative control proving it still fails
> on the condition it was written for.** A gate that no longer has a subject is
> not a gate that passes; it is a gate that has stopped answering, and it looks
> identical from the outside.

The negative control is the non-negotiable half. "I moved the pattern" is a
claim about the gate's text; "it fails on the mutation it exists to catch" is a
claim about the gate. This repo has the same rule for tests
(`check-no-vacuous-tests`) and for the merge-gating lanes
(`check-lane-contracts`); this states it for the deletion case, where the gate
was correct until the moment the subject went away.

**This is the campaign's most repeated defect class, and every instance was
found by measurement rather than by review:**

| where | what had stopped answering |
| --- | --- |
| `check-cpp-capability-layout`, W1's acceptance (issue 1204) | the gate forced each capability macro ON against a hosted baseline that self-defines all of them, so it compared the baseline against itself seven times — `3752` and `3752`. A broken implementation satisfied it. Rebuilt with a freestanding arm; the honest mutation is on `::nros::Node`, which sits under no capability guard. |
| `declared_qos_depth.cpp` / `_probe.cpp` (phase-427 W4) | a pair whose whole value is that the lane requires the SECOND to FAIL and greps its diagnostic. "Zero `ComponentNode` in the tree" is satisfied by a migration that leaves both compiling and proving nothing — which is why W4 now accepts on the probe still failing, with the lane's grep moving in the same commit. |
| `check-cxx-standard-floor`'s docstring | it names `component_node.hpp`'s `if constexpr` as the reason the floor is 17. Delete the header and the gate explains itself with a file that does not exist — the reason goes vacuous even where the check does not. |
| W4's RFC-0047 acceptance | *"the several-named-nodes capability survives"* — satisfiable only vacuously, because the capability was never there ("The several-named-nodes capability, corrected"). The subject-less variant: no deletion required, the subject never existed. |
| the `compat` translation unit in the C++ parity lane | reduced by the shim's deletion to a TU that emits only records the `std` TU already emits — measured, 0 of 162 records marked `ported`. A TU that can never contribute reads live and answers nothing; deleted. |
| `detail::argv_has_ros_args` | the `--ros-args` refusal, if inlined into `init`, would be observable only by a process that then dies. It is `constexpr` and `static_assert`ed in the ordinary compile lane precisely so it cannot quietly stop refusing — both silencing mutations were tried and both fail their own named assertion. |

Two of those (`declared_qos_depth` and `check-cxx-standard-floor`) are owed by
phase-427 W4's own deletion, which is what makes this a rule the merge has to
carry rather than a retrospective.

The rule extends past gates to PROSE that cites a subject, which is how this
class connects to the two staleness rules the campaign already runs:
`check-ledger-orphan-refs` (a ledger row may not cite a file that does not
exist) and issue 1022's class (prose citing a source that does not support it).
A reason nobody can check and a check nobody can fail are the same failure,
recorded in two places.

## Where the refusal fires: the earliest point the defect is KNOWABLE

`rclcpp::init(argc, argv)` forced this and it generalises.

The rule says a contract that silently drops configuration must fail to compile.
Applied literally to `init`, that means a `static_assert` on the two-argument
overload — which rejects **every** caller, including the overwhelmingly common
embedded one that forwards `main`'s argv and has no ROS arguments at all. It
also makes upstream's own tutorial `main` unportable, which collides head-on
with phase-417 stage 1's acceptance that the ported template be byte-identical
to upstream. Two parts of this design cannot both hold under the literal
reading.

The resolution is that the rule's real requirement is **loudness**, and compile
time is the earliest point loudness is *available* — not the only point it is
permitted:

> A refusal fires at the earliest point the defect is KNOWABLE. When the
> signature or the type carries it, that is compile time and a `static_assert`
> or `= delete` is correct. When only the VALUE carries it, that is the call,
> and a loud abort naming the unsupported input is correct. Silence remains
> forbidden at every point.

So `init(argc, argv)` compiles, scans for `--ros-args`, and aborts naming the
flag if it is present. A program with no ROS arguments is unaffected, because
nothing was dropped; a program that passes them dies immediately instead of
running for three hours on a remap that was never applied.

**The predicate must be separately checkable, or this becomes a check nothing
runs.** An abort inlined into `init` can only be observed by a process that then
dies, which is the shape of a gate that quietly stops working. Ours is
`constexpr` (`rclcpp_compat.hpp`, `detail::argv_has_ros_args`), so its cases are
`static_assert`ed in the ordinary compile lane — including the two mutations
that would silently stop it refusing: a null `argv` entry ending the scan early,
and a prefix match treating `--ros-args-extra` as the flag. Both were
mutation-tested and both fail their own named assertion.

This is a refinement of the rule, not an exemption from it. A disposition of
ADOPT-BOUNDED with a runtime refusal is still a promise that the contract never
differs silently; it just makes the promise where the information exists.

## Who implements an adopted name

RFC-0019 is Stable and says it plainly: **the Rust API is the source of truth;
C and C++ are thin shims that delegate.** Adopting ROS 2's names must not become
a licence to grow a second implementation behind them — a compat surface is
exactly where that pressure appears, because the fastest way to make a ported
line compile is often to write the logic in the wrapper.

The distinction that decides the hard cases:

> **Ergonomics may live in the wrapper. Behaviour may not.** A second
> *spelling* is free; a second *code path that can produce a different answer*
> is a violation.

Not a second implementation, and therefore fine in C/C++ alone:

* type aliases and nested typedefs (`Publisher<T>::SharedPtr`)
* inline forwarders and convenience overloads that convert and delegate
* `= delete` / `static_assert` diagnostics — a REFUSE-LOUD name emits no code
* doc comments, including the ADOPT-BOUNDED envelopes
* container/string conversions that copy and call through (`FixedString` ↔
  `std::string`)

A second implementation, and therefore Rust-side work first — these are
RFC-0020's five violation classes, restated as they show up in adoption:

* state machines (goal lifecycle, request/reply tracking)
* retry / timeout / polling loops that spin the executor from inside the wrapper
* CDR serialisation or deserialisation
* topic / service / action name construction, and remap resolution
* direct transport calls

### The consequence for planning

**A C or C++ gap is frequently a Rust gap wearing a wrapper's clothes**, and the
two cost very different amounts. So every work item carries the question *which
layer implements this?* and the order is Rust → FFI slot → C → C++.

The cross-language audit makes this cheap to exploit: of the 37 capabilities
where our own three surfaces disagree, roughly half are ones **Rust already has
and one or both wrappers never exposed** — named loggers and per-logger levels,
parameter type queries, undeclare, descriptors and ranges, QoS equality, GID
attribution, name resolution. Under SSoT those are the cheapest work in the
whole roadmap: the behaviour exists and is tested, and the wrapper is a
forwarder.

The remainder genuinely need Rust to grow first, and planning them as C/C++
tasks would have mis-costed them by an order of magnitude:

* typed C subscription delivery needs an `add_subscription` variant that carries
  caller-owned message storage through the FFI (stage 5);
* `rclcpp::init(argc, argv)` honouring `--ros-args` is remap resolution — name
  construction, violation class 4 — so the parser belongs beside
  `nros::resolve_name`, not in the shim;
* `rclcpp::Rate` is the sharpest case: a loop in the wrapper that spins the
  executor is violation class 2 *and* the thing RFC-0021 forbids. It is
  admissible only as a forwarder onto an executor-driving entry point that
  already exists in Rust — which is why it is planned that way rather than as
  the obvious fifteen-line C++ class.

### When a second path is warranted

Only when the wrapper must satisfy a language contract the Rust side cannot
express, and then it is recorded rather than assumed. The bar is the same one
RFC-0020 audits against, and a new second path needs: what language contract
forces it, why no FFI slot can carry it, and what keeps the two from drifting.
`nros::ResultOf<T>` (`Expected<T>` until phase-427 W8) is the model — it exists
because RFC-0018 forbids exceptions and Rust's `Result` cannot cross
`extern "C"`, and it holds no state of its own.

## Naming: replace, with alias as the migration step

The end state is the one the rename intends: **our API carries ROS 2's names.**
`nros::` stops being the spelling a user writes.

Getting there is a two-step, and the intermediate step is what makes it safe
rather than a flag day:

1. **Alias.** The ROS 2 spelling becomes a first-class name declared in the API
   headers, `nros::` remains and both work. `rclcpp_compat.hpp` disappears as a
   separate file because its content has moved into the headers it was shimming.
2. **Replace.** `nros::` is deprecated, then removed, and in-tree call sites
   (110 C++ and 75 C example files) migrate. This is the irreversible step for
   out-of-tree consumers and happens once, deliberately, with a changelog entry
   — the same discipline phase-379 W7 step 4 applies to the deprecated-alias
   batch.

### On ODR

An earlier draft of this RFC treated the ODR collision — nano-ros defining
`rclcpp::Node` in a build that also links real rclcpp — as a primary argument
for keeping `nros::` permanently. **That is overstated: nano-ros and ROS 2 are
not linked into one binary.** They interoperate over the wire, and the host-side
tooling that talks to a nano-ros image is a separate process.

It is kept as a guard rather than as an argument, because the guard is one line
and the failure it prevents is silent:

```cpp
#ifdef RCLCPP__RCLCPP_HPP_
#error "nano-ros and ROS 2's rclcpp are both in this translation unit. \
        They define the same names differently. Link one."
#endif
```

Cheap insurance against a configuration nobody intends, not a reason to keep two
vocabularies forever.

### What survives the correction

The ordering does, and it never depended on ODR:

> **The rename is cheap and cosmetic; the compatibility is the work.** Adopting
> ROS 2's names does not by itself make one more ported line compile. What makes
> lines compile is nested `SharedPtr` types, `create_service`, parameters on the
> node, `now()`. Those are needed whatever the namespace is called.

So the rename lands last. Doing it first would relabel the gaps without closing
one, and would spend the property that currently makes a mismatch visible —
our names differ, so a shape that differs is *visible* — while the mismatches
are still there. The two live inversions above (`ParametersQoS`, the inert
`NodeOptions` setters) are exactly what "inherited by a rename" looks like.

There WAS one mismatch the rename made strictly worse, and it is fixed
(corrected 2026-09-05). The shim `Node` pumped its own callbacks while
`nros::Node` was arena-driven, so a file mixing them got no callbacks and no
error, and only the differing NAMES made that visible at all. `pump()` and
`rclcpp_compat.hpp` were both deleted by this phase — every entity a
`rclcpp::Node` creates is now arena-registered through the same call the native
path makes, and `nros.hpp` says so at the call site. This paragraph outlived the
fix by a month and was still being cited as the reason the node-type merge was
not scheduled, which is the cost of stating a prerequisite in two documents and
retiring it in neither.

# Part II — Settled decisions

## Settled: `nros::` is phased out entirely; ours-only names take `rclcpp::` too (2026-09-05)

The previous section kept `nros::create_node`, `nros::bind_timer`,
`nros::Timer` and `nros::spin_once` in our namespace on the grounds that they
are ours-only. **That is overturned.** The goal is that a user writes `rclcpp::`
and never learns a second vocabulary, and a half-migrated API fails that goal
just as surely as a wrong signature does. `nros::` is phased out; the ours-only
names move to `rclcpp::` with the rest.

Two sub-cases, and only the second needed deciding:

* **Has an upstream counterpart** — moves, unconditionally. This was already
  the rule.
* **Has no upstream counterpart** — also moves. A user of nano-ros is writing
  against nano-ros; making them spell one capability `nros::` and its neighbour
  `rclcpp::` teaches a distinction that serves the implementer, not them.

The ODR objection is retired for the reason already recorded: we do not link
our `rclcpp` against ROS 2's in one image.

### The real hazard, and why it is not a reason to refuse

Taking a namespace we do not own means **upstream may later define a name we
have already taken**, with different semantics. Then a ported file's
`rclcpp::X` silently binds ours — compile-and-differ, deferred in time, which
is exactly what this RFC forbids.

It is not hypothetical. `spin_once` is not in rclcpp, but **`rclpy.spin_once`
exists**, with a different signature (`spin_once(node, timeout_sec=None)`)
against our `spin_once(timeout_ms) -> Result`. A user arriving from rclpy reads
our name and brings rclpy's meaning. And `Timer` is free in rclcpp today only
because upstream spells it `TimerBase` / `WallTimer` — a future `rclcpp::Timer`
is entirely plausible.

**So the decision comes with a tripwire rather than a hope.** We already record
the upstream surface (`docs/reference/api-surface/rclcpp.json`, 198 distinct
names today) and already diff our surface against it every `check-api-parity`
run. A new gate asserts:

> no name we define in `rclcpp::` as an OURS-ONLY extension may appear in the
> recorded upstream surface.

Today that passes: `Timer`, `spin_once` and `create_node` are absent upstream
while `TimerBase`, `WallTimer` and `Node` are present. The value is what happens
on the next `--refresh`: if upstream adds `rclcpp::Timer`, the refresh turns a
silent semantic collision into a hard red naming the symbol, at the moment the
recorded surface moves, and the response is a rename on our side. That is the
earliest point the defect is knowable, which is this RFC's own rule applied to
itself.

Every such name also keeps a ledger row with `disposition: extension` and prose
saying it is ours in upstream's namespace, so the parity report cannot be read
as "upstream has this".

**The gate's scope is OURS-ONLY EXTENSIONS, and the word is load-bearing
(2026-09-09).** A ported ALIAS onto one of our types is a name upstream HAS, so
it appears in the recorded surface by construction and must not trip the gate —
`rclcpp::TimerBase` is exactly that, and is required to exist (see "The alias
rule" in Part I). The two are distinguishable by the ledger, not by the surface
diff: an extension carries `disposition: extension` and a ported alias does not.
A gate keyed on "we declare it and upstream declares it" would fire on every
successful adoption, which is the opposite of what it is for.

### The cost, stated

The drop-in becomes **one-directional, and always was**. A ROS 2 file compiles
here unchanged; a nano-ros file using `rclcpp::spin_once` or the out-ref
`create_publisher` does not compile against real ROS 2. That is the correct
direction for the campaign — the goal is ROS 2 code running on nano-ros, not
the reverse — but users will assume symmetry, so the book must say it plainly
rather than letting a build failure say it later.

## Settled: `rclcpp::` is the HOME, not an alias onto `nros::` (2026-09-09)

The section above settles WHICH SPELLING a user writes. It leaves open the
question the node-type merge ran into, which is a different one: **where is the
type DEFINED?** A `class nros::Node` with `namespace rclcpp { using Node =
::nros::Node; }` satisfies "the user writes `rclcpp::`" and still makes `nros::`
the home. The two are not the same decision, and the merge could not proceed
without the second.

**Settled: `rclcpp::` is the home.** The class is `class rclcpp::Node`; `nros::`
holds the transitional alias, deprecated and then deleted. That is the direction
"The node API, revised" already stated ("`rclcpp::Node` is the class and
`nros::Node` is the alias, the reverse of the shim we are retiring"), now settled
as a decision rather than left as a draft's aside, and extended to every type the
sweep reaches.

### What blocked it was the MEASURING TOOL, and the tool is what changed

The node-type merge found that defining the merged class in `rclcpp::` turned
`just check api-parity` red, and kept the class in `nros::` with an alias
because of it. The red was real and the diagnosis was right: the C++ lane rooted
OUR native surface at namespace `nros` alone, so a class defined in `rclcpp::`
was extracted only by the compat translation unit, landed `surface: ported`, and
its `native_bucket` — the bucket `--check` actually gates — read **`theirs-only`**.
About sixty members of one merged class re-bucketed as names only ROS 2 has.

The instrument was measuring the half of our vocabulary we are deprecating. Under
this decision that rooting is not conservative, it is backwards: it reports the
home namespace as foreign. So the tool was fixed, not the header.

`scripts/api-parity.py` now roots every C++ translation unit at `OUR_CPP_ROOTS =
{nros, rclcpp, rclcpp_action, rclcpp_lifecycle}` — the vocabulary we ship, both
halves of it. Three properties, each pinned by `--self-test`
(`just check api-parity-ledger`):

* a name **we** declare in `rclcpp::` correlates as OURS, on the native surface;
* a name **upstream** declares that our headers do not still correlates
  `theirs-only`. Widening our roots cannot move it, because `theirs` comes from
  the recorded upstream surface and never from a namespace filter. Measured on
  the real lane: `WallTimer`, `Waitable`, `Waitable::is_ready` and
  `uninstall_signal_handlers` read `theirs-only` before and after;
* the widening is a NAMED LIST, not a prefix match — `rclcpp::detail` stays out,
  because it is upstream's internals and admitting it would import a private
  surface as ours.

Mutation-tested in both directions: narrowing the roots back to `{nros}` fails
the first check, and adding `rclcpp::detail` fails the third. And end-to-end
through clang: declaring `rclcpp::uninstall_signal_handlers` — a name that is
upstream-only today — in one of our headers moved it `theirs-only` → `same` on
the gated surface (`theirs-only` 651 → 650). Under the OLD rooting the same
declaration moved the ported summary and left the gated native summary at
`theirs-only` 695, unchanged. That is the defect, reproduced on demand.

Two consequences of the fix, both stated rather than absorbed:

* **The C++ lane now reports ONE surface.** The `compat` translation unit had
  been reduced by the shim's deletion to "the same header, read for the other
  namespace", and once that namespace is ours it emits only records the `std` TU
  already emits — measured, 0 of 162 records marked `ported`. A TU that can never
  contribute is the gate shape that reads live and answers nothing, so it was
  deleted. The `surface` / `native_bucket` / `--check-ported` machinery stays; it
  is one tuple row to re-admit a genuinely separate ported header. Issue 1020's
  question is not retired, its ANSWER is: with one set of headers and one home
  vocabulary there is one surface.
* **The gate got strictly stronger and cost nothing.** It used to enforce on the
  native surface and merely REPORT the ported one; now the union is enforced.
  Measured, `--check --require-disposition` stayed green with **zero** new ledger
  rows — the ledger had already been written against the union. Bucket counts,
  before → after (C++): `same` 88 → 135, `arity-only` 12 → 11, `systematic`
  12 → 9, `differs` 20 → 21, `ours-only` 354 → 360, `theirs-only` 695 → 651.
  Forty-four names we ship stopped being reported as names only ROS 2 has.

### The flip is UNBLOCKED; what remains is a sweep

Nothing in the tooling now argues for defining a type in `nros::`. What remains
is mechanical and belongs to the phase-428 sweep: **nine types are defined in
`nros::` and aliased into upstream's namespace**, and each moves its definition
across, leaving the alias pointing the other way.

| aliased as | defined as | header |
| --- | --- | --- |
| `rclcpp::Clock` | `nros::Clock` | `clock.hpp:135` |
| `rclcpp::Duration` | `nros::Duration` | `duration.hpp:164` |
| `rclcpp::Time` | `nros::Time` | `time.hpp:158` |
| `rclcpp::Publisher<M>` | `nros::Publisher<M>` | `publisher.hpp:386` |
| `rclcpp::Subscription<M>` | `nros::Subscription<M>` | `subscription.hpp:880` |
| `rclcpp::Client<S>` | `nros::Client<S>` | `client.hpp:432` |
| `rclcpp::Service<S>` | `nros::Service<S>` | `service.hpp:334` |
| `rclcpp_action::Client<A>` | `nros::ActionClient<A>` | `action_client.hpp:542` |
| `rclcpp_action::Server<A>` | `nros::ActionServer<A>` | `action_server.hpp:651` |

`Node` is the tenth and is the node-type merge's own file, in flight; it is the
one that surfaced the problem and is not swept here.

The move is a one-way rename per type and each is independently landable, so the
sweep does not need a flag day. What it does need is the ORDER this RFC already
states: the compatibility is the work, the rename is cheap, and a type is only
worth moving once its contract is the one the name promises.

## Settled: state hides behind a pointer only when it EXCEEDS a pointer (2026-09-09)

The node's `void* hosted_` (phase-438 W4) is right, and the two sections above
give the reason in terms of the node alone. Applied as a habit it is wrong for
most types, and the C++ sweep was about to apply it as a habit — so the rule is
stated with both halves.

> **Configuration-dependent state goes out of line behind one unconditional
> pointer when it is LARGER than a pointer. When it is the SIZE of a pointer,
> the member stays, unconditionally, in every configuration.**

Both halves are the same arithmetic, and neither is a style preference.

**The large half.** `rclcpp::Node`'s hosted-only state is a `weak_ptr`, the
owned-entity list and the rest of the shape a ported file reaches, against a
192-byte freestanding node. Hiding it buys the whole 0135/0460 property: one
layout in every TU of one image, so a freestanding TU holding a node by value
and a hosted TU taking `Node&` agree about every member offset. The indirection
is paid on hosted-shape calls only, and never on a freestanding target, where
the pointer is null and the block is never allocated. `check-cpp-capability-layout`
is what measures it.

**The small half, and this is the one that had to be decided.** `nros::Timer`
and `nros::GuardCondition` each carry exactly one configuration-dependent
member: a `std::unique_ptr<std::function<void()>>` under `NROS_CPP_STD`
(`timer.hpp:159-169`, `guard_condition.hpp:119-123`), which owns the closure the
`NROS_CPP_STD` convenience wrappers heap-allocate. Measured, on the member
layout the two classes actually declare:

| type | freestanding | `-DNROS_CPP_STD` | delta |
| --- | ---: | ---: | ---: |
| `nros::Timer` | 24 | 32 | **8** |
| `nros::GuardCondition` | 32 | 40 | **8** |
| `void*` | | | 8 |

The delta IS a pointer. So hiding it behind a pointer costs the same eight
bytes it was going to cost, adds a dereference on the dispatch path, and adds an
allocation and its failure mode — in exchange for nothing, because the eight
bytes were the whole disagreement.

**Decision: both take the member unconditionally, in both configurations.** A
freestanding image pays eight bytes per timer and per guard condition for a
layout that is the same everywhere, with no one-definition hazard to reason
about and no indirection where the executor dispatches. The member is unused
freestanding — the wrappers that populate it do not exist there — which is
exactly what "pay eight bytes for one stable layout" means.

The rule, restated as the test to apply to the next type the sweep reaches:
*measure the delta first.* If it exceeds a pointer, indirection is a saving. If
it is a pointer, indirection is the same cost plus a dereference plus an
allocator, and the honest answer is to stop hiding it. If it is SMALLER than a
pointer — a `bool`, a `uint8_t` — indirection is a net loss and the question does
not arise.

Two things this deliberately does not do. It does not gate the member on
`NROS_CPP_STD`, which is the shape that MAKES the hazard (a base or a member
present in one TU and absent in another is issues 0135 and 0460, and px4 sets
`-DNROS_CPP_STD` on one module of a larger image on purpose). And it does not
hand-mirror a standard-library type's size to "equalise" the two
configurations — this repo has a gate against that shape for FFI structs
(`check-ffi-struct-mirrors`) and a standard-library internal is a worse thing to
mirror than our own header, not a better one. Declaring the real member is the
only way to be sure the two configurations agree about it.

## Settled: C takes rcl's spellings (2026-09-04)

The question the disposition pass could not answer, because it is a decision and
not a reading: does the C API keep `nros_publisher_get_zero_initialized`
(module-first, our convention) or take `rcl_get_zero_initialized_publisher`
(rcl's free-function shape)?

**It takes rcl's.** The goal is drop-in replacement, and a ported file's line is
rcl's line. That resolves a real split — the same class landed `adopt` in four
ledger shards and `refuse-loud` in one, which was never agent disagreement but
an unmade decision surfacing five times.

Consequences, in the order they bite:

* **Every row declined on SPELLING alone flips to `adopt`.** Those declines
  argued from our naming convention, and RFC-0036 already forbids recording a
  preference as a divergence. They were the largest coherent group in the C
  ledger with no platform content.
* **New C entry points land under `nros_` and reach the user as `rcl_`.** The
  alias comes from `<nros/rcl_compat.h>`, not from renaming as we go: a
  half-renamed C surface is worse than either end state, and RFC-0089's
  migration is alias-then-replace precisely so the two steps are separable.
* **`nros_ret_t` is the sharpest case and is NOT a rename.** Its doc claims
  compatibility with `rcl_ret_t` and only `OK` agrees — ours are −1/−2/−3/−7,
  rcl's are 1/2/11/101. The compat header MAPS the constants; renumbering ours
  would silently flip the meaning of every stored return code across three FFI
  seams. This is the one place where taking rcl's spelling must not mean taking
  rcl's values.

## Settled: each language follows ITS OWN upstream (2026-09-04)

Where the three upstreams disagree with each other, we follow each language's
own. The worked case is logging:

```
rclrs    log_info!(logger, "…")
rclcpp   RCLCPP_INFO(logger, "…")
rcl/rcutils  RCUTILS_LOG_INFO_NAMED(name, "…")
```

There is no single "ROS 2 spelling" to match, so "respect the official API" does
not resolve it on its own. The tie-break is what a porting user actually reads:
**one client library, not three.** A Rust node ported from rclrs is read beside
rclrs; matching rclcpp there would be matching a library that user never opens.

**This deliberately gives up part of stage 4's convergence, and that is the
cost, stated rather than hidden.** Stage 4 spent real effort making our three
surfaces agree — and that was always INSTRUMENTAL to drop-in, never an end in
itself. Where agreeing with each other and agreeing with upstream pull apart,
upstream wins, because upstream is what a ported file was written against.

The rule survives the exception: our languages must still agree about
CAPABILITY and CONTRACT. What may differ is the spelling, and only where the
upstreams differ first. A capability present in one of our languages and absent
in another is still the defect stage 4 exists to remove.

## Settled: the typesupport parameter keeps OUR type (2026-09-04)

`rcl_publisher_init` takes `const rosidl_message_type_support_t *`; ours takes
`const nros_message_type_t *` in the same position. We keep ours.

Not a preference — a rosidl typesupport's MEMBERS are its contract
(`typesupport_identifier`, `data`, and the `func` dispatcher that resolves the
implementation at runtime), against our flat `type_name` / `type_hash` /
`serialized_size_max`. Adopting the name would claim a dispatcher we do not
have, and the "compiles and differs" that follows is exactly what this RFC
forbids.

It costs a ported call site nothing today, because the argument comes from our
codegen either way — `ROSIDL_GET_MSG_TYPE_SUPPORT` does not exist here. What it
costs is one type NAME visible in a signature after the compat layers are gone,
and that is the honest price of not faking a structure.

## End state: no compat layer survives

Confirmed 2026-09-04. The compat headers are SCAFFOLDING with a demolition
date, not a permanent surface:

* `rclcpp_compat.hpp` and `<nros/rcl_compat.h>` exist only to bridge two
  spellings of one thing. When there is one spelling they have nothing to
  bridge, so they do not have to be argued away — **they dissolve by
  construction**, which is the property that keeps them from rotting into
  permanent debt.
* `cmake/compat/` likewise: it shadows `<rclcpp/rclcpp.hpp>` and stubs
  `find_package(rclcpp)` because our headers live elsewhere under other names.

The acceptance test for the end state is therefore sharp and mechanical: **the
ported templates must build with every compat layer deleted.** Not "still
compile with the shim present" — deleted.

### The one thing that does NOT dissolve, and why that is fine

`rcl_compat.h` does two jobs, and only one is a spelling. The other is the
`RCL_RET_*` value mapping, and RFC-0089 forbids renumbering ours to match rcl's
— that would silently flip the meaning of every stored return code across three
FFI seams.

So `RCL_RET_TIMEOUT` does not go away; it becomes the NAME of our constant, with
our value. A ported `if (ret == RCL_RET_TIMEOUT)` is correct. A program that
hardcodes `if (ret == 2)` is not, and never was. That is a defensible line: we
adopt rcl's vocabulary, not its numbering, and the vocabulary is what source
compatibility is made of.

## The error channel, settled (2026-09-05)

Two channels, and the value-carrying one is renamed:

| the operation | channel |
| --- | --- |
| cannot fail; answers a question | `bool` |
| can fail, produces nothing | `Result` |
| can fail, produces a value | `ResultOf<T>` — **today's `Expected<T>`, renamed** |
| **upstream THROWS** | `Result<T>` (or `Result`), never an exception — RFC-0018 |
| a ported API whose upstream channel is `bool` | `bool`, unchanged |

`Result` becomes the `void` case of one template, with `Result` as the alias, so
there is one template rather than two unrelated types a reader must learn.
`Expected<T>` survives as a deprecated spelling for one release.

**CORRECTED 2026-09-07, when W8 implemented it: the template cannot also be
called `Result`.** This section said `Result<T>`, and that spelling is not
available — an identifier in a C++ scope names a class, a class template, or an
alias, never two of those. All three routes were measured on gcc 12.3 and
clang 14 and all three are ill-formed:

```
template <typename T = void> class Result;  Result f();
    "invalid use of template-name 'Result' without an argument list" (c++14)
    "deduced class type 'Result' in function return type"           (c++17)
template <typename T> class Result;  class Result { };
    "class template 'Result' redeclared as non-template"
template <typename T> class Result;  using Result = Result<void>;
    "redeclared as different kind of entity"
```

So the STRUCTURE this section asks for landed in full — one template, the
value-less case IS its `void` specialization, `Result` is an alias for that
specialization, and there are no longer two unrelated types — and only the
template's own spelling differs: it is **`ResultOf<T>`**. `Result` keeps the
bare name because that is what 1100+ call sites and the whole public API already
write for the void case, and a name used bare is the one worth keeping bare.

The deprecation of the old name is a `[[deprecated]]` **class** template that
converts from `ResultOf<T>`, not the alias template the obvious reading asks
for: measured, `[[deprecated]]` on an alias template warns on gcc 12.3 and is
SILENT on clang 14, both standards. A deprecation half our users are never told
about is just an alias, so the shape follows the diagnostic.

Two properties this has to keep, both already true and both worth stating so a
refactor does not lose them:

* **Both reach every target.** `result.hpp` carries zero capability gates and
  parses under the ThreadX `-nostdinc++` shim — measured by the header lane, not
  assumed. The rename must not introduce a gate.
* **Neither may be silently discarded.** `[[nodiscard]]` (`NROS_NODISCARD` on
  C++14) on both, which is what makes `rclcpp::init(argc, argv);` as a bare
  statement a warning the `-D warnings` lanes already treat as an error. This is
  the whole reason the channel question mattered: upstream returns `void` there,
  and a widened return type at a discarding call site is a signature change the
  compiler does not point at.

## Settled: the three questions the node-type merge was waiting on (2026-09-05)

The structural blocker was retired (see above); what remained were three
decisions, not readings. All three are now made.

### 1. `get_logger()` follows ROS 2 after the merge

Today the two spellings collide on an identical signature: `rclcpp::Node::
get_logger()` returns a `rclcpp::Logger` built from the hardcoded sentinel
`"nros.compat"`, while `nros::Node::get_logger()` returns the real
`nros_log::Logger` handle keyed on the node's own name. Same name, same arity,
different observable behaviour — case (3) of "four ways a row correlates `same`
falsely", which this RFC already names.

**The merged accessor takes ROS 2's behaviour: a logger named for the node.**
The sentinel was a placeholder for a shim that no longer exists, and a logger
that cannot tell you which node emitted a record is worse than the one it
replaced. This is a strict improvement in both directions, so it needs no
disposition beyond `adopt`.

### 2. Mirror ROS 2's node types — and the count that implies is ONE

The instruction was to mirror rclcpp and avoid duplicates. Measured, the mapping
that motivated the question is **inverted**, so recording it before acting:

| type | where it is actually used |
| --- | --- |
| `nros::Node` | everywhere, and MOSTLY IN WORKSPACES — 51 files under `workspaces/features`, 41 under `workspaces/cpp`, 30 under `workspaces/c`, 27 under `workspaces/mixed`. The RFC-0043 typed component takes it by reference: `Result Configure(::nros::Node&)`. |
| `nros::ComponentNode` | **five directories total** — one standalone POC (`native/cpp/component-node-poc`) and two workspace subnode packages. |

So it is not "`Node` for standalone, `ComponentNode` for workspaces". Both live
in workspaces; they differ by ROLE. `nros::Node` is a node you are handed;
`ComponentNode` is a base class you derive from, constructed from an
entry-supplied `NodeHandle`.

Upstream has no counterpart to that distinction. `rclcpp_components` composes
plain `rclcpp::Node`s — a ported component derives from `rclcpp::Node` and there
is no second node type to mirror. Mirroring therefore means **one node type**,
with `ComponentNode`'s two distinguishing features demoted from a type to
constructors on it:

* construction from an entry-supplied `NodeHandle` rather than the global
  executor — a constructor overload.

That is the whole list. An earlier draft had a second item — "RFC-0047's one
component, several named nodes", called genuinely ours-only — and **it does not
exist**; see "The several-named-nodes capability, corrected" below. So
`ComponentNode` carries exactly one thing that is not already on `Node`, and it
is a constructor.

This also closes the duplication `nros.hpp` already flags against itself
("KNOWN DUPLICATION… There should be ONE helper"): the parameter facade exists
twice, once in C++14 on `rclcpp::Node` and once in C++17 `if constexpr` on
`ComponentNode`. One node type is what makes one facade possible.

### 3. Parameters live in Rust; C and C++ are thin wrappers

RFC-0019/0020 already say the Rust API is the implementation SSoT and that
ergonomics may live in the wrapper while behaviour may not. A node-local C++
parameter store is behaviour living in the wrapper, and it has the visible
consequence that a parameter declared through `rclcpp::Node` does not appear in
`ros2 param list` — the store the parameter SERVICES read is the executor's, in
Rust.

**The merged node's parameters are the Rust store.** `rclcpp::Node`'s inline
`ParameterServer<NROS_RCLCPP_MAX_PARAMS>` member and `ComponentNode`'s facade
both become forwarders to it.

The gap this exposes is small and specific: the FFI has
`nros_cpp_declare_param` plus typed getters (integer / double / bool / string)
and **no setter at all** — `grep -c nros_cpp_set_param` is 0, and there is no
`set_parameter` on the Rust executor either. `set_parameter<T>` therefore has
nothing to forward to yet, so the merge carries one piece of Rust-side work
rather than a pure C++ refactor. That is the right shape: the capability is
missing where the SSoT is, and adding it in C++ would have been the second
implementation this rule exists to prevent.

Resolves the C++ half of issue 0793.

# Part III — The node, the timer, and what stays invented

## The rclcpp node model, and what nano-ros actually needs from it (2026-09-05)

Decision 2 above says "mirror rclcpp, which means one node type". That is the
conclusion; this section is the reasoning, because the question it answers —
*we don't build standalone node binaries, so doesn't that force a node type of
our own?* — is a good one with a surprising answer.

### What rclcpp actually has

Four things that are easy to conflate:

1. **`rclcpp::Node`** — an ordinary C++ object. Creating one does not create a
   process, a thread, or a scheduler. It registers a node with the middleware
   and owns the entities you create through it.
2. **An executor** — a separate object that *drives* nodes. Nodes are added to
   it (`add_node`), and it dispatches their callbacks. `rclcpp::spin(node)` is
   sugar that constructs a `SingleThreadedExecutor`, adds the node, and spins.
3. **Composition** — several nodes in ONE process, on one executor. Upstream
   supports two ways to get there, and this is the crux:
   * *dynamic composition*: `rclcpp_components` `dlopen`s a shared library and
     instantiates a registered class into a running container process;
   * **manual composition**: the components are LINKED into one executable
     whose `main` constructs each of them and adds them to one executor.
4. **`rclcpp_lifecycle::LifecycleNode`** — a genuinely distinct type, with a
   state machine and its own interface set.

### The answer to "we link, we don't spawn"

**Manual composition is upstream's own answer to exactly our constraint, and it
uses a plain `rclcpp::Node`.** A component in that model is a class that derives
from (or holds) `rclcpp::Node`, with a constructor taking `NodeOptions`; the
`RCLCPP_COMPONENTS_REGISTER_NODE` macro is what makes it *additionally*
loadable, and omitting it costs nothing but that.

So the constraint that motivated a nano-ros-specific node type — one image, no
`dlopen`, no per-node binaries — is a constraint upstream already lives under
whenever anyone does manual composition. It does not require a new node type. It
requires the *component* shape, which is a class, not a node type.

That is why the mapping collapses:

| rclcpp | nano-ros |
| --- | --- |
| `rclcpp::Node` | the one node type |
| manual composition (`main` constructs components, adds to one executor) | **exactly our model** — the generated entry constructs each component and the executor drives them |
| `RCLCPP_COMPONENTS_REGISTER_NODE` + `dlopen` | absent, deliberately: no dynamic loading on firmware |
| `NodeOptions` | `NodeOptions` |
| `LifecycleNode` | our lifecycle set, a separate type as upstream has it |

`nros::ComponentNode` was invented for the row that turns out not to need a new
type. What it really carries is two things, and neither is a node kind:

* **an entry-supplied `NodeHandle`** instead of the global executor. Upstream's
  equivalent is that a manually-composed node is added to an executor the `main`
  owns. This is a CONSTRUCTOR, and its nearest upstream spelling is passing
  `NodeOptions` and letting the caller do `executor.add_node(...)`.

There was a second bullet here — "RFC-0047's one-component-several-named-nodes",
described as ours-only and as a reason to keep a distinction. **It was a
misreading of RFC-0047 and the capability is not in the tree**; the correction is
the next section. `ComponentNode` carries ONE thing, and it is a constructor.

### The several-named-nodes capability, corrected (2026-09-09)

Six places in this RFC and two in phase-427 attributed "one component, several
named nodes" to **RFC-0047**, and listed it as an ours-only capability the merged
node type must preserve — including as one of "three constructors" on it, one of
which existed for no other reason. **Every one of those citations is wrong, and
the capability they name does not exist.** Measured, not reviewed:

* **RFC-0047 is `docs/design/0047-unified-sched-context-binding.md`, "Unified
  sched-context binding via callback groups".** Its normative content is that a
  callback binds to a sched-context at the granularity ROS 2 uses — the CALLBACK
  GROUP — with group structure in code and group→tier policy in `system.toml`.
  Grepping it for a second node identity returns nothing. Its one sentence about
  several nodes is a CITATION OF RFC-0046, not a claim of its own.
* **`ComponentNode` owns exactly one node.** `component_node.hpp:734` is
  `Node node_;`, a single member; RFC-0044 Q1 chose WRAP over derive precisely so
  the component holds one `Node`. Every `create_*` forwards to that one member.
  There is no second identity to name, and no API that could ask for one.
* **The packages said to exercise it exercise callback groups.** Both subnode
  packages (`examples/workspaces/realtime-cpp{,-subnode-portable}/src/subnode_pkg`)
  say so in their own first line — "ONE ComponentNode with TWO callback groups" —
  and their body creates `ctrl` and `telem` groups on one `sub_node`. That IS
  RFC-0047, and it is the reason the citation looked plausible: the packages are
  the RFC-0047 proof, just not of this.

**What is real, under its real name.** Several named nodes in ONE IMAGE exists,
and it belongs to the **EXECUTOR**, not to a component: `Executor::create_node`
and `Executor::node_builder(name)` (`executor.hpp:180`, `:189`; the Rust twin in
"Context and `init`, settled"). Its source is **RFC-0046** — launch-authoritative
node identity, the single `node_builder(name)` funnel — whose own summary says a
multi-component launch yields **per-component graph nodes**, one each. RFC-0047
cites RFC-0046 for exactly this, which is the likeliest route the misattribution
took.

So the correct statement of the model is one node per component, several
components per image, all on one executor — which is upstream's manual
composition unchanged, and needs no ours-only capability at all.

**Consequences, applied throughout this RFC:**

* the merged type has **TWO** constructors, not three. The third existed only to
  preserve this;
* nothing here is an ours-only "several identities" divergence, so no ledger row
  is owed for one;
* phase-427 W4's acceptance is amended to match (its subnode-package clause is
  correct as written and stays — those packages are the callback-group proof).

### What this means for the merge

One node type, two constructors: the global-executor one (`rclcpp::Node`'s, what
a ported file writes) and the handle-taking one (what a generated entry writes).
The RFC-0043 typed component keeps taking a node by reference, which is unchanged
— it was never a node type, only a `configure(Node&)` convention.

The thing to NOT do is preserve two types because they have two construction
paths. `rclcpp::Node` already has several constructors and remains one type;
construction is not identity.

## The node API, proposed under the governing principle (2026-09-05)

Supersedes "The node API, revised" above, which predates the principle and kept
four names in `nros::`. Every decision below is annotated with the clause it
follows from: **[1]** constraints first, **[2]** port upstream within them,
**[3]** invent only where upstream has nothing.

### Type and namespace

```cpp
namespace rclcpp {
class Node { /* ... */ };
}
namespace nros { using Node = ::rclcpp::Node; }   // transitional; deprecated, then deleted
```

`rclcpp::Node` is the class **[2]**. One type — `ComponentNode` is deleted, not
aliased, because once its handle became a constructor it had no distinction
left. (CORRECTED 2026-09-08: the class is DEFINED in `nros::` and aliased into
`rclcpp::`, for a measured reason about `api-parity.py`'s namespace roots —
§"AMENDED 2026-09-08", item 2. Both spellings are one type. `ComponentNode` is
still present; phase-427 W4 is not started.) `nros::` survives only as a migration alias so the remaining call sites
are optional to move rather than a flag day.

**Layout is probe-independent [1].** Hosted-only state lives out of line behind
one unconditional pointer, allocated lazily on the first hosted-shape call and
never on a freestanding target:

```cpp
nros_cpp_node_t handle_;
bool            initialized_;
void*           executor_handle_;
Clock           clock_;
void*           hosted_ = nullptr;   // detail::NodeHosted*, null on freestanding
```

Enforced by `check-cpp-capability-layout`; this is the rule `timers_` broke.

### Construction

Upstream constructs a node with a name and throws on failure. We cannot throw
**[1]**, so the constructor is kept and the failure channel changes **[2]**:

```cpp
Node() noexcept;                                                  // [3] uninitialised, pair with init()
explicit Node(const char* name, const char* ns = nullptr) noexcept;  // [2] upstream's shape, no allocator
Result init(const char* name, const char* ns = nullptr) noexcept;    // [3] explicit-code form
bool   ok() const noexcept;                                          // [2] replaces the throw
```

Hosted adds upstream's exact spellings **[2]**:

```cpp
explicit Node(const std::string& name, const NodeOptions& = NodeOptions());
Node(const std::string& name, const std::string& ns, const NodeOptions& = NodeOptions());
```

`ok()` is **adopt-bounded**: upstream throws, we set a flag. That is weaker, and
the weakening is only safe if it is loud. The generated entry checks `ok()` and
halts naming the node (RFC-0044 Q2 already does this); a standalone `main` must
check it, and the book must say so. This is the same class as `init()`'s
discarded `Result` — a difference the compiler does not point at — so it is an
outstanding loudness item, not a solved one.

### Entity creation — one name, two signatures

Upstream's name is kept and the signature changes **[2]**. The compiler forces
the edit: there is no overload taking `(const std::string&, int)` on a
freestanding target, so a ported line fails to compile rather than differing.

```cpp
// [2] freestanding — caller-owned storage, no allocator, every target
Result create_publisher   (Publisher<M>&,    const char* topic, const QoS& = {}) noexcept;
Result create_subscription(Subscription<M>&, const char* topic, void(*)(const M&), const QoS& = {}) noexcept;
Result create_service     (Service<S>&,      const char* name,  ..., const QoS& = ServicesQoS()) noexcept;
Result create_client      (Client<S>&,       const char* name,  ..., const QoS& = ServicesQoS()) noexcept;
Result create_wall_timer  (Timer&, uint64_t period_ms, void(*)(void*), void* ctx) noexcept;

// [2] hosted — upstream's exact signatures
std::shared_ptr<Publisher<M>>    create_publisher<M>(const std::string& topic, const QoS&);
std::shared_ptr<Subscription<M>> create_subscription<M>(const std::string& topic, const QoS&, Cb);
std::shared_ptr<TimerBase>       create_wall_timer(std::chrono::duration<...>, Cb);
std::shared_ptr<Service<S>>      create_service<S>(const std::string& name, ...);
std::shared_ptr<Client<S>>       create_client<S>(const std::string& name, ...);
```

**Member-function binding folds into `create_wall_timer` [2] rather than being a
free `bind_timer` [3].** A component binds its own method with no allocation and
no `std::function`:

```cpp
template <typename C, void (C::*M)()>
Result create_wall_timer(Timer& out, uint64_t period_ms, C* self) noexcept;
```

That removes an invented name by reusing an upstream one — which is what clause
2 asks for, and it is why this proposal has fewer inventions than the previous
one.

### What is invented, and why each is unavoidable

| name | why upstream has nothing | tripwire |
| --- | --- | --- |
| `rclcpp::Timer` | ours is a HANDLE; upstream's `TimerBase`/`WallTimer` are a virtual hierarchy we have no vtable budget for **[1]** | absent upstream today; `TimerBase`/`WallTimer` are present, so the gate watches for `Timer` appearing |
| `rclcpp::spin_once(timeout_ms)` | upstream's one-cycle verb is `spin_some(node)`, with no timeout and no return | absent in rclcpp — but **`rclpy.spin_once` exists with a different signature**, so the ledger row must say so |
| `Node::init` / `Node::ok` | upstream throws; a no-exception target needs a channel **[1]** | absent upstream |
| the out-ref `create_*` family | caller-owned storage has no upstream counterpart **[1]** | shares upstream's NAME, so it is a ported signature, not an invention |

Each invented name carries a ledger row with `disposition: extension`, and the
collision gate asserts none of them appears in the recorded upstream surface.

### Usage — standalone, hosted (a ported file, unchanged)

```cpp
rclcpp::init(argc, argv);
auto node = std::make_shared<rclcpp::Node>("talker");
auto pub  = node->create_publisher<std_msgs::msg::String>("chatter", 10);
auto timer = node->create_wall_timer(std::chrono::seconds(1), [pub]{ /* ... */ });
rclcpp::spin(node);
rclcpp::shutdown();
```

### Usage — standalone, freestanding

```cpp
if (!rclcpp::init().ok()) return 1;

rclcpp::Node node("talker");          // no allocation; upstream's shape
if (!node.ok()) return 1;             // replaces upstream's throw

rclcpp::Publisher<std_msgs::msg::String> pub;   // inline storage
if (!node.create_publisher(pub, "chatter").ok()) return 1;

while (rclcpp::ok()) {
    pub.publish(msg);
    rclcpp::spin_once(100);
}
```

Every line is `rclcpp::`. Nothing names `nros::`.

### Usage — workspace component (the firmware recommendation)

Unchanged in structure; only the spellings move. No allocator, no derivation, no
vtable — the only one of the three shapes that compiles on every target we ship.

```cpp
class Talker {
    rclcpp::Publisher<std_msgs::msg::Int32> pub_;
    rclcpp::Timer                           timer_;
    int count_ = 0;

    void on_tick();

  public:
    rclcpp::Result configure(rclcpp::Node& node) {
        NROS_TRY(node.create_publisher(pub_, "chatter"));
        return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
    }
};
```

The generated entry constructs each component and calls `configure(node)` —
upstream's manual composition with the `main` generated instead of written.

### Usage — workspace component, hosted (a ported component, unchanged)

```cpp
class Talker : public rclcpp::Node {
  public:
    explicit Talker(const rclcpp::NodeOptions& opts) : Node("talker", opts) {
        pub_ = create_publisher<std_msgs::msg::Int32>("chatter", 10);
    }
};
```

Same type. Deriving needs the hosted constructor, so it is hosted-only; on a
freestanding target the answer is the shape above, which needs no derivation.

### Outstanding loudness items this proposal creates

Both are cases where a signature changed and the compiler does NOT force the
edit, which the governing principle requires be made loud by other means:

1. `Result` needs `[[nodiscard]]` (`NROS_NODISCARD` on C++14 targets) so
   `rclcpp::init(argc, argv);` as a bare statement warns. `grep -n nodiscard`
   on `result.hpp` returns nothing today.
2. `Node::ok()` needs a story for the standalone `main` that never calls it.
   The generated entry already checks; a hand-written `main` does not, and a
   node that failed to create currently proceeds silently.

## The C++ std surface is REQUESTED, not discovered (2026-09-08)

The compile-or-conform rule needs a surface on which an upstream rclcpp file
compiles unmodified. This section settles what that surface is and who turns it
on, because the answer today is "the toolchain does, behind the consumer's
back".

Six macros gate it. Each is defined by a block repeated 29 times across 13
headers whose `#elif` arm reads `__has_include(<memory>)` — so `NROS_CPP_STD` is
an opt-in, and the same macros are ALSO discovered from the include path. On any
hosted compiler they are on whether or not the consumer asked, and
`rclcpp::Node` is guarded on the discovered ones (`nros.hpp:447`).

**Measured, nothing but ported code wants that surface.** The hosted
`shared_ptr`-returning `create_*` family has ONE user in the tree and it is a
compile test; the freestanding out-ref family has 27 call sites in `examples/`
alone. Every consumer of the hosted `rclcpp::Node` class is a porting template,
a compile test, or the `diagnostic_updater` shim. No application code, on any
platform.

**So the axis is not host versus embedded.** It is *ported rclcpp code* versus
*code written for nano-ros*, and every real nano-ros program — native
included — is already on the freestanding side. `NROS_CPP_HAS_SHARED_PTR` asks
"can this toolchain reach `<memory>`" while the question that needs answering is
"does this consumer want the porting surface". Two questions, one name: the
collapse CLAUDE.md records for `native`/`posix`/`linux`, one layer over.

**Decision: the `#elif __has_include` arm is deleted.** The std surface is
reachable only through `NROS_CPP_STD`. It is a declared porting surface — the
thing you enable to compile someone else's file — and never something a build
acquires by being hosted. phase-438 carries it.

This is the same argument phase-359 made for Rust, where `std` was found to be a
second implementation of the platform layer rather than a convenience over it.
The C++ case is weaker in one way and stronger in another: the surface is
genuinely needed (it is what makes porting work, so it is not deleted), but it
was never even chosen.

### What follows for the node merge

Three of phase-427's hardest problems are consequences of the discovery, not of
the merge:

* `rclcpp::Node` does not exist freestanding, because its guard is the
  discovered macros. That is the single reason the type that must become the
  one node type is absent from half its targets.
* Hosted-only MEMBERS exist — `owned_entities_`, the `enable_shared_from_this`
  base — because the hosted API is a different SHAPE of the same class rather
  than additive methods over a fixed layout. Once the surface is opt-in and the
  layout unconditional, the 0135/0460 hazard has nothing to bite on.
* `shared_from_this` shrinks from a base class that changes everyone's layout to
  a hosted-only method over a `weak_ptr` behind `hosted_`. The decision recorded
  above stands; its cost is the same, but it stops being a layout question.

phase-427 W1 therefore moves to phase-438 W4, and phase-438 lands before both
phase-426 and phase-427.

## Conciliating phase-426 and phase-427 (2026-09-07)

The two phases that implement this RFC each declared the other out of scope, in
matching words:

* phase-427: *"Parameters — phase-426. W4 touches the facades but does not
  depend on it."*
* phase-426: *"Making `nros::ComponentNode` disappear; that is the node-type
  merge, which W4 touches but does not depend on."*

Measured, the symmetry is false in one direction, and several claims in this
RFC did not survive contact with the code. This section records the owner's
governing reason, the corrected dependency, and six corrections.

### The governing reason: one loading model, therefore one node type

This RFC previously argued for one node type from upstream's shape —
*"`rclcpp_components` composes plain `rclcpp::Node`s, so there is no second
node type to mirror."* The owner's reason is stronger and is the one to lead
with:

> **In nano-ros a node is always linked into the final image. There is no
> distinction like ROS 2 on Linux. We still maintain the shape from ROS 2.**

`ComponentNode` versus `Node` encodes a LOADING-MODEL distinction — a component
`dlopen`ed into a container at runtime versus a node with its own `main`. That
distinction does not exist here; there is one loading model, static linking.
A type whose only job is to mark which loading path an object arrived by has no
referent in this system.

Two consequences follow immediately. The `NodeHandle` constructor exists because
the entry supplies the executor handle rather than the node reaching for a
global — but if everything is linked into one image there is always a global
executor, so this is a constructor overload, not a type. And "keep ROS 2's
shape" means keep DERIVATION, which the next correction shows we can.

### Correction 1 — deriving is NOT hosted-only, and the RFC said it was

This RFC states, of the ported hosted component: *"Deriving needs the hosted
constructor, so it is hosted-only; on a freestanding target the answer is the
shape above, which needs no derivation."* It also calls the non-derived
`configure(Node&)` shape *"the only one of the three shapes that compiles on
every target we ship."*

Both are false. Measured — compiled to an object file, not parsed:

```cpp
class Talker : public nros::Node {
  public:
    nros::Timer timer_;
    int         count_ = 0;
    nros::Result start() { return create_wall_timer(timer_, 1000, &Talker::tick, this); }
    static void tick(void* ctx) { static_cast<Talker*>(ctx)->count_++; }
};
Talker g_talker;
static_assert(!__is_polymorphic(Talker), "no vtable is introduced by deriving");
static_assert(!__is_polymorphic(nros::Node), "the base has no vtable");
```

```
c++ -c -std=c++14 -fno-exceptions -fno-rtti -ffreestanding -nostdinc++ \
    -isystem packages/boards/nros-board-threadx-qemu-riscv64/cxx-compat ...
COMPILE EXIT=0
```

Derivation costs nothing on a freestanding target. It needs no allocator, no
exceptions, and no vtable — `nros::Node` has no virtual member, so deriving
introduces none. What was actually hosted-only was never derivation: it was
`rclcpp::Node` ITSELF, because that entire class sits inside
`#if defined(NROS_CPP_HAS_SHARED_PTR) && ...` (`nros.hpp:447`), plus
`ComponentNode`'s virtual destructor. Neither is a property of deriving.

This matters because the build system already agrees with the owner and not
with this RFC. `nros_components_register_node` defaults to `SHAPE rclcpp` and
its comment calls the `configure(Node&)` shape "legacy"
(`cmake/NanoRosVerbs.cmake:436-438`), while this RFC calls `configure(Node&)`
"the firmware recommendation". After the merge the cmake default is the correct
one on every target, and the two documents stop disagreeing.

`configure(Node&)` remains available and is still the right shape when one
component drives several nodes or holds no identity of its own. It is an
option, not the fallback for targets that cannot derive — there are none.

### Correction 2 — the dependency is real, one-directional, and it is 427 that has it

phase-427's W1 rehomes hosted-only state behind `void* hosted_`. The parameter
store is not hosted-only, so `hosted_` does not move it, and one type means one
store size:

| | today | per |
| --- | ---: | --- |
| `::nros::Node` | 192 B | node |
| `rclcpp::Node` — `ParameterServer<16>` | 3 752 B | node, always, even declaring nothing |
| `::nros::ComponentNode` — `ParameterServer<256,8,4096>` | 55 776 B | node, always, even declaring nothing |

Every naive merge is a defect: ComponentNode's store makes every freestanding
node ~55 KB, and `rclcpp::Node`'s silently cuts ComponentNode's derivers from
256 declared parameters to 16. The third door is this RFC's own decision 3 —
forward to the Rust store — and that needs the writer phase-426 owns.

So **phase-426 W1–W4 precede phase-427 W1**, and phase-427's "does not depend on
it" is withdrawn. phase-426's mirror-image claim stands: keying the Rust table,
adding the writer and registering services per node are Rust and RMW work that
does not care how many C++ node types exist.

**Re-cut the boundary rather than interleave the phases.** phase-426 owns the
parameter SSoT end to end, INCLUDING the deletion of both C++ stores, which is
already its W4. phase-427 then begins from a node that has no parameter store,
and its W1 becomes what it was written to be: a question about hosted-only
state and nothing else. Each phase keeps an acceptance it can check alone.

### Correction 3 — the memory consequence of decision 3, which neither phase states

"Parameters are the Rust store" is right on SSoT grounds, and it is not free.
The Rust table is a leaked heap allocation sized by its widest variant
(`executor/spin.rs:7895`):

> *285,184 bytes at the default `MAX_PARAMETERS=32` and 2,281,472 at 256 —
> `ParameterValue` is sized by its `StringArray` variant, so every slot costs
> ~8.5 KiB regardless of what it holds.*

Against a per-node C++ store that is unconditional, the trade is:

| image | today | after |
| --- | ---: | ---: |
| any number of nodes, no parameters declared | 3.5–55 KB per node | **0** (the table is lazy) |
| one node using parameters | 55 KB | **285 KB** |
| five nodes using parameters | 277 KB | 285 KB |

So the merge is a large win for images that declare nothing, roughly neutral
around five nodes, and a REGRESSION for a small one-node image that uses
parameters. Issue 0756 already records 256 slots overrunning the Zephyr thread
stack and hanging boot with no output. The ~8.5 KiB per slot is the defect
underneath and belongs to phase-382, not here — but both phases must carry the
number rather than meet it on a Zephyr image.

### Correction 4 — RFC-0047's "several named nodes" does not exist

This RFC and phase-427 W4 both preserve *"RFC-0047's one component, several
named nodes"* as a documented ours-only capability. Measured, there is no such
capability to preserve:

* `ComponentNode` holds exactly one `Node node_` and takes exactly one `name`.
  There is no second identity anywhere in the class.
* RFC-0047 is *"Unified sched-context binding via callback groups"*; its
  "sub-node splitting" means per-callback-group tiering INSIDE one node, and
  the word "node" is singular throughout.
* Both subnode packages are one node with two groups —
  `ComponentNode(h, "sub_node")` once, then two `create_callback_group` calls.
* `entity_inventory.rs:604` derives `NROS_EXECUTOR_MAX_NODES` from the rule
  *"a `ComponentNode` constructor is one `Node::create` is one node NAME"*. If
  a component could own several identities that knob would be under-derived on
  every image using one.

The claim is deleted, here and in phase-427. W4's acceptance criterion "the
RFC-0047 several-named-nodes capability survives" was satisfiable only
vacuously, which is the shape of an acceptance nobody can fail.

### Correction 5 — `ok()` is two different predicates, and the merge narrows one

This RFC gives the merged type `bool ok()` "replacing upstream's throw", and
says *"the generated entry checks `ok()` and halts naming the node"*. The entry
does more than that (`packs/entry/cpp/node_body.jinja:29-32`): it reads
`ok()`, `error_what()` AND `error_code()`, and returns the code.

The two `ok()`s answer different questions. `ComponentNode::ok()` is STICKY: it
is false if the node failed to create **or** if any of ten later `create_*` /
`declare_parameter` calls failed, through `set_error`'s first-failure-wins flag
with the issue-#230 acquire/release ordering. This RFC's `ok()` is construction
only. Merging them without saying so narrows the boot halt from "this node did
not finish registering" to "this node was not created".

`error_what()`, `error_code()` and `set_error()` have no home in the proposed
layout or API list. The merged node keeps all three and the sticky predicate,
and `nros::detail::report_component_failure` stays where it is as a free
function — the generated entry calls it directly and does not need the class.

### Correction 6 — the out-ref rationale is true of one entity kind, not the family

Both documents justify the whole out-ref `create_*` family with *"the arena
stores `&entity` as its dispatch context and has NO unregister"*. That is true
of subscriptions (`subscription.hpp:723,759,795,845` pass `&out`) and false
elsewhere: `nros_cpp_timer_create` stores the CALLER's context and returns an
id, and the member-pointer subscription path registers with `ctx = self` and
produces no C++ object at all.

Two consequences. `ComponentNode`'s `timers_` pool exists only because its
storage-less overload has no out-parameter — with the out-ref form the caller
owns the cell and the pool disappears entirely, along with its
`NROS_COMPONENT_MAX_TIMERS` knob and the issue-1167 `#error` floor. And the
member-pointer fold needs FOUR signatures, not the one this RFC gives; the
subscription forms take no out-ref because there is nothing to put in it:

```cpp
template <class C, void (C::*M)()>
Result create_wall_timer(Timer& out, uint64_t period_ms, C* self) noexcept;

template <class C, void (C::*M)()>
Result create_timer_in(const CallbackGroup&, Timer& out, uint64_t, C* self) noexcept;

template <typename Msg, class C, void (C::*M)(const Msg&)>
Result create_subscription(const char* topic, C* self, const QoS& = QoS::default_profile()) noexcept;

template <typename Msg, class C, void (C::*M)(const Msg&)>
Result create_subscription_in(const CallbackGroup&, const char* topic, C* self,
                              const QoS& = QoS::default_profile()) noexcept;
```

### Correction 7 — "no vtable budget" was never measured, and the real cost is not bytes

This RFC declines upstream's `TimerBase`/`WallTimer` hierarchy partly on "a
vtable we have no budget for". Searched: no measurement of a C++ vtable cost
exists in any RFC or phase document. The only vtable figures in `docs/` are for
the RMW's C function-pointer struct, which is a different thing.

Measured now, one virtual destructor plus one virtual method on a two-int base
with a derived override, `-Os -ffreestanding -fno-exceptions -fno-rtti`:

| target | object `.bss` | vtable | extra `.text` |
| --- | --- | --- | --- |
| arm-none-eabi cortex-m4/thumb | 12 → **16** (+4) | 20 B | 26 B |
| riscv64 rv64gc | 12 → **24** (+8) | 40 B | 22 B |

Both freestanding lanes compile virtual functions without complaint. So the
budget argument is not the reason, and stating it as one made an architectural
decision look like a resource decision.

**The reason that IS real, and is better than the one given:**

```
$ arm-none-eabi-nm -u o_NONVIRT.o     # (empty)
$ arm-none-eabi-nm -u o_VIRT.o
         U memset
         U _ZdlPvj                     # operator delete(void*, unsigned int)
```

A virtual destructor forces emission of the D0 *deleting* destructor, which
leaves an undefined reference to `operator delete` — on a target where nothing
ever deletes a node, and where the allocator may not exist at all. That is a
link-time consequence of a keyword, not a byte count, and it is the argument to
keep.

`ComponentNode`'s vptr is 8 bytes of its 55,776 — 0.014 %. The size was never
the problem there either.

### Correction 8 — the virtual destructor is paying for a base pointer nobody creates

Two premises behind keeping it are false:

* **The generated entry never holds a `ComponentNode*`.** It declares the
  storage with the DERIVED type and placement-news the derived type into it —
  verified against real generated output, where the word `ComponentNode` does
  not occur. Nothing is ever deleted: static arena, no destructor call.
* **The compat `main` is not a `ComponentNode` site.** It casts to
  `rclcpp::Node`, a different and non-polymorphic type. And that `main` is never
  actually emitted in this tree.

The only construct in the repo that produces a `ComponentNode*` base pointer is
the `NROS_COMPONENT(Class)` factory macro, which has zero users. A `shared_ptr`
keeps the derived deleter through both conversion spellings — verified by
running it, with a raw-`delete`-through-a-non-virtual-base control that does
skip the derived destructor, so the experiment discriminates.

Both real workspace derivations still compile with the base devirtualised under
`-Werror -Wnon-virtual-dtor -Wdelete-non-virtual-dtor`. The merged type is
non-polymorphic and nothing has to be given up for that.

One adjacent fact for whoever schedules the migration: **no embedded fixture
derives `ComponentNode` today.** The only `SHAPE rclcpp` packages are the two
`subnode_pkg`s, and every fixture consuming them is `platform = "linux"`. So the
freestanding derivation proved in correction 1 is currently a capability with no
in-tree consumer — which is an argument for adding a fixture in the same phase,
not for assuming it works.

### What correction 1 and correction 5 together mean for `hosted_`

Today a hosted and a freestanding TU cannot disagree about `rclcpp::Node`,
because it does not exist freestanding at all — both shims lack `<memory>`,
`<string>`, `<vector>` and `<functional>`. After the merge it exists everywhere,
and the disagreement becomes possible. Measured on a patched header that gates
the destructor's `virtual` on `NROS_CPP_STD`: `sizeof` 3752 versus 3760 and
`__is_polymorphic` flipping outright — the issue 0135/0460 hazard, live on the
axis px4 actually exercises.

`check-cpp-capability-layout` catches that mutation now, and only since
phase-427 W0 (issue 1204) gave it a freestanding arm. Its `hosted_only_reason`
entry for `rclcpp::Node` is standing in for exactly this measurement, and W1 is
what removes the entry.

### What the merge must not quietly break

`packages/api/nros-cpp/tests/compile/declared_qos_depth.cpp` and its
`_probe.cpp` sibling are GATES, not tests: the lane compiles the first clean,
requires the second to FAIL, and greps its diagnostic for
`declared_depth_agrees<1, 10>` and the topic name. Their whole value is that a
check which has never failed is not a check. phase-427 W4's acceptance — "zero
`ComponentNode` in the tree" — is satisfied by a migration that leaves both
files compiling and proving nothing. Migrating them while keeping them
FALSIFIABLE is the work item, and the lane's grep pattern moves in the same
commit.

Three smaller things that live in the doomed header and need explicit homes:
the Zephyr placement-`new` shim (`component_node.hpp:78-87`, cited as precedent
by `heap_sequence.hpp:26`), `check_declared_depth` (public API with no ledger
row, so the parity gate will not notice it vanish), and
`declare_parameter<std::vector<T>>`, which has no FFI to forward to and no work
item that owns it.

`NROS_COMPONENT(Class)` needs no home: it has zero invocations in the tree, and
the entry placement-news the class directly rather than calling its factory.

### One thing that becomes possible, and should not be taken

`adopt_launch_seed_`'s `if constexpr` chain is the ENTIRE C++17 requirement of
the C++ header set — measured, all 45 headers parse clean at
`-std=c++14 -pedantic-errors` on gcc 12.3 and clang 14 unless
`NROS_SYSTEM_PARAM_SERVICES` is defined, and then only those four lines fail.
That function dies in the merge, along with its twin on `rclcpp::Node`.

Do not drop the floor to C++14. `packages/api/nros-cpp/CMakeLists.txt:315`
gives the independent reason: 17 is what Humble's `rclcpp` exports, and under
the compile-or-conform rule declaring 14 to a consumer whose upstream declares
17 re-creates issue 1118 pointing the other way. The gain is that `cxx_std_17`
becomes a POLICY choice rather than a forced one — and
`check-cxx-standard-floor`'s docstring, which names `component_node.hpp`'s
`if constexpr` as the reason, must be rewritten in the same commit or the gate
will explain itself with a file that no longer exists.

## `shared_from_this` — the merged node cannot keep the base (2026-09-07)

The node-API proposal above gives the merged type this layout, with no base
class:

```cpp
nros_cpp_node_t handle_;  bool initialized_;  void* executor_handle_;
Clock clock_;             void* hosted_ = nullptr;
```

Today's `rclcpp::Node` does have one — `std::enable_shared_from_this<Node>`
(`nros.hpp:540`) — and it is load-bearing. Two in-tree templates use upstream's
own two-phase idiom to hand a node to `diagnostic_updater`:

```cpp
auto node = std::make_shared<SmokeNode>();
node->post_init();          // calls shared_from_this(); cannot be done in the ctor
```

(`examples/templates/rclcpp-compat-smoke/src/talker.cpp:75-76` and
`examples/templates/topic-state-monitor-port/src/topic_state_monitor.cpp:106`.)
Both are hand-written `main`s, so this is a live runtime path, not a
compile-only surface.

**The base cannot simply be gated.** A base that exists only where `<memory>`
does changes `sizeof` between two TUs of one image, and px4 sets
`-DNROS_CPP_STD` on one module of a larger image deliberately. A freestanding TU
holding a `rclcpp::Node` by value and passing `Node&` to a hosted TU would have
every member offset shifted — issues 0135 and 0460, exactly the hazard the
layout rule exists to prevent.

**Nor can it be equalised by mirroring.** Reserving two pointers' worth of
opaque storage freestanding, to match what `std::enable_shared_from_this` happens
to occupy, hand-mirrors a foreign type's layout. This repo has a rule and a gate
against that shape for FFI structs (`check-ffi-struct-mirrors`), for the reason
that the mirror drifts silently. A standard-library internal is a worse thing to
mirror than our own header, not a better one.

**Decision: the base goes.** `shared_from_this()` survives as a hosted-only
member over a `std::weak_ptr<Node>` held in `detail::NodeHosted`, populated by an
explicit `bind_shared(std::shared_ptr<Node>)` call. `std::make_shared<Derived>`
populates the real base and nothing else does, so without the base nothing can
populate it implicitly, and the explicit call is the only honest substitute.

**The cost, stated.** A ported file that calls `shared_from_this()` needs one
added line, and a file that does not add it gets a runtime refusal rather than a
compile error. That is weaker than clause 2's "mechanical edit" standard, and it
is the same class as `ok()` and as `init()`'s discarded `Result`: a difference
the compiler does not point at, admissible only because it is loud. Disposition
is `adopt-bounded`, and `shared_from_this()` on an unbound node must refuse
loudly rather than return an empty pointer for the caller to dereference.

The alternative — keep the base and accept that `rclcpp::Node` stays hosted-only
— was rejected because it abandons the merge: phase-427 W4's acceptance requires
one of the subnode packages to build for a freestanding target, which is the
test of whether the merged type fits at all.

## Review of the invented parts (2026-09-05)

Four items were listed as inventions. Reviewed against the governing principle,
**two of the four are not inventions at all** and one of my stated risks was
overstated. Evidence is the recorded upstream surface
(`docs/reference/api-surface/rclcpp.json`) and our own headers.

### 1. `Timer` — do NOT adopt upstream's hierarchy; DO adopt its names

> **This item was WITHDRAWN by "Timer, studied against RTOS semantics" below,
> and it was RIGHT (confirmed 2026-09-09).** Its conclusion — refuse the
> hierarchy, take the `TimerBase` NAME as a hosted alias — is what stands. The
> withdrawal weighed only what the name teaches OUR readers; the alias's real
> job is to keep one template text compiling against real rclcpp AND against
> nano-ros, which `just colcon-parity` measures and header-reading cannot see.
> See "The alias rule" in Part I.

Upstream has `TimerBase`, `WallTimer`, `GenericTimer`, `TimerCallbackType`,
`create_timer`, `create_wall_timer`. `TimerBase` is a POLYMORPHIC base;
`WallTimer`/`GenericTimer` are templates over clock and callback;
`create_wall_timer` returns `shared_ptr<WallTimer<...>>`, conventionally stored
as `rclcpp::TimerBase::SharedPtr`.

**Refactoring ours into that hierarchy is refused by clause 1.** A polymorphic
base puts a vtable in every image that holds a timer, on targets chosen for
having no allocator and a fixed arena. The benefit would be that a ported file
could DERIVE from `TimerBase`, which is rare; the cost is paid by every image.

**But the common ported line is not derivation — it is a member declaration:**

```cpp
rclcpp::TimerBase::SharedPtr timer_;      // idiomatic upstream member
```

That is a name, not a hierarchy, and clause 2 says port it. So on hosted
targets:

```cpp
namespace rclcpp {
using TimerBase = ::rclcpp::Timer;        // [2] the ported name
}                                          //     Timer gains a SharedPtr typedef
```

A ported file's member declaration then compiles unchanged, and a file that
tries to DERIVE from `TimerBase` fails to compile — mechanical, and honest,
because we do not have the hierarchy.

**Net: `Timer` stops being a pure invention.** `rclcpp::Timer` remains the
freestanding spelling and the thing that actually exists; `TimerBase` is a
ported alias. `WallTimer`/`GenericTimer` stay absent — they are templates over a
clock type we do not have, and aliasing them would claim a genericity we cannot
deliver.

### 2. `spin_once` — keep it, and my collision risk was overstated

**Upstream's verb is already ported.** `rclcpp::spin_some(node)` exists here
(`nros.hpp:988`) with upstream's semantics — drain what is ready, no wait — as a
0-timeout `spin_once`. So the question is not "should we adopt `spin_some`"; we
already did.

`spin_once(timeout_ms) -> Result` is a *blocking wait with a budget*, which
rclcpp has no verb for. That is a real capability on an RTOS: a task that must
not spin-poll needs to sleep until work arrives or the budget expires.

**Correcting myself on the risk.** I flagged `rclpy.spin_once` as a live
collision hazard. It is weaker than I said: rclpy's signature is
`spin_once(node, timeout_sec=None)`, so a user carrying that habit writes
`spin_once(node, 0.1)` in C++ and gets **no matching overload** — mechanical,
not silent. The two names live in different languages, and the C++ compiler
separates them for us. The ledger row should record the rclpy sibling for the
reader's benefit; it is not an argument for renaming.

**Keep `rclcpp::spin_once`.** It stays a genuine invention, with the collision
gate watching in case rclcpp ever adds one.

### 3. `Node::init` / `Node::ok` — and the error-channel rule they exposed

Both stay, and neither is exotic: `ok()` is a state query and matches
`rclcpp::ok()`'s own spelling; `init()` is the checked construction form a
`-fno-exceptions` target needs in place of a throwing constructor.

The review's real finding is that we have **two error channels and no stated
rule** for choosing between them. `Result` (no value) and `Expected<T>` (value
or error) both exist in `result.hpp`, and — measured, not assumed — `result.hpp`
carries **zero capability gates** and parses under the ThreadX `-nostdinc++`
shim, so both are available on every target.

**The rule, stated:**

| the operation | channel |
| --- | --- |
| ours-only, can fail, produces nothing | `Result` |
| ours-only, can fail, produces a value | `ResultOf<T>` (was `Expected<T>`) |
| a state query that cannot fail | `bool` |
| **a PORTED API** | **upstream's channel, even when that is `bool`** |

The last row is clause 2 outranking local consistency, and it is load-bearing:
`rclcpp::Node::get_parameter` returns `bool` upstream, so ours does too. A
tidier all-`Expected` surface would be a preference recorded as a divergence,
which RFC-0036 forbids.

**What this made outstanding — CLOSED by phase-427 W8 (2026-09-07):** both
halves now carry `NROS_NODISCARD`, and the `cpp` lane compiles a TU that
discards each of them and requires the build to fail.

One correction to the sentence that motivated it. `rclcpp::init(argc, argv);`
as a bare statement is NOT the case the attribute catches, because
`rclcpp::init` returns `void` here — deliberately, by the ported-channel rule
two paragraphs up. The case is the WIDENING one layer down: `nros::init()`
returns a `Result` where `rclcpp::init()` returns nothing, and it is a call to
the widened form whose discarded value nothing would otherwise point at. The
attribute found two live instances of exactly that on its first run:
`rclcpp::shutdown()` was discarding `nros::shutdown()`'s `Result` and answering
`true` unconditionally, and the two ported-file probes drop `publish()`'s
result the way an upstream file does, which is the porting cost this rule
exists to charge rather than hide.

### 4. The out-ref `create_*` family — forced by the arena, not a style

Not an invention: it shares upstream's NAME and changes the signature, which is
clause 2.

And the signature is not a preference. **The executor arena stores `&entity` as
its dispatch context and has no unregister path** (`nros.hpp:625-641`,
`:806-812`). So the entity's address must be stable for the node's lifetime and
owned by someone who outlives it. A value-returning
`Expected<Publisher<M>> create_publisher(...)` would MOVE the object out of the
function, changing the address the arena has already recorded — a dangling
dispatch context, silently, on the first callback.

That is why the hosted `shared_ptr` overload also keeps `owned_entities_`: the
returned pointer is a co-owner, never the only owner. The allocation is
inherent to the arena contract, not to the spelling — and the out-ref form is
the shape that needs no allocation at all.

`Expected<T>::make()` already exists as the hosted-friendly value-returning
form (`result.hpp:190`), and its own doc comment says the out-param remains the
canonical zero-cost API. That stands.

### Revised inventory

| item | verdict |
| --- | --- |
| `rclcpp::Timer` | ported name `TimerBase` added (hosted alias); `Timer` remains ours as the concrete type. **This row stands** — the withdrawal below was itself withdrawn 2026-09-09 |
| `rclcpp::spin_once` | **invention, kept** — upstream has no blocking-with-budget verb; `spin_some` already ported |
| `Node::init` / `Node::ok` | kept; `ok()` matches upstream's spelling, `init()` is the no-exception channel |
| out-ref `create_*` | **not an invention** — ported name, changed signature, forced by the arena |

Two of four were mislabelled. What remains genuinely invented is `spin_once`
and `Node::init`, plus `Timer` as a concrete type behind a ported alias.

## Timer, studied against RTOS semantics (2026-09-05) — no hierarchy; the `TimerBase` ALIAS is restored (2026-09-09)

**Read the amendment at the end of this section first.** The study of the
HIERARCHY stands in full and is the reason `Timer` is flat. Its conclusion about
the NAME — that we should not take `TimerBase` at all — is overturned: the alias
is load-bearing for `colcon-parity`, and deleting it empties the set of timer
spellings that compile in both libraries.

The objection is right and it kills the alias: **`TimerBase` is a name that
promises children.** A reader who finds it expects `WallTimer` and
`GenericTimer` beside it, and aliasing a concrete class to that name sells a
taxonomy we do not have. Either port the hierarchy or do not take the name.

So: can the hierarchy be ported? Measured, not assumed.

### What upstream's three types are FOR

* `TimerBase` — a non-template base so the executor can hold a
  `std::vector<TimerBase::WeakPtr>` without knowing the callback type. Its job
  is **type erasure for the executor**.
* `GenericTimer<FunctorT, ClockT>` — stores the callback functor BY VALUE and is
  parameterised on the **clock**.
* `WallTimer<FunctorT>` — `GenericTimer` pinned to the steady clock.

The hierarchy exists because upstream's executor needs a type-erased handle
while the timer wants the functor inline.

### Why neither reason survives here

**Type erasure is already done, one layer down.** Our `Timer` is not an owner —
it is a HANDLE: `{void* executor_; size_t handle_id_; bool initialized_;}`. The
callback lives in the executor arena as a raw `void(*)(void*)` plus a `void*`
context, and dispatch goes through that pointer. The executor never needs a
base class because it never holds a C++ timer object at all. A `TimerBase` here
would be a vtable added for API shape with **no dispatch that uses it** —
clause 1 refuses it, and unlike the usual vtable argument this one buys nothing
even in principle.

**The clock parameter corresponds to a capability we do not have, and that is
the real finding.** Our timers are scheduled by the executor against
`nros_platform_clock_ns` — one **monotonic** counter, with
`ExecutorConfig::clock_us` as the only override (`executor/spin.rs:8746`).
`create_timer` takes `(Timer&, uint64_t period_ms, callback, ctx)`: **no clock
argument anywhere.** `nros::Clock` with its `NROS_CLOCK_{SYSTEM,ROS,STEADY}_TIME`
types is a time-READING API; it does not drive timer scheduling.

So upstream's `WallTimer` vs `GenericTimer<Clock>` distinction is exactly the
distinction between a steady-clock timer and a **ROS-time / simulated-time**
timer — and we only have the first. `GenericTimer` cannot be ported because the
thing it is generic over does not exist here. Porting the NAME while having one
clock would be the compile-and-differ this RFC forbids: a ported file passing a
`Clock` would either fail to compile (fine, but then the name bought nothing) or
compile and silently ignore the clock (forbidden).

### Should we invent our own hierarchy instead?

No, and for the same reason the alias was wrong. **A hierarchy with one leaf is
a promise with no content.** The two axes that might have been children are
already expressed better:

* one-shot vs repeating → `create_timer_oneshot` and `set_oneshot`/
  `set_repeating` **methods**, because a timer changes between them at runtime
  and a type cannot;
* clock source → does not exist yet.

Inventing `nros::TimerBase` with a single `WallTimer` under it would reproduce
the misleading promise we just refused to import, and charge a vtable for it.

**Decision: `rclcpp::Timer` stays flat and stays ours.** No `WallTimer`, no
`GenericTimer`, and no hierarchy. (LANDED 2026-09-08, phase-430 W7 — the tree had
a `TimerBase` HIERARCHY for a phase in spite of this paragraph; see §"AMENDED
2026-09-08" for that ruling and its argument.) This paragraph also said "no
`TimerBase`" at all, and required a ported `rclcpp::TimerBase::SharedPtr timer_;`
to be RENAMED to `rclcpp::Timer timer_;`. **That half is overturned 2026-09-09** —
the rename is mechanical here and impossible upstream, so it is exactly the edit
that empties the two-library intersection. `TimerBase` returns as a hosted alias
onto the flat `Timer`; see the amendment below.

The verb is already accurate and already ported: `create_wall_timer` says
*wall*, and wall is what we schedule on.

### AMENDED 2026-09-05 — ROS time is coming, and the conclusion survives

The premise below ("the clock it is generic over does not exist here") is
scheduled to change: **phase-430 brings ROS time**, for rosbag replay. The
reasoning above was correct and one of its two legs is being removed.

**The conclusion does not change.** The clock becomes a RUNTIME FIELD on the
flat `Timer` plus a second verb (`create_timer(clock, period, cb)` beside
`create_wall_timer`), not a type parameter and not a hierarchy — because the
other leg is untouched: the executor dispatches through a raw function pointer
in the arena, so it never needs a type-erased base, and a `TimerBase` would
still be a vtable no dispatch uses.

Upstream distinguishes the two cases by TYPE *and* by VERB. The type
distinction stays unportable; **the verb distinction is portable and it is what
ported code writes**, so that is what we take.

### AMENDED 2026-09-09 — the ALIAS is restored; only the HIERARCHY stays refused

This section withdrew `TimerBase` on the argument that "`TimerBase` is a name
that promises children… Either port the hierarchy or do not take the name."
**The first half stands and the second half is wrong**, and §"Review of the
invented parts" item 1 — which proposed exactly this alias and was withdrawn
here — was right.

What the withdrawal did not weigh is that the alias is not for OUR readers. It
is what lets one template text compile against real rclcpp AND against
nano-ros: upstream has `TimerBase` and no `Timer`, we have `Timer` and no
`TimerBase`, so with the alias deleted no spelling of a timer member satisfies
both and `just colcon-parity` has nothing to build. See "The alias rule" in Part
I; this is the case that produced it.

So, precisely:

* **`rclcpp::Timer` stays flat and stays the concrete type.** Everything this
  section measured about the hierarchy holds: our `Timer` is a HANDLE, dispatch
  is a raw function pointer in the arena, so a base class would be a vtable no
  dispatch uses; and `GenericTimer`'s clock parameter is a capability we do not
  have. No `WallTimer`, no `GenericTimer`, no invented one-leaf hierarchy.
* **`rclcpp::TimerBase` is restored as a hosted ALIAS onto it, and is NOT
  deprecated.** A ported `rclcpp::TimerBase::SharedPtr timer_;` compiles; a file
  that tries to DERIVE from it fails to compile, which is honest — we do not
  have the hierarchy, and the failure is at the line.
* The name promising children is a real cost, and it is paid down by the doc
  comment on the alias saying what it is, not by deleting the only spelling that
  works in both libraries.

### The condition under which this is revisited

If ROS-time or simulated-time timers are ever implemented — a bag-driven or
sim-driven clock feeding the executor's scheduler — the port is
`create_timer(clock, period, callback)` plus a clock field on `Timer`. **Still
flat.** The hierarchy would only become right if the executor needed a
type-erased base, and it will not, because dispatch stays a raw function
pointer in the arena.

## AMENDED 2026-09-08 — what the merge measured, and three things this RFC got wrong

phase-427 landed W1, W2, W3 and W5 (the node-type merge itself), W6 and W8's
loudness half, and phase-430's W6 and W7. Three claims above are now false or
unbuildable, and the corrections are measurements rather than preferences.

### 1. The `TimerBase` question is RULED, and the ruling is DELETE

§"Timer, studied against RTOS semantics" decided the timer stays flat. The TREE
DISAGREED WITH IT for a phase: phase-417 W1.a had landed `class TimerBase` with
a virtual destructor and `detail::WallTimer : TimerBase` under it — precisely
the hierarchy this document refused, added by the same campaign, and neither
document noticed. phase-430 W7 asked for a ruling either way. **It is deleted**
(phase-427, 2026-09-08), argued from the executor's dispatch rather than from
taste:

* **The vtable had no caller.** The only virtual member was `~TimerBase()`, and
  no virtual call through a `TimerBase*` existed anywhere in the tree — none
  can, because the executor's callback slot is `nros_cpp_timer_callback_t`, a
  raw `void(*)(void*)`, and dispatch goes through the STATIC
  `detail::WallTimer::trampoline`. Even the destructor's virtuality was dead:
  the cell is built with `std::make_shared<detail::WallTimer>()`, so the control
  block already records the concrete deleter and destruction through a base
  pointer was correct without it.
* **The flat shape is strictly better as a handle.** `create_wall_timer` now
  returns `std::shared_ptr<nros::Timer>` ALIASED onto the private cell — the
  shape `create_subscription` has always used — so the returned type is the type
  that exists and the cell stays an implementation detail.
* **The NAME is KEPT, and §"Timer, studied against RTOS semantics" is WRONG
  about it — measured by a gate, not argued.** That section says an alias
  "sells a taxonomy we do not have" and concludes "either port the hierarchy or
  do not take the name". Deleting the name turned `colcon-parity` red:

  ```
  rclcpp::Timer -> 'Timer' in namespace 'rclcpp' does not name a type;
                   did you mean 'Time'?          [REAL ROS 2, humble]
  ```

  `just colcon-parity` builds `examples/templates/local-msg-package` against a
  REAL `/opt/ros/<distro>` install, and `cpp-port-minimal-publisher` is vendored
  UNMODIFIED to demonstrate upstream source compiling here. Both declare a timer
  member, and upstream has no `rclcpp::Timer`. **So the alias is what keeps the
  intersection of "compiles under real ROS 2" and "compiles under nano-ros"
  non-empty for any ported node holding a timer** — which is most of them.
  Without it, that intersection is empty and the campaign's own flagship
  property is false.

  This vindicates §"Review of the invented parts" item 1, which proposed exactly
  this alias and which the later section withdrew. The withdrawal's reasoning
  was about ADVERTISING and is answered by documentation plus a probe
  (`timer_base_is_a_name_not_a_hierarchy.cpp` pins `is_same<TimerBase, Timer>`,
  `!is_polymorphic`, and the continued ABSENCE of `WallTimer`/`GenericTimer`
  with a self-tested detector). It is not deprecated: a deprecation would
  promise a removal that would break the dual-compile property on purpose.

  **The general lesson, which is bigger than the timer.** For a name UPSTREAM
  HAS and we lack, the ported alias is not a courtesy — it is the only thing
  that keeps a file compilable BOTH ways. "Do not take the name" is available
  only where upstream does not have it either. Any future decision to withhold
  an upstream name should be checked against `colcon-parity` before it lands,
  because that gate is the one place the two-directional requirement is
  measured.

  The cost measurement itself was wrong twice before this: first "zero non-test
  call sites" (a `grep --include` glob the shell expanded to nothing), then "one
  deprecated release". Both are recorded in phase-427 rather than tidied away.

§"Review of the invented parts" item 1 — which proposed `TimerBase` as a ported
ALIAS — is superseded by this and by the section that already withdrew it. It
is left in place as the record of the argument.

`rclcpp::Timer` is UNCONDITIONAL, unlike the `TimerBase` it replaces: it is
`nros::Timer`, which needs no `<memory>`, so a freestanding target gets the ROS
2 timer name too. Only the nested `SharedPtr` aliases were ever hosted-only.

### 2. The namespace direction is INVERTED, and the reason is the parity tool

§"Type and namespace" says `rclcpp::Node` is the class and `nros::Node` the
migration alias. **It is the other way round in the tree, and the blocker is
`scripts/api-parity.py`'s own configuration.**

The extractor reads the NATIVE C++ surface with namespace root `{"nros"}` and
the PORTED surface with `{"rclcpp", "rclcpp_action", "rclcpp_lifecycle"}`, and
`--check` gates only the native bucket. Defining the class in `rclcpp` empties
`nros::Node::*` from the native surface, so roughly sixty members re-bucket to
`theirs-only` with no ledger row and the gate goes red. That is a namespace
question answered by a measurement tool's roots, and the fix — moving the roots
— re-buckets the WHOLE ported surface at once, which is phase-428's sweep and
not one type's business.

This costs nothing observable: both spellings name ONE class,
`std::is_same<rclcpp::Node, nros::Node>::value` is asserted by a compile probe,
and the `rclcpp::Node` alias is UNCONDITIONAL where the shim class was declared
only under four capability probes. It is also the CONSISTENT direction —
`rclcpp::Publisher`, `rclcpp::QoS`, `rclcpp::Timer` and `rclcpp::Clock` are all
aliases of `nros::` definitions already, so `Node` being the one exception would
have been arbitrary.

**Consequence for the "Replace" step.** phase-427 W7 ("`nros::Node` deprecated
with `NROS_DEPRECATED_MSG`") cannot be done in this direction: a deprecation
attaches to the alias, and the alias is the name a user is supposed to write.
The migration macro ships (`NROS_CPP_DEPRECATED_MSG`, `result.hpp`) and is used
on the one ours-only name phase-427 did retire, `nros::bind_timer`. The `Node`
half waits on the flip — and whoever lands it must migrate every in-tree
`nros::Node` site in the same commit (191 lines across `examples/` and
`packages/` C++ sources, 218 counting codegen templates, goldens and prose),
because a bare `[[deprecated]]` on an alias warns at every one of them at once,
which is the flag day this document's two-step exists to avoid.

### 3. The one-name half of the settled shape is NOT EXPRESSIBLE in C++

§"The error channel, settled" specified one template and one name: `Expected<T>`
renamed to `Result<T>`, today's `Result` becoming `Result<void>` with `Result`
as the alias. The language does not allow the last part, W8 measured it, and
**that section now carries the correction inline** — one template
**`ResultOf<T>`**, the value-less case as its `void` specialization, `Result` as
the alias for that specialization, and `Expected<T>` deprecated for one release.
Read it there; it has the three ill-formed routes and the compiler messages.

What is recorded HERE is the part that section does not: the acceptance text
named the wrong probe. `rclcpp::init(argc, argv)` returns `void` here, as
upstream's does — a PORTED api keeps upstream's channel, which is that section's
own last row — so a TU discarding it compiles clean and the gate would have been
vacuous. The discarding call site the loudness item was about is `nros::init()`,
which returns a `Result` where `rclcpp::init()` returns nothing, and the WIDENING
is the one signature change the compiler does not otherwise point at.
`result_nodiscard_probe.cpp` discards both halves of the channel and the lane
requires two `unused-result` diagnostics.

That probe earned itself on its first run: `rclcpp::shutdown()` was discarding
`nros::shutdown()`'s `Result` and answering `true` unconditionally — a fini that
failed reported success, which is exactly the silence the attribute exists to
break.


### 4. `std::enable_shared_from_this` is a capability-dependent LAYOUT

Not previously noticed anywhere. The shim `rclcpp::Node` derived from it, which
was invisible while the whole class lived inside a capability `#if` — there was
nothing to compare it against. On the merged type it is a hosted-only BASE with
a `std::weak_ptr` member, i.e. 16 bytes of layout behind a probe: the exact
defect `check-cpp-capability-layout` exists to catch, and the one the deleted
`timers_` member already shipped once.

So the base is gone and `shared_from_this()` is a METHOD returning a pointer
that aliases `this` with an EMPTY owner. ADOPT-BOUNDED and ledgered
(`cpp:Node::shared_from_this`, `divergence`): it observes the node without
extending its lifetime, where upstream's shares ownership, and it never throws
where upstream raises `bad_weak_ptr`. In this API a node is constructed by the
generated entry or by `main` and outlives everything it is handed to, so the two
behave the same — but a caller who stores the pointer past the node's scope gets
a dangling one.

The general rule this generalises to, worth stating because it will recur: **a
BASE CLASS is a member for layout purposes.** Any `#if`-gated base is the same
defect as an `#if`-gated field, and the capability-layout gate measures `sizeof`
precisely so the distinction never has to be argued.

### 5. RFC-0047's "one component, several named nodes" DOES NOT EXIST

Decision 2 above lists it as `ComponentNode`'s second distinguishing feature and
as a divergence to preserve. Measured: `ComponentNode` owns exactly ONE
`nros::Node`, created once in its constructor, and
`packages/cli/nros-cli-core/src/entity_inventory.rs` states the invariant in so
many words — "a `ComponentNode` constructor is one `Node::create` is one node
NAME". `docs/design/0047-unified-sched-context-binding.md` says nothing about
several named nodes either; the phrase originates here.

What the two subnode packages actually exercise is **one node with several NAMED
CALLBACK GROUPS bound to different sched contexts** — `create_callback_group`
plus `create_timer_in` / `create_subscription_in` — which is RFC-0047's real
subject, and which the merged node already carries. There is no
several-named-nodes capability to preserve when `ComponentNode` is deleted.

## Parameters: feature-complete, Rust-side SSoT, and `ros2 param list` must work

Decision 3 says the store lives in Rust. Stating what "feature complete" costs,
because the gap is larger than the missing setter.

**What exists.** The executor owns a `nros_params::ParameterTable` and
registers all six ROS 2 parameter services — `GetParameters`, `SetParameters`,
`SetParametersAtomically`, `ListParameters`, `DescribeParameters`,
`GetParameterTypes` (`executor/spin.rs:7182`). Service-wise that is the
complete upstream set.

**The gap is not the service list; it is the KEYING.** Those services are
registered under ONE FQN, built from the executor's own `node_name` and
`namespace`. The store is likewise executor-global. But an image composes
several nodes onto one executor — that is the whole point of the model above.
So today:

* `ros2 param list` shows the executor's node, not the image's nodes;
* two nodes declaring the same parameter name collide in one flat table;
* a parameter declared through a C++ node facade is invisible to the services
  entirely, because the facade's store is a different store (issue 0793).

Upstream's model is one store and one set of six services PER NODE. Matching it
means:

1. **Key the table by node**, not by executor.
2. **Register the six services per node identity**, under each node's FQN, so
   `ros2 param list` enumerates the image's nodes as a ROS 2 user expects. Note
   the cost this lands on: a service server IS a zenoh queryable, six per node,
   against `ZPICO_MAX_QUERYABLES` — the CLAUDE.md pitfall about
   `[param_services]` claiming six slots becomes six PER NODE, which is a
   sizing decision, not an oversight to discover at runtime.
3. **Add the missing setter.** `grep -c nros_cpp_set_param` is 0 and there is no
   `set_parameter` on the executor either, so `SetParameters` currently has a
   service without a store-side writer for the wrapper path.
4. **Delete the C++ stores**, both of them, once 1–3 land. Not before: deleting
   a store that has no replacement is how a capability disappears quietly.

That ordering matters. Steps 1–3 are Rust-side, and step 4 is what makes the
C/C++ side a thin wrapper rather than a second implementation — which is
RFC-0019/0020's rule, and the reason this is a Rust job rather than a C++ one.

# Part IV — Argument order, and what it forced

## Taking rclc's argument ORDER forces a decision on RFC-0041

Measured over the seven entry points: only one is a pure permutation
(`node_init`). Three more — `subscription_init`, `service_init`, `timer_init` —
differ because **ours carry `callback` and `context` that rclc's do not**:

```
rclc_subscription_init_default(sub, node, type_support, topic_name)         [4]
nros_subscription_init        (sub, node, type_info, topic_name, cb, ctx)   [6]
```

That is RFC-0041 (Stable): the callback binds to the ENTITY at creation, where
rclc binds it at `rclc_executor_add_subscription`. So "sort the arguments to
match" is not reachable by reordering — those two arguments have to go
somewhere else, or stay.

### CORRECTED — RFC-0041 does not decide this, and the rule cited instead does not exist

I first wrote that taking rclc's order requires reversing RFC-0041, which is
Stable, and that this was the last blocker for deleting the C compat header.
**Both halves are wrong**, and checking took one grep:

* **RFC-0041 says nothing about a binding site.** Its normative content is the
  DISPATCH MODEL — every callback-capable entity is callback-based by default,
  the executor pumps once per `spin_once`, and an entity must be arena-registered
  to be dispatched at all. Where the callback is *passed* — `*_init` or
  `executor_add_*` — appears nowhere in it.
* **`executor-owns-no-entity-storage`, cited by name in ten ledger rows as the
  reason, is defined nowhere.** Zero occurrences in `docs/design/`. It reads as
  a settled principle and is a phrase the ledger invented for itself.

So the binding site is an implementation habit that was retroactively attributed
to an RFC that does not mandate it. That is issue 1022's class — prose citing a
source that does not support it — in the rows that were about to justify keeping
a compat layer forever.

**What IS real**, and is the only constraint found: `c:timer_exchange_callback`
gives a concrete reason for the RUNTIME case — "the executor's static dispatch
table cannot be rewritten while it is being walked". That forbids *swapping* a
callback on a live entity. It says nothing about which call first supplies one.

**So the decision is smaller and already unblocked.** Moving the callback from
`*_init` to `executor_add_*` needs no RFC amendment; it needs the same shape
W5.a shipped and proved this week
(`nros_executor_add_subscription_typed(exec, sub, msg, cb, ctx, invocation)` —
rclc's arguments in rclc's order). What remains is making it the only shape and
migrating in-tree callers.

**What still needs writing down** is the reverse: the ten rows asserting a rule
that does not exist have to be corrected, and if the habit has a real
justification, it belongs in an RFC rather than in ledger prose that cites
itself.

## Argument ORDER follows the same decision, and is mostly not a separate task

Settled 2026-09-04 with the spellings: where we and rclc/rclcpp name the same
function, our parameters take THEIR order. A ported line has to compile
unchanged, and an argument list is as much of the line as the name.

Measured before committing to it, over the seven entry points the compat work
touches. Only ONE is a reordering:

```
rclc_node_init_default(node, name, namespace_, support)
nros_node_init        (node, support, name, namespace_)     <-- permutation
```

The other six differ for reasons a reorder cannot fix, and reading them is what
makes the task tractable:

| pair | difference | what closes it |
| --- | --- | --- |
| `publisher_init`, `client_init` | `type_support` vs `type_info` — a TYPE difference in the same position | a decision about the typesupport handle, not order |
| `subscription_init`, `service_init` | ours carries `+2` (`callback`, `context`) | RFC-0041 binds the callback to the ENTITY at creation; changing it is a design reversal, not a rename |
| `timer_init` | ours carries `+1` (`context`); also `timeout_ns` vs `period_ns` | same RFC-0041 question, plus a naming one |
| `executor_add_subscription` | rclc carries `+2` (`msg`, `callback`) | W5.a's typed delivery — it converges when that lands |

So argument order is **downstream of the shape decisions**, not parallel to
them. Reordering the one true permutation is a day's work; the other six
converge or do not depending on RFC-0041 and W5.a, and forcing an order onto
lists of different lengths would produce a signature that matches upstream in
neither.

### The hazard, CORRECTED — a distinguishable type is not enough in C

The first version of this section said `node_init` was safe to reorder because
the moved parameter is a `struct nros_support_t *` crossing two `const char *`,
so a stale caller "fails to compile". **That is false for C**, and it was
falsified by a mutation test rather than by review:

```
C   : warning: passing argument 1 of 'f' from incompatible pointer type
      ...even under -Wall -Wextra.
C++ : error: cannot convert 'support_t*' to 'const char*'
```

C permits an incompatible pointer argument with a diagnostic that is a WARNING
by default. So a reorder in the C API is silent-by-default for exactly the
callers it must not be silent for: out-of-tree consumers, who do not build with
our flags.

**The rule, restated:**

* A reorder is safe only where the build treats an incompatible pointer as an
  ERROR. Ours does; a user's does not, and we do not control that.
* Therefore a C reorder must be accompanied by a RENAME, so a stale call fails
  on the identifier — which C does diagnose fatally — rather than on the
  argument type, which it does not.
* Where the moved parameter's type is INDISTINGUISHABLE from its neighbours
  (two `const char *`), it is silent in C++ too, and the rename is not optional
  in either language.

This is why the compat header's reordering forwarder is a `static inline` that
names each parameter and never a macro, and why its probe pins the reorder with
`#pragma GCC diagnostic error "-Wincompatible-pointer-types"` scoped to that TU:
the guard cannot be advisory when the reorder is the whole reason the forwarder
exists. Without the pragma the guarding mutation PASSED.

# Part V — What the measurement can and cannot show

## What "mostly full compat" can honestly mean

It cannot mean the whole rclcpp surface. Four upstream idioms account for most
of what we decline, and three of them are load-bearing for ROS 2's design and
incompatible with ours:

| idiom | rows | can we? |
| --- | ---: | --- |
| executor as wait-set assembler over polymorphic `Waitable`s | ~128 | no — dynamic membership needs an allocator (RFC-0002) |
| parameters as a distributed service with owned-storage values | ~56 | partly — server side yes, client side is a product choice |
| graph and middleware queryable at runtime | ~55 | **partly, and the table above was wrong.** `nros::Executor` already ships `get_node_names`, `count_publishers`/`count_subscribers`, the four `*_names_and_types_by_node` forms and `get_{publishers,subscriptions}_info_by_topic` (`executor.hpp:205-301`) over shipped FFI. 18 rows are pure forwarders from `Executor` to `Node`. What is genuinely impossible is a GRANTED QoS read-back and graph *events* |
| types erased and resolved at runtime | ~45 | no — no dynamic loader |

The honest claim is therefore about PROGRAMS, not surface: *the shapes a ROS 2
node is actually written in compile and behave, and everything else fails
loudly.* That is measurable — the in-tree ported templates are the measurement —
and it is what the roadmap targets.

## The split, measured

All 717 uncovered rclcpp items, classified against the rule:

| disposition | rows | distinct sites | cost |
| --- | ---: | ---: | --- |
| ADOPT | 117 | ~45 | ~820 lines |
| ADOPT-BOUNDED | 123 | ~50 | ~1150 lines |
| REFUSE-LOUD | 69 | **17** | ~120 lines, diagnostics only |
| ABSENT | 408 | — | 0 |

ABSENT is 57 % and is not a concession — 83 rows are the wait-set/`Waitable`
protocol, 42 runtime type erasure, 35 parameter-client internals, 18
`get_node_*_interface`, 21 `make_shared` on types a user never constructs. A
ported user program names none of them.

The shape worth noticing: **69 refusals collapse into 17 diagnostics**, because
a refusal is per-CONCEPT, not per-symbol — the sixteen inert `NodeOptions`
setters share one message. That is the whole loudness pass at about 120 lines,
which is why it can gate the rename without gating the schedule.

## A refusal is a DECLARATION, so refusing improves the numbers

This is the stage-0 finding one level deeper, and it is worse.

REFUSE-LOUD is implemented as a `= delete` or a `static_assert` inside a
template. Both are DECLARATIONS. The extractor sees a declaration, so the symbol
appears on OUR side and the row moves off `theirs-only`:

```
cpp:NodeOptions::enable_logger_service   declined   ours-only
```

That row is `ours-only` because we refused it. Before the refusal it was a name
only rclcpp had; after, it is a name we "have". **The more of the surface we
refuse, the better the correlation looks**, and nothing in the bucket says why.

So a correlation count cannot distinguish:

* *we have this* — `adopt`;
* *we declared it in order to refuse it* — `refuse-loud`.

**Consequence, and it is a rule about how this campaign may report itself:** a
headline compatibility number must EXCLUDE `refuse-loud` rows. Counting them as
coverage counts refusals as features, which is the same error as counting
`ParametersQoS` as `same` while it differs hundredfold — arrived at from the
opposite direction.

It also settles what `disposition` is for. It is not commentary on a verdict; it
is the only field that separates a name we serve from a name we declared in
order to say no. The correlator cannot recover that, because at the level of
names and shapes there is nothing to recover.

## Four ways a row correlates `same` without our having implemented it

A collapse pass over rows that correlate `same` looked mechanical and was not.
Measured 2026-09-04, and each of these was hit:

1. **The gate reads a DIFFERENT surface than the headline bucket.** `--check`
   gates the NATIVE bucket; the shim surface is opt-in (`--check-ported`). Of 53
   rows reading `same`, only 16 were `same` natively — the other 37 are
   `theirs-only` without the shim and still gated. A blind collapse takes the
   gate red.
2. **A refusal correlates `same` by construction** — a `static_assert` template
   or an inert getter is a declarable name, and correlation compares names and
   shapes.
3. **A sentinel correlates `same` and is neither.** `rclcpp::Logger` in the shim
   has the right name and the right member and carries no information to the
   sink.
4. **UPSTREAM moving produces `same` with no change of ours at all.** The rclrs
   reference was re-pinned 0.5.1 → 0.7.0, and 0.7.0 ships actions. Nine rows
   saying "rclrs ships NO action API at all; the convergence point is whenever
   rclrs grows actions" became false without anyone touching our tree.

(4) is the one worth naming as a class: **the ledger is a join over two moving
surfaces, so it goes stale from THEIR side too.** Every other staleness rule in
this campaign — issues 1012, 1022 — assumes we moved. A `--refresh` that bumps
the recorded surface must be followed by a re-read of every row that argued from
its absence, and nothing enforces that today.

## `disposition` is not evidence, and W-M2 proved it

The disposition pass was PROSE-DRIVEN: it read each row's `why` and classified.
That is why it stamped `refuse-loud` on `cpp:Rate`, `Rate::sleep`,
`Rate::reset` and `WallRate` — rows whose prose argued from RFC-0021 — when
W2.d had already SHIPPED them (`rclcpp_compat.hpp:1327`), answering the
objection rather than overriding it: `sleep()` computes a deadline and makes one
call into `nros::spin(remaining_ms, poll_ms)`, so the executor keeps running.

So the field records an intent, and an intent can be wrong about the code. A
later pass must verify against the header, not against the field. Recorded here
because the failure is in a mechanism this RFC introduced, not in the rows.

## Dispositions apply to every upstream item, not only `declined` ones

The four values were measured over all 717 uncovered rclcpp items (117 / 123 /
69 / 408). `--require-disposition` gates only `declined` rows, and two
independent passes over 515 of them returned **zero `adopt-bounded`** — 0 of 307
and 0 of 208.

That is structural, not an omission. `adopt-bounded` means *we have this name,
with a weaker but non-inverting envelope*; `declined` means *we do not have the
contract*. The two cannot both hold, so on a declined row the disposition is a
three-way choice.

`adopt` and `adopt-bounded` live where the RFC's own examples live — `gap` and
`divergence` rows. `create_wall_timer`'s quantised period is a divergence, not a
decline. So:

* on `declined` rows the vocabulary is `adopt` (a verdict bug — the row should
  not be `declined`), `refuse-loud`, or `absent`;
* the gate's scope is narrower than the taxonomy's, deliberately, because
  declines are where a porting user is most likely to be surprised — but a
  reader must not take the zero for a missing population.

## `absent` is a FREQUENCY test, not an internals test

The first wording said `absent` is "correct for rclcpp internals a user program
never names". Both classification passes reported applying a different test, and
the different test is the right one: **would a real ported program name this?**

Two large `absent` populations are not internals by any reading — rcl/rclc
struct-lifecycle plumbing that is public but has nothing to act on
(`*_impl_t`, `*_options_fini`, `get_zero_initialized_*_options`), and whole
rclrs subsystems (the async executor machine, Workers, dynamic messages) that a
user CAN name but almost never does.

A corollary both passes derived independently, worth stating: **a member reached
only through a refused type is `absent`, because the refusal already fired at
the type.** Diagnosing it twice teaches nothing and puts a message where no call
site exists.

## Prerequisite: the measurement must include the shim

**HISTORY as of 2026-09-09 — read "Settled: `rclcpp::` is the HOME" for the
lane's shape today.** This section states the problem stage 0 was written to
solve, and the solution it chose was a fourth translation unit with the shim's
namespace admitted for it alone. That solution is retired, in favour of the
same namespace being admitted for ALL of them.

The C++ parity lane reads three translation units and filters to namespace
`nros`, so `rclcpp_compat.hpp` contributes zero rows (issue 1020). Every number
in this RFC about "how far we are" is therefore measured against the NATIVE API,
not against what ported code reaches. Fixing that is stage 0 of the roadmap:
without it, progress and noise are indistinguishable.

## The measurement can flatter, which is why `disposition` is not bookkeeping

Stage 0 admitted `rclcpp_compat.hpp` as a fourth translation unit, so the lane
reported two surfaces: the NATIVE API's distance from rclcpp, and what a ported
file actually reaches. The ported surface was better by construction — `same`
went 84 → 110, `theirs-only` 717 → 692.

**AMENDED 2026-09-09 — the lane now reports ONE surface, and the argument below
is unaffected.** The two surfaces were a split by NAMESPACE, and with `rclcpp::`
ruled our home both vocabularies are ours; the numbers on the one surface are
`same` 135 and `theirs-only` 651. What the split bought was that a shim-only row
could not be mistaken for a native one, and that mattered while the shim was a
separate header. The reason `disposition` exists is untouched: it was never about
WHICH surface a row sits on, but about a row correlating `same` by shape while
its contract differs — which is as true on one surface as on two, and is exactly
what the two rows below do.

**Two of those newly-`same` rows are the live inversions this RFC was written
about.** Measured 2026-09-04:

```
ParametersQoS                          state=same   surface=ported
NodeOptions::use_intra_process_comms   state=same   surface=ported
```

(`surface=ported` is how those rows read on 2026-09-04. Both are now plain
`same`; the defect each records is unchanged, which is the point.)

`ParametersQoS()` returns `QoS(10)` where upstream is `KEEP_LAST, 1000`, and
that `NodeOptions` setter stores its argument and is never read. Both now
correlate `same`, because correlation compares NAMES and SHAPES and neither
differs. The instrument cannot see the defect it is measuring.

So a row can be `same` by shape and `refuse-loud` by disposition at the same
time, and those are not in tension — they answer different questions. Without
the disposition, "the ported surface matches on 110 items" is a claim the
campaign cannot back, because two of the 110 are known to differ silently and
nothing in the row says so.

That is the argument for the field. It is not metadata about the ledger; it is
the only thing standing between a compatibility number and a false one. The
gate exists (`--require-disposition`) and is off until W-M2 populates the rows.

## Consequences for RFC-0036

RFC-0036 catalogues divergences and says a preference may not be recorded as one.
This RFC adds the missing half: a divergence must also declare its DISPOSITION —
whether the upstream name is adopted, bounded, refused loudly, or absent. A
`declined` verdict with no disposition does not say whether a ported program
gets a compile error or a surprise, which is the only thing a porting user needs
to know.

# Appendix — superseded drafts

## The proposed node shape (2026-09-05)

**SUPERSEDED** by "The node API, proposed under the governing principle"
below. Kept because its measurement of the three working shapes is still the
evidence; its namespace decisions are not.

Grounded in the three shapes that actually exist in the tree, not in the two the
merge question implied.

### The three working shapes today

**A — standalone `main`** (`examples/native/cpp/talker`, and every
`examples/<plat>/<lang>/` leaf):

```cpp
nros::init();
nros::Node node;                        // caller-owned storage
nros::create_node(node, "talker");      // out-ref, returns Result
node.create_publisher(pub, "/chatter"); // out-ref, returns Result
while (running) nros::spin_once(100);
```

**B — workspace typed component** (RFC-0043; the dominant workspace shape —
41 files in `workspaces/cpp` alone):

```cpp
class Talker {
    Publisher<Int32> pub_;   // inline storage, no allocator
    Timer timer_;
    void on_tick();
  public:
    Result configure(nros::Node& node);   // node passed BY REFERENCE
};
```

**C — `ComponentNode`** (RFC-0044/0047; five directories):

```cpp
class SubNode : public nros::ComponentNode {
    SubNode(NodeHandle h) : ComponentNode(h, "sub_node") {}
};
```

And upstream's component, for comparison:

```cpp
class Talker : public rclcpp::Node {
    explicit Talker(const NodeOptions& o) : Node("talker", o) {
        pub_ = create_publisher<M>("chatter", 10);           // returns shared_ptr
        timer_ = create_wall_timer(1s, [this]{ on_tick(); });
    }
};
```

C is upstream's shape with the handle swapped for the executor. **B has no
upstream counterpart at all** — and B is the one that carries our hardest
constraint, because it needs neither an allocator nor derivation nor a vtable.

### The proposal

**One node type. Three constructors. Two entity-creation shapes. B unchanged.**

```cpp
namespace nros { class Node; }
namespace rclcpp { using Node = ::nros::Node; }   // unconditional alias
```

*Constructors* — because construction is not identity, and `rclcpp::Node`
already has several:

1. `Node()` + `nros::create_node(node, name, ns)` — shape A, out-ref, no
   allocator, works on every target. Stays exactly as it is.
2. `Node(const std::string& name, const NodeOptions& = {})` — upstream's, for a
   ported file. Hosted-only, because `std::string` is.
3. `Node(NodeHandle handle, const char* name, const char* ns = nullptr)` —
   shape C's, promoted from a second class to a constructor on the one type.

*Entity creation* — both shapes on the one type, distinguished by argument
order, which is already how the overloads resolve:

| shape | signature | reach |
| --- | --- | --- |
| ours | `create_publisher(Publisher<M>& out, const char* topic, qos) -> Result` | every target |
| upstream | `create_publisher<M>(const std::string& topic, qos) -> shared_ptr<Publisher<M>>` | hosted |

The out-ref family is an **ours-only name for a capability upstream lacks**
(caller-owned storage), which is a permanent divergence under this RFC's own
rules, not a transitional one. The `shared_ptr` family allocates, and that is
inherent to the arena contract rather than to the spelling: the arena stores a
raw pointer and has no unregister, so anything handed back as a pointer needs a
second owner with node lifetime (`owned_entities_`).

*Derivation* — `class Talker : public rclcpp::Node` becomes available, which is
what a ported component writes. It requires the hosted constructor, so it is
hosted-only, and that is honest: on a freestanding target the answer is shape B.

*Shape B survives untouched.* `Result configure(nros::Node&)` takes the one node
type by reference. It was never a node type — only a convention — and after the
merge it is the same convention over the same type. **This is the shape to keep
recommending for firmware**, because it is the only one of the three that needs
no allocator and no vtable.

*`ComponentNode`* becomes `using ComponentNode = Node;` for one release, then
goes. Its one real content is already accounted for: the handle constructor
(3 above). (This draft also named "RFC-0047's several-named-nodes" as a second
content and as constructor 3; corrected 2026-09-09 — see "The several-named-nodes
capability, corrected". The capability is not in the tree and the citation is
RFC-0046's, about the EXECUTOR.)

### What this costs, stated rather than implied

* **`get_logger()` changes behaviour** on the `rclcpp::` spelling: a node-named
  logger replaces the `"nros.compat"` sentinel. Decided above; strictly better,
  but it IS a behaviour change and belongs in the changelog.
* **A vtable appears** only if someone derives. Shapes A and B add none, so no
  embedded image pays for the hosted shape unless it uses it.
* **`sizeof(Node)` must not move with a capability probe** — the hosted members
  live behind an unconditional owner member, enforced by
  `check-cpp-capability-layout` and already the reason `timers_` is gone.
* **The 409 `nros::` call sites do not have to move.** The alias is
  unconditional and both spellings name one type, so migration becomes
  optional per file rather than a flag day.

## The node API, revised (2026-09-05)

**SUPERSEDED** by "The node API, proposed under the governing principle"
below. It predates the principle and kept four names in `nros::`, which is
overturned.

Three constraints, settled: **`rclcpp::Node` is the name**, there is **one node
type**, and it **compiles freestanding**. The third is what shapes the rest,
because upstream's constructor takes `std::string` and its `create_publisher`
returns `shared_ptr` — neither of which exists on a `-nostdinc++` target.

### Layout — one pointer, never a probe

The hosted-only state cannot be a set of conditional members: that is the
capability-layout rule, and `timers_` already shipped a violation of it. So it
goes out of line behind ONE unconditional pointer.

```cpp
namespace rclcpp {
class Node {
    // ... freestanding core, identical on every target ...
    nros_cpp_node_t handle_;
    bool             initialized_;
    void*            executor_handle_;
    ::nros::Clock    clock_;
    // The hosted extras -- `NodeOptions`, the co-ownership vector, the
    // shared_ptr bookkeeping -- live in `detail::NodeHosted`, allocated LAZILY
    // on the first hosted-shape call and never on a freestanding target.
    // One pointer, present in every configuration, so `sizeof(Node)` does not
    // follow a capability probe (`check-cpp-capability-layout`).
    void* hosted_ = nullptr;
};
} // namespace rclcpp

namespace nros { using Node = ::rclcpp::Node; }   // transitional alias, deprecated later
```

Note the direction: **`rclcpp::Node` is the class and `nros::Node` is the
alias**, the reverse of the shim we are retiring. That is what makes the rename
a rename rather than a second name — and it means the 409 remaining `nros::`
call sites keep compiling while they migrate, instead of needing a flag day.

`ComponentNode` is deleted, not aliased. It had no distinction left once the
handle became a constructor.

### The API, by reach

**Freestanding core — every target, no allocator, no vtable, no `std::`:**

```cpp
Node();                                            // caller-owned storage
Result nros::create_node(Node&, const char* name, const char* ns = nullptr);
Result nros::create_node_on(Node&, void* handle, const char* name, const char* ns = nullptr);

Result create_publisher   (Publisher<M>&,    const char* topic, const QoS& = {});
Result create_subscription(Subscription<M>&, const char* topic, void(*cb)(const M&), const QoS& = {});
Result create_service     (Service<S>&,      const char* name, ..., const QoS& = ServicesQoS());
Result create_client      (Client<S>&,       const char* name, ..., const QoS& = ServicesQoS());
Result create_timer       (Timer&, uint64_t period_ms, nros_cpp_timer_callback_t, void* ctx);

const char* get_name() const;
const char* get_namespace() const;
Logger      get_logger() const;      // named for THIS node (decision 1)
Time        now() const;
// the 13 graph queries, unchanged
```

The out-ref family keeps `nros::`-side free functions (`nros::create_node`)
because they are **ours-only names for a capability upstream lacks** —
caller-owned storage. Under this RFC that is a permanent divergence, not a
transitional spelling, so it is correct for them to keep our namespace while the
TYPE takes upstream's.

**Hosted additions — methods only, gated, layout-neutral:**

```cpp
explicit Node(const std::string& name, const NodeOptions& = NodeOptions());
Node(const std::string& name, const std::string& ns, const NodeOptions& = NodeOptions());

std::shared_ptr<Publisher<M>>    create_publisher<M>(const std::string& topic, const QoS&);
std::shared_ptr<Subscription<M>> create_subscription<M>(const std::string& topic, const QoS&, Cb);
std::shared_ptr<TimerBase>       create_wall_timer(std::chrono::duration<...>, Cb);
std::shared_ptr<Service<S>>      create_service<S>(const std::string& name, ...);
std::shared_ptr<Client<S>>       create_client<S>(const std::string& name, ...);

// parameters -- forwarders to the Rust store after phase-426, never a second store
T    declare_parameter<T>(const std::string&, const T&);
bool get_parameter<T>(const std::string&, T&) const;
```

Deriving (`class Talker : public rclcpp::Node`) needs the hosted constructor, so
it is hosted-only. On a freestanding target the answer is the workspace
component shape below, which needs no derivation at all.

### Usage — standalone

*Hosted, a ported file, unchanged from upstream:*

```cpp
#include <nros/nros.hpp>

int main(int argc, char** argv) {
    rclcpp::init(argc, argv);
    auto node = std::make_shared<rclcpp::Node>("talker");
    auto pub  = node->create_publisher<std_msgs::msg::String>("chatter", 10);
    auto timer = node->create_wall_timer(std::chrono::seconds(1), [pub]{ /* ... */ });
    rclcpp::spin(node);
    rclcpp::shutdown();
}
```

*Freestanding, the same type, our names for our capabilities:*

```cpp
#include <nros/nros.hpp>

int main() {
    NROS_TRY_RET(rclcpp::init(), 1);            // returns Result, unlike upstream's void
    rclcpp::Node node;                          // no allocation
    NROS_TRY_RET(nros::create_node(node, "talker"), 1);

    rclcpp::Publisher<std_msgs::msg::String> pub;   // inline storage
    NROS_TRY_RET(node.create_publisher(pub, "/chatter"), 1);

    while (rclcpp::ok()) {
        pub.publish(msg);
        rclcpp::spin_once(100);
    }
}
```

Both name **one** `rclcpp::Node`. The difference is which capabilities the
target has, and every difference fails to COMPILE rather than differing at
runtime — `create_publisher<M>("chatter", 10)` is simply not declared where
`<memory>` is not.

### Usage — workspace

The RFC-0043 typed component is unchanged except that the node's type is now
spelled upstream's way. **This stays the recommendation for firmware**: no
allocator, no derivation, no vtable, and it is the only one of the three shapes
that compiles on every target we ship.

```cpp
// talker_pkg/include/talker_pkg/Talker.hpp
class Talker {
    rclcpp::Publisher<std_msgs::msg::Int32> pub_;   // inline
    nros::Timer                             timer_; // ours-only: upstream has no `Timer`
    int count_ = 0;

    void on_tick();                                  // bound BY IDENTITY, no string name

  public:
    rclcpp::Result configure(rclcpp::Node& node);    // the one node type, by reference
};

// talker_pkg/src/Talker.cpp
rclcpp::Result Talker::configure(rclcpp::Node& node) {
    NROS_TRY(node.create_publisher(pub_, "/chatter"));
    return nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, 1000, this);
}
```

The generated entry constructs each component and calls `configure(node)` —
which is upstream's *manual composition* with the `main` generated instead of
written.

*Hosted workspaces may instead derive, which is what a ported component looks
like:*

```cpp
class Talker : public rclcpp::Node {
  public:
    explicit Talker(const rclcpp::NodeOptions& opts) : Node("talker", opts) {
        pub_ = create_publisher<std_msgs::msg::Int32>("chatter", 10);
    }
};
```

Both are the same type. Which one a package uses is a reach decision, and the
one that reaches further is the one with no `std::` in it.

### What still has no upstream spelling, deliberately

`nros::create_node`, `nros::bind_timer`, `nros::Timer`, `nros::spin_once`,
`rclcpp::init`'s `Result` return, and the out-ref `create_*` family. Each is a
capability upstream does not have, so each keeps our namespace and a ledger row
with a disposition. Giving them upstream spellings would be the compile-and-differ
this RFC exists to forbid.

## Context and `init`, settled (2026-09-07)

Two questions came out of the phase-428 closure: whether `rclrs::Context::
default_from_env()` should exist beside `nros::init()`, and — behind it — what
"from the environment" can mean on a target that has no environment. Both are
settled here; the work items are phase-427 W9 and W10.

### `Context` is not `Node`, in either language

They answer different questions, and one image holds one of the first and
several of the second. `Context` is WHERE this image is connected: locator,
domain, RMW, session mode, and the source those came from. One per process or
image; a resolved value, not an entity on the graph. `Node` is a named
participant with its own entities. RFC-0046 already puts several named nodes
in one image — one per component, through the single `node_builder(name)` funnel
— and the bridge image (`Executor::open_multi`) opens two sessions,
so folding context into node would either copy session config onto every node
or force one node per session. Upstream draws the same line: rclrs has an
explicit `Context` object, rclcpp hides one behind `init()` as the global
default context every `Node` reads. Our C++ already does the rclcpp thing;
the Rust side doing the rclrs thing is the same model in each language's own
spelling (clause "each language follows its own upstream").

The value is thin — five fields and "create the executor from me". The
executor owns the session; the context says where; the node has the name.

### "From the environment" on a target with no environment

rclrs's `default_from_env` reads the process environment (`ROS_DOMAIN_ID`,
`RMW_IMPLEMENTATION`). Hosted, ours reads the same thing (`init.rs`,
`try_resolve_hosted`), and a launcher projects per-node values into that
environment before exec. On an RTOS or bare-metal image there is no process
and no environment; the same values are BAKED at compile time —
`option_env!("NROS_LOCATOR")`, `NROS_DOMAIN_ID` exported by the leaf
`build.rs` from Kconfig or `config.toml` — with one runtime hook for a locator
a board hands in. The semantics survive with the source moved from run time
to build time: "the environment this image was built for or launched in".
That is a bounded adoption, not a lie, and it is what the row says.

Consequence: `Context` compiles on EVERY target. Today `init.rs` sits whole
behind the `env` feature (`std::env`), so a freestanding image has no
`Context` at all and the entry macros build an `ExecutorConfig` by hand from
the baked constants. After W9 the `env` feature gates only the process-env
reader; the freestanding constructor reads the baked constants, and the entry
macros call it — one source of the baked shape.

### The Rust shape

```rust
pub struct InitOptions { domain_id: Option<u32> }          // the one option that exists
impl InitOptions { pub fn new() -> Self; pub fn with_domain_id(self, Option<usize>) -> Self; }

impl Context {
    pub fn default_from_env() -> Result<Context, InitError>;        // hosted: env; freestanding: baked
    pub fn from_env(o: InitOptions) -> Result<Context, InitError>;  // + domain override
    #[cfg(feature = "env")]
    pub fn new(args: impl IntoIterator<Item = String>, o: InitOptions) -> Result<Context, InitError>;
                                                                    // refuses --ros-args loudly; absent freestanding
    #[cfg(feature = "alloc")]
    pub fn create_executor(&self) -> Result<Executor<'static>, InitError>;
    pub fn create_executor_in<'b>(&self, backing: &'b mut [MaybeUninit<u64>]) -> Result<Executor<'b>, InitError>;
}
impl Executor<'_> {
    pub fn create_node(&mut self, name: &str) -> Result<Node, NodeError>;   // several named nodes: RFC-0046
    pub fn spin(&mut self, opts: SpinOptions) -> Result<(), NodeError>;
    pub fn spin_once(&mut self, timeout_ms: u32) -> Result<(), NodeError>; // ours, kept
}
pub fn init() -> Result<Context, InitError>;   // stays: the C++-symmetric anchor, equal to default_from_env()
```

| rclrs | ours | disposition |
| --- | --- | --- |
| `Context::default_from_env` | same | `adopt`; source bounded on freestanding, stated in the row |
| `Context::from_env(InitOptions)` | same, domain only | `adopt-bounded` |
| `Context::new(args, InitOptions)` | same hosted, refuses `--ros-args` at run time | `adopt-bounded` hosted, `absent` freestanding (no argv) |
| `Context::create_executor` | returns `Result`, `alloc` only | `adopt-bounded` |
| `create_executor_in` | ours | `extension` |
| `Executor::create_node` | same | `adopt` |
| `Executor::spin(SpinOptions)` | subset of options | `adopt-bounded` |
| `spin_once` | ours | `extension` (kept, phase-427) |
| `nros::init` | ours, equals `default_from_env` | kept; the C++ anchor |

### CORRECTED 2026-09-09 — the port is SIX edits, not two, and the count is measured

This section first said a ported rclrs `main` "changes in exactly two places,
both `?` on calls that can fail here and cannot there". That was a prediction,
and phase-427 W10 measured it against upstream's own text — the port is held as
two files and diffed at BUILD time
(`packages/testing/nros-tests/tests/rclrs_talker_port.rs`; the ported file is
`include!`d as well as `include_str!`d from one path, so "it ports" is a fact
about the same bytes the count is computed from). **Six lines differ.** The two
predicted are there; four more were never counted:

| kind | n | what |
| --- | ---: | --- |
| import | 2 | the crate each name comes from (`rclrs` → `nros`, the message crate). Any port has these; a prediction about OUR divergences omitted them, and a user counting edits does not. |
| error type | 1 | the `main` signature. `RclrsError` → `Box<dyn core::error::Error>` — and this line is possible AT ALL only because W10 gave `NodeError` a `Display` impl (hence `Error`, hence `?`). Before that the port could not keep one error type in `main`, so the original claim was not merely undercounted, it was unreachable. |
| mutability | 1 | `let mut node`. Ours registers into the executor, so the binding is `mut`; rustc names it. |
| **predicted** | **2** | the two `?`s — `create_executor` and `spin` can fail here and cannot upstream. |

So the sentence's claim about the ERROR TYPES stands and is one of the six, not
an invisible extra: the difference is real, `?` absorbs it, and a `match` sees
it. What was wrong is the number, and the reason it was wrong is instructive —
the prediction counted only the divergences this RFC is ABOUT, and a porting
user counts every line they touch. Under the governing principle all six are
mechanical (each is a compiler-directed edit at the line), which is the property
that actually had to hold; "two" was never the property, only a proxy for it.

The line that is a real design difference — `create_executor` needing a backing
on a no-alloc target — is still the one the compiler names.

**A SEVENTH difference exists and is deliberately not in the count: `spin` →
`spin_blocking`.** It falls on the same line as one of the two `?`s, so it costs
no extra edit, but it is a genuine divergence and absorbing it into a line that
was going to change anyway is how a divergence stops being visible.
`Executor::spin` is taken here by `spin(Duration) -> !` — the body of an RTOS
task, RFC-0002's one-executor-per-task shape — so upstream's
`spin(SpinOptions)` has no free name. **Open item:** moving upstream's
`spin(SpinOptions)` onto `spin` (and renaming ours) is a later wave; until it
lands, a ported rclrs `main` renames the call. It is not W10's, and it is not
closed by W10 being green.

### The C++ shape, and why `init` has two overloads

Not multiple ways to init a node. One way to init the process context, with
argv OPTIONAL, and nodes constructed one way (`Node(name)` + `init()`
freestanding, `make_shared<Node>(name)` hosted; phase-427 W2).

```cpp
inline void init(int argc, char const* const* argv);   // hosted text; aborts if argv carries --ros-args
inline void init();                                    // no argv
```

Upstream has only the argc/argv form because upstream always has a
`main(argc, argv)`. An RTOS image has no such `main` — Zephyr's takes nothing,
ThreadX starts a thread, bare metal enters `app_main` — so the ported
tutorial's `main` does not survive the port on those targets regardless, and
the user writes `init()` (or `init(0, nullptr)`, which also compiles). The argv
form is `adopt-bounded` on every target: it accepts argv, honours nothing in
it, and refuses `--ros-args`. The single-spelling alternative (argv overload
hosted-only) was considered and rejected: the argv form is what makes a ported
hosted `main` compile unchanged, and on RTOS the `main` is rewritten anyway.

Who initialises the context, per case:

| case | who | the user writes |
| --- | --- | --- |
| standalone, hosted | the user's `main` | `rclcpp::init(argc, argv)` / `Context::default_from_env()` |
| standalone, RTOS or bare | the user's entry function | `rclcpp::init()` / `Context::default_from_env()` (baked) |
| workspace, hosted | generated entry | nothing; the node is a component |
| workspace, RTOS | generated entry | nothing; the node is a component |

In a workspace project the user never writes any of this: `nros::main!(launch
= "bringup")` generates the entry, the config comes from the SystemModel, and
the same node links into a Linux process or a Zephyr image unchanged.
