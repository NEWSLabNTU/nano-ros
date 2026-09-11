# Phase 427 — one node type, named `rclcpp::Node`, compiling freestanding

**Status (2026-09-11). W1, W2, W3, W4, W5, W6, W7 and W8 LANDED (W5's runtime
half and W6's book page closed 2026-09-09; W4's freestanding acceptance split
out as issue 1247; W7 landed 2026-09-11 once its blocker cleared — see "W7
LANDED" below).** Implements RFC-0089 §"The node API, proposed
under the governing principle". Preconditions are met by phase-417: the
`pump()` blocker is gone, `check-cpp-capability-layout` measures the layout
rule, and the `-nostdinc++` lane can see a freestanding regression.

## What this is

Three C++ node shapes collapse to one type:

| today | after | status |
| --- | --- | --- |
| `nros::Node` — out-ref creation, freestanding, 103 files | one type, both spellings | **DONE** |
| `rclcpp::Node` — hosted, `shared_ptr`, derivable | the same type, hosted members out of line | **DONE** |
| `nros::ComponentNode` — derivable, entry-supplied handle, 5 dirs | DELETED; its handle becomes a constructor | **DONE** |

Both spellings name ONE class — `std::is_same<rclcpp::Node, nros::Node>::value`
is asserted by `tests/compile/one_node_type.cpp`. Which of the two is the
definition and which the alias is settled the OTHER way from RFC-0089's end
state, for a measured reason recorded below; neither is deprecated yet, so the
remaining call sites are still optional to migrate.

## What landed, and what it measured (2026-09-08)

W1, W2, W3, W5 landed together (one merge is not divisible), W6/W8 landed
first because the merge needed the macro, and phase-430's W6 and W7 landed with
them because they are the same headers. W4 landed 2026-09-09. W7 turned out not to
mean what it says; the reason is measured and recorded below rather than worked
around.

### The number W1 asked for

`sizeof`, hosted `g++ -std=c++17`, via `check-cpp-capability-layout`'s own probe:

| | `nros::Node` | `rclcpp::Node` |
| --- | --- | --- |
| origin/main, baseline | 192 | 3752 |
| origin/main, `-DNROS_CPP_STD=1` | 192 | 3752 |
| **after, baseline** | **200** | **200** |
| **after, `-DNROS_CPP_STD=1`** | **200** | **200** |

Two types became one, and the acceptance criterion — identical with and without
`-DNROS_CPP_STD` — holds. The +8 is `void* hosted_`, which is the entire cost of
the hosted surface to a node that never makes a hosted-shape call: the block is
allocated lazily and a freestanding image can never allocate it at all.

`~Node()` is byte-identical in every configuration. That was not free: the
hosted block's type is hosted-only, so a `#if`-gated `delete` in the destructor
would have given two TUs of one image two different inline destructors — the
ODR half of the same defect. The block carries its own `destroy` function
pointer instead.

### `std::enable_shared_from_this` had to go, and that is a divergence

The shim derived from it. It is a hosted-only BASE with a `std::weak_ptr`
member — 16 bytes of layout behind a capability probe, which is exactly what
the rule forbids and what the deleted `timers_` member already shipped once. So
the base is deleted and `shared_from_this()` survives as a METHOD returning a
pointer that aliases `this` with an EMPTY owner: it observes the node without
extending its lifetime, where upstream's shares ownership.

Ledgered as `divergence` / `adopt-bounded` (`cpp:Node::shared_from_this`). Two
example templates call it (`rclcpp-compat-smoke`, `topic-state-monitor-port`,
both `diagnostic_updater::Updater(shared_from_this(), …)`) and both still build,
because in this API a node is constructed by the generated entry or by `main`
and outlives everything it is handed to.

### W5 was forced by the merge, not chosen after it

The two `get_logger()`s could not both survive: two overloads differing only in
return type are ill-formed. The ported channel wins (clause 2), so the merged
accessor returns `rclcpp::Logger` — which now carries the node's NAME **and**
the opaque `nros_log::Logger` handle, with an implicit conversion to
`nros_logger_t`. That conversion is what kept every native call site compiling:
`NROS_LOG_INFO(node.get_logger(), …)` and the `logger == nullptr` check in
`examples/native/cpp/logging` are unchanged.

### W3 retired `bind_timer` for real

All 22 in-tree call sites moved to
`node.create_wall_timer<C, &C::method>(out, ms, self)` in the same commit;
`NROS_BIND_TIMER` repoints to the member (a deprecation a macro hides is not
one); the scaffold codegen and its test moved with them. The free function stays
one release as a deprecated forwarder with no second code path.

Sweep — and the first one was run with a `--include` glob the shell expanded
to nothing, which is why the `TimerBase` count below was also wrong. The
reliable form is `grep -rn --exclude-dir=.git 'bind_timer' .`, and it now
returns only the deprecated definition, the probes that name it, the gate that
asserts it warns, and archived phase docs. Two doc SNIPPETS the first sweep
missed (`book/src/getting-started/workspace-cpp.md`, the model-1 slides)
migrated too: a book page teaching a deprecated call is a call site with extra
reach.

### The namespace direction was the opposite of the RFC's end state — RESOLVED by W7, 2026-09-11

**Superseded.** Everything below was true of the node-type merge and is kept as
the record of why the direction was inverted for two days; W7 flipped it, the
class is `rclcpp::Node`, and both blockers named here are gone (phase-428 fixed
the extractor's roots; PRs #797 and #806 merged). Read it for the reasoning, not
for the current shape.

RFC-0089 describes `rclcpp::Node` as the class and `nros::Node` as the migration
alias. **It is the other way round here, because of the parity tool's own
configuration.** `scripts/api-parity.py` extracts the NATIVE C++ surface with
namespace root `{"nros"}` and the PORTED surface with `{"rclcpp", …}`, and
`just check api-parity --check` gates only the native bucket. Defining the class
in `rclcpp` empties `nros::Node::*` from the native surface, so roughly sixty
members re-bucket to `theirs-only` with no ledger row and the gate goes red.

That is not a reason to keep two vocabularies — both spellings name one type and
`std::is_same<rclcpp::Node, nros::Node>::value` is asserted — but it does mean
the flip is a change to the extractor's roots, which re-buckets the whole ported
surface at once. That belongs with phase-428's whole-tree `nros::` -> `rclcpp::`
sweep, not with one type.

Consistency argues the same way: every other type in this API is already spelled
this direction (`rclcpp::Publisher`, `rclcpp::QoS`, `rclcpp::Timer`,
`rclcpp::Clock` are aliases of `nros::` definitions). `Node` being the one
exception would have been arbitrary.

The alias is UNCONDITIONAL, which the shim class was not: it lived inside
`#if defined(NROS_CPP_HAS_SHARED_PTR) && …`, so a freestanding target had the
node and not its ROS 2 name — the two vocabularies split exactly where the port
matters most.

### W7 LANDED 2026-09-11 — the flip first, then the deprecation

Both blockers below cleared, and W7 landed as one branch in two commits: the
namespace flip, then the migration and the attribute together.

**The flip.** `class Node` is DEFINED in `rclcpp::` now and `nros::Node` is the
alias, the tenth and last of RFC-0089's table. PRs #797 and #806 merged, so the
stacking objection is gone; the extractor objection is gone too, because
phase-428 made `OUR_CPP_ROOTS` name both halves of the vocabulary we ship. Three
mechanics the other nine types did not need are recorded in RFC-0089 §"The flip
is DONE — all ten": forward declarations must move (an elaborated `class Node;`
in `nros::` would declare a second class), friend declarations must be qualified
AND parenthesized (`friend Result(::nros::init)(...)`, because a
nested-name-specifier is parsed greedily), and all sixteen out-of-line
`Node::create_*` bodies move with the class. The near-miss worth repeating is
`QoS`: `rclcpp::QoS` is a SUBCLASS of `nros::QoS`, so a bare `QoS` inside
`namespace rclcpp` would have re-typed every `create_*` parameter to the derived
class — compiling, non-erroring, and wrong.

**The deprecation, and the migration in the same commit** — which is what the
paragraph below demanded. 258 in-tree C++ spellings migrated; the attribute is
`NROS_CPP_DEPRECATED_MSG` and UNCONDITIONAL, not feature-armed. The evidence for
that choice, since PR #753 set the armed precedent in Rust: #753 armed BECAUSE
its tree had not migrated (65 hard errors across 47 files), and that argument
inverts here. Every other C++ deprecation this API ships is unconditional. And a
macro nothing in-tree defines leaves the probe as the only compiler that ever
sees the attribute, which is a gate whose subject disappears. The reach it is
for is out-of-tree and it is real: `nros-v0.5.0` (2026-06-08) shipped
`nros::Node` in six `examples/templates/**` files a user copies out.

**What did NOT migrate, and why.** The raw grep is 561 sites; 258 are C++ code.
The rest split three ways, each deliberate:

* **`nros::Node` also names a RUST TRAIT** (`nros-macros`, `impl nros::Node for
  Talker`, 58 `.rs` sites). Different symbol, different language, untouched.
* **`docs/roadmap`, `docs/issues` and the dated paragraphs of the api-parity
  ledger are historical record.** "`nros::Node` WAS the definition" is true as
  written; rewriting it would turn a record into a falsehood. Left alone, this
  file's own history included.
* **`book/src`, `docs/guides`, the example READMEs and RFC-0018's code
  blocks teach the name a reader should WRITE**, so those migrated — 28 sites.
  RFC-0044 did not: its subject is `ComponentNode`, which is deleted, so it is
  history too.

**One pre-existing tool defect fell out of it.** `correlate.canon_type`'s
namespace strips are `^`-anchored, so a LEADING `::` made one type canonicalise
two ways — `::std::string` never reduced to `string`. Our hosted overloads are
spelled `::std::string` and upstream's `std::string`, so SEVEN C++ rows reported
`systematic` over an overload we actually ship. same 131 -> 138, systematic
14 -> 7, nothing the other way. Two ledger rows also changed shard,
`cpp:spin_once` and `cpp:global_handle` (`exec`/`other` -> `node`): their
declarations genuinely live in `node.hpp` now, which is what the qualified
friendship requires, and the taxonomy files a top-level name by its header.

### What blocked it, recorded as it stood before the flip

W7 is "`nros::Node` deprecated with `NROS_DEPRECATED_MSG`, not deleted". A
deprecation attaches to the ALIAS, and after the merge the alias is
`rclcpp::Node` — the name a user is supposed to write. Deprecating the
definition would deprecate both spellings at once, because they are one name for
one class.

So W7 is **blocked on the namespace flip**, which is blocked on the extractor
roots above. What landed of it:

* `NROS_CPP_DEPRECATED_MSG` ships (`result.hpp`), C++11 attribute with a
  GNU/clang fallback, so the migration channel exists.
* It is USED, on the one ours-only name this phase actually retired:
  `nros::bind_timer`.
* Even after the flip, a bare `[[deprecated]]` on `nros::Node` warns at every
  in-tree site at once — 191 lines across `examples/` and `packages/` `.cpp` /
  `.hpp`, 218 counting codegen templates, goldens and prose — which the phase's
  own "Not in scope" section says must stay optional and incremental. Whoever
  lands it should migrate the tree in the same commit, or the attribute is a
  flag day wearing a migration's clothes.

### W8's settled shape, and the one part of it C++ cannot express

RFC-0089 settled on "one template, one name": `Expected<T>` renamed to
`Result<T>`, today's `Result` becoming `Result<void>` with `Result` as the alias.
**C++ cannot express the last part.** In one namespace `Result` is either a
class-template name — in which case the bare `Result` that every call site and
every ported file writes is ill-formed, because a defaulted template parameter
still requires `Result<>` — or a non-template type name, in which case
`Result<T>` is ill-formed. A using-declaration, an alias template and a
nested-namespace re-export all redeclare the same name and collide with the
other spelling.

What landed (2026-09-07, recorded in full under "W8 LANDED" below) is the
STRUCTURE the RFC asked for with only the template's own spelling changed: one
template **`ResultOf<T>`**, the value-less case IS its `void` specialization,
and `Result` is an alias for that specialization. `Expected<T>` survives one
release as a `[[deprecated]]` class template converting from `ResultOf<T>`.
`NROS_NODISCARD` is on both, and the RULE for which channel an API picks is
stated in `result.hpp` beside the types it governs.

Blast radius, measured: exactly ONE discard existed in the headers —
`rclcpp::shutdown()` dropping `nros::shutdown()`'s `Result`. It now reports it,
which is also more faithful, since upstream's `bool` means "was the shutdown
successful".


### phase-430 W6 and W7, landed here because they are the same headers

**W7 — the `TimerBase` ruling: DELETE.** RFC-0089 §"Timer, studied against RTOS
semantics" decided `Timer` stays flat; the tree disagreed (phase-417 W1.a had
landed `class TimerBase` with a virtual destructor and `detail::WallTimer` under
it). The ruling is delete, argued from the executor's dispatch:

1. **The vtable has no caller.** The only virtual member was `~TimerBase()`, and
   there is no virtual call through a `TimerBase*` anywhere in the tree — there
   cannot be. The executor's callback slot is `nros_cpp_timer_callback_t`, a raw
   `void(*)(void*)`, and dispatch goes through the STATIC
   `detail::WallTimer::trampoline`; the executor never holds a C++ timer object.
   Even the destructor's virtuality was dead: the cell is built with
   `std::make_shared<detail::WallTimer>()`, so the control block already records
   the concrete deleter.
2. **The name promises children we refuse to have.** `WallTimer` and
   `GenericTimer` stay absent by decision — the clock axis is a runtime field
   plus a second VERB, which is the shape phase-425 shipped. A base whose one
   leaf is `detail::`-private advertises a taxonomy the header refuses two
   paragraphs later.
3. **The flat shape is a better keep-alive.** `create_wall_timer` returns
   `std::shared_ptr<nros::Timer>` aliased onto the private cell — the shape
   `create_subscription` has always used.

**Cost, measured TWICE, and both earlier answers were wrong.** This is worth
recording in full, because each wrong answer changed what shipped.

*First answer, "zero non-test call sites."* A `grep` whose `--include` glob the
shell expanded to nothing — the command matched no files and reported no hits.
The real in-tree count is three source files
(`cpp-port-minimal-publisher`, `workspace-shadowing`, `local-msg-package`) plus
a book snippet. Acting on the wrong number, the name was deleted outright.

*Second answer, "a one-release deprecated alias."* Better, and still wrong,
because it treated the alias as a courtesy to in-tree consumers. **The
`colcon-parity` gate then failed and named the real constraint**:

```
rclcpp::Timer -> 'Timer' in namespace 'rclcpp' does not name a type;
                 did you mean 'Time'?          [REAL ROS 2, humble]
```

`just colcon-parity` builds `examples/templates/local-msg-package` against a
REAL `/opt/ros/<distro>` install, and `cpp-port-minimal-publisher` is vendored
UNMODIFIED to demonstrate upstream source compiling here. Both declare a timer
member, and **upstream has no `rclcpp::Timer`** — only `TimerBase`/`WallTimer`.
So without the alias, the intersection of "compiles under real ROS 2" and
"compiles under nano-ros" is EMPTY for any ported node that holds a timer, which
is most of them.

**Settled ruling.** The HIERARCHY is deleted — that is what W7 asked and where
the cost was. The NAME is KEPT as the ported alias, and NOT deprecated:
deprecating it would promise a removal that would break the campaign's own
flagship property on purpose. `rclcpp::TimerBase` and `rclcpp::Timer` are one
flat type, pinned by `timer_base_is_a_name_not_a_hierarchy.cpp`
(`is_same<TimerBase, Timer>`, `!is_polymorphic`, both nested `SharedPtr`
spellings, and both member declarations the templates actually contain).
`WallTimer` and `GenericTimer` stay absent, with a self-tested detector so
"we still do not have them" is asserted rather than assumed.

RFC-0089 §"Timer, studied against RTOS semantics" says an alias "sells a
taxonomy we do not have". That argument is right about advertising and did not
consider the dual-compile templates; the correction is in the RFC's amendment.
`rclcpp::Timer` is UNCONDITIONAL, so a freestanding target gets a ROS 2 timer
name too.

**W6 — the clock-taking verb** on the merged type
(`create_timer<C, &C::m>(out, clock, ms, self)`) and the hosted free
`rclcpp::create_timer(node, clock, period, cb)`, humble's only form, returning
the same cell `create_wall_timer` does. Two period spellings, because
`rclcpp::Duration` is implicitly constructible from a chrono duration upstream
and `nros::Duration` is not (it reaches targets where `<chrono>` does not
exist), so the conversion is an overload rather than a constructor.

### Acceptance, and what proves each

| item | proof |
| --- | --- |
| W1 `sizeof` invariance | `check-cpp-capability-layout`, plus the table above |
| W1 hosted members present | `one_node_type.cpp` exercises `get_node_options`, the `shared_ptr` creators and the parameter facade |
| W2 freestanding TU | `tests/compile/one_node_type_freestanding.cpp`, compiled `-nostdinc++` against the ThreadX minimal libcpp: constructs a node, creates a publisher |
| W2 hosted constructor verbatim | `one_node_type.cpp` `upstream_construction_verbatim()` |
| W3 both families, no ambiguity | `one_node_type.cpp` `both_create_families_on_one_object()` |
| W3 member binding, no allocation | `one_node_type.cpp` `Talker::configure` + the `static_assert` on its shape |
| W3 ported line FAILS freestanding | `tests/compile/ported_create_publisher_freestanding_probe.cpp`, an expected-failure whose diagnostic the lane GREPS for the out-ref overload's signature — "it failed" is also what a typo produces |
| W5 distinct logger names | `one_node_type.cpp` `two_nodes_two_logger_names()` compile-side, plus TWO runtime cells: `nros_node::executor::tests::two_nodes_on_one_executor_emit_under_their_own_names` (two nodes on one executor, asserted on the captured `Record`s) and `nros_cpp::node_logger_name_tests::two_cpp_nodes_emit_under_their_own_names` (the accessor `rclcpp::Node::get_logger()` actually calls) |
| W6/W8 `[[nodiscard]]` | `result.hpp`; the one in-tree discard is fixed |
| phase-430 W6 | `one_node_type.cpp` `clock_driven_timer_is_humbles_free_verb()` |
| phase-430 W7 | `ros2_one_dispatch_path.cpp` asserts `!is_polymorphic<detail::WallTimer>` and the return type |

Gates run green: `just check cpp` (which now carries the three new probes),
`just check api-parity` (5 new rows in `node.json`), `check-cbindgen-headers`,
`check-ffi-struct-mirrors`, `check-api-parity-ledger`,
`check-cpp-freestanding-includes`, `check-cpp-capability-layout`.
`just check fast` has 4 reds, all pre-existing and environmental on this host
(zenoh-pico and the XRCE vendored trees are not checked out; a
`docs/issues/README.md` commit citation; a sandbox `PermissionError` in
`check-xrce-source-manifest`'s own self-test).

### A sweep the merge made possible, and one live defect it found

`check-cpp-capability-layout` MEASURES its rule rather than grepping for it,
which is right, but its reach is an authored three-name list
(`TYPES=("rclcpp::Node" "::nros::Node" "::nros::QoS")`) written for the
`timers_` defect it was built to catch. Nothing else in the API has ever been
measured — the issue 0196 shape, a gate narrower than its own rule.

Asking the same question of every public type turned up **three that follow a
probe**, all pre-existing and none of them `Node`:

| type | baseline | `-DNROS_CPP_STD=1` |
| --- | --- | --- |
| `nros::Timer` | 24 | **32** |
| `nros::GuardCondition` | 32 | **40** |
| `nros::ComponentNode` | 55784 | **55848** |

One member, twice: the `#ifdef NROS_CPP_STD` `std::unique_ptr<std::function<
void()>> closure_` that `attach_std_closure` writes. `ComponentNode` is not a
third instance — it holds `Timer timers_[8]`, and 8 x 8 = 64 is exactly its
delta.

Filed as **issue 1225** rather than fixed here, because the mechanical fix (the
`hosted_` trick) costs a FREESTANDING target +8 bytes per timer and per guard
condition and +64 per `ComponentNode`, and the cheaper alternative — making the
closure caller-owned, the shape `rclcpp::detail::WallTimer` already uses — is a
design question this phase did not ask. `nros::Node` (200/200), `QoS`,
`Executor`, `Clock`, `Time`, `Duration`, `CallbackGroup`, `NodeBuilder`,
`LifecycleNode` and `rclcpp::NodeOptions` are clean.

### W5's runtime half landed, and the claim was FALSE when it was written

The compile probe proves the logger is built from the node's name. Writing the
runtime assertion — *two nodes in one image emit records under distinct logger
names* — measured that the tree did not do it, and had never done it.

Four accessors answer "which logger is this node's?" —
`nros_node::executor::Node::logger`, `nros_node_get_logger`,
`nros_cpp_node_get_logger` and `nros_log_get_logger`. Three of the four called
`nros_log::get_logger`, which is a LOOKUP: it answers `DEFAULT_LOGGER` for any
name no `'static` `Logger` was `register_logger`ed under, and nothing on any
node-creation path registers one. So every node in an image resolved to the one
catch-all logger and emitted every record under the name `"nros"` — the
`"nros.compat"` sentinel W5 deleted, reached by a different route, and invisible
to a probe that only reads the name back off the accessor.

The fix is the answer phase-417 W4.d had already built for the fourth accessor:
the create-or-fall-back-loudly resolution is now `nros_log::resolve_logger`, ONE
helper the four call sites share instead of a second spelling in three crates. A
pre-registered `'static Logger` still wins, because lookup comes first; an
exhausted arena still falls back to the catch-all, and now says so at WARN
instead of aliasing silently — once per process, because a C++ `NROS_LOG_*` call
site asks `node.get_logger()` on every line.

Asserted on the RECORDS, through a real `LogSink`, because a name read off the
accessor and a name a dispatched `Record` carries are the two answers that were
disagreeing. Both cells are hosted: `nros-log`'s capture seam works on any
target, but the buffers are host types. The nros-cpp cell builds its two
`nros_cpp_node_t` values by hand — the accessor reads exactly one field, and
node creation needs a live executor and a backend session — so the full
node-lifecycle form of the claim is the nros-node cell.

Negative control: reverting both accessors to `get_logger` fails both cells
with `both nodes resolved to ONE logger, named `nros``.

## W4 — LANDED 2026-09-09, and what measuring it corrected

Both blockers the previous pass identified were ruled on by the maintainer, and
both rulings survived contact with a compiler — but the REASONS moved. What
follows replaces the "what it has to decide first" analysis; the original
prediction is kept inline where the measurement contradicts it, because a
corrected prediction is more useful than a deleted one.

### Blocker 1, the silent collision — resolved by RENAME, and the trigger was
### not the spelling the item named

The ruling: the ours-only family takes a different NAME, because in C++ a
signature-only difference is resolved silently. Applied as an `_in` suffix —
`create_publisher_in`, `create_subscription_in`, `create_wall_timer_in`. Third
application of the rule in this campaign, after the reordered C node initialiser
and the clock-taking C timer verb.

That suffix was **not free**, and the follow-on below (`_in_group`) is what it
cost: `_in` already meant "in a callback group" on seven overloads, so W4 gave
one class two families under one name. Recorded there, not here, because the fix
is a naming rule and not part of the deletion.

The item predicted `create_publisher<M>("chatter", 10)` would bind OURS, on the
reasoning that array-to-pointer decay is a standard conversion while
`std::string` needs a user-defined one. **Measured on the merged shape (gcc 13,
`-std=c++17`), that call binds UPSTREAM**: `int -> size_t` is a standard
conversion and rescues upstream's second argument, so ours is not better in
every argument and does not win.

The spelling that DOES collide is the explicit-QoS one:

| ported call | binds | how loud |
| --- | --- | --- |
| `create_publisher<M>("chatter", 10)` | UPSTREAM (`shared_ptr`) | correct, silent |
| `create_publisher<M>("chatter", rclcpp::QoS(10))` | **OURS (by value)** | accepted, note only |

gcc takes the second as an extension, emitting nothing but "ISO C++ says that
these are ambiguous". So the collision is real, it is silent, and it fires on
the spelling a ported rclcpp file is most likely to carry — which is a stronger
argument for the rename than the one the item made, not a weaker one.
`nros::QoS(int)` being `explicit` is what keeps the integer form safe;
`rclcpp::QoS(size_t)` is not explicit, which is what makes the QoS form unsafe.

Pinned in `tests/compile/one_node_type_ours_only_names.cpp`: the positive half
asserts (via `decltype` on the call, not a comment) that every ported spelling
reaches upstream's overload on the real type, and the NEGATIVE CONTROL
reconstructs the pre-rename shape and asserts it binds the wrong one. Without
the control the file would pass equally well if the collision had never existed.

### Blocker 2, the unconditional bytes — resolved by SPLITTING, and the number
### the item cited was 0.4 % of the real one

`sizeof(nros::ComponentNode)` measured **55,784 bytes**. The 24-byte latch and
the 192-byte timer pool the item asks about are rounding error next to its
inline `ParameterServer<256, 8, 4096>`, which is ~55.5 KB and which **no
consumer in the tree uses** (zero `declare_parameter` call sites across all
three subnode/POC packages). So the parameter facade did not move: components
inherit `Node`'s existing hosted store, and 55.5 KB per component left with the
type.

Of the two the ruling did address:

- **The error latch STAYS unconditional**, as ruled. It is the error channel a
  `-fno-exceptions` target has instead of a throwing constructor, so putting it
  behind the hosted block would leave the firmware case — the one that cannot
  throw — with no channel at all.
- **The timer pool becomes a template parameter defaulting to zero**, as ruled,
  realized as `template <::size_t MaxTimers> class NodeWithTimers : public Node`.
  `Node` itself could NOT become the template, and this is measured rather than
  preferred: `rclcpp::Node` is an alias `one_node_type.cpp` asserts with
  `std::is_same`; 218 in-tree sites spell `Node` with no argument list, which a
  class template with a defaulted parameter does not permit; and every `Node&`
  parameter in the tree would otherwise accept exactly one depth, so a component
  with a pool would not be a node the executor could take. Deriving keeps the
  IS-A relation `ComponentNode` never had — it WRAPPED a node, which is the
  reason this merge exists — while keeping the bytes opt-in.

### The number

`sizeof`, via the layout gate's own `ShowSize<sizeof(T)>` probe:

| | hosted `g++ -std=c++17` | freestanding `-nostdinc++` (ThreadX shim) |
| --- | --- | --- |
| before W4 | 200 | 200 |
| **after W4** | **224** | **224** |

+24, exactly the latch, and identical in both configurations — so
`check-cpp-capability-layout` still passes: a capability probe may gate a
METHOD, never `sizeof`. `NodeWithTimers<N>` adds its pool on top, opt-in.

### The gates: 14 surfaces, not 4

The previous pass reported four gates keyed on the file. A full sweep
(`scripts`, `just`, `.config`, `.github`, `cmake`, plus every checker in the
tree) found **fourteen**, and one of the reported four was not a gate on this
file at all:

**Nine would have HARD-ERRORED** — `scripts/api-parity.py`'s
`CPP_TRANSLATION_UNITS` (whose `extract_cxx` RAISES on any clang error);
`nros.hpp`'s own `#include` of the header, which widens that from one TU to all
four AND breaks `just check cpp`'s header-glob loop; four compile TUs in
`just/check/lanes.just` (`declared_qos_depth`, its expected-failure probe,
`timer_binding_paths`, `ros2_param_launch_seed`); `entry.cpp.jinja`'s emitted
include; and the example/e2e surfaces.

**Five would have passed VACUOUSLY** — the shape this campaign keeps hitting:
`check-c-array-guard-probe.py`'s `PROBE_CONTEXT` key (the table is consulted
only for files the scan DISCOVERS, so a key naming a deleted file is never
read); its upstream half `check-c-array-pool-floors.py`; the two
`assert!(src.contains("#include <nros/component_node.hpp>"))` unit tests in
`emit_cpp.rs`, which assert on EMITTED TEXT and so stay green over a header that
no longer exists; and 18 ledger rows, which `api-parity.py` has no orphan check
for.

**One was misreported**: `scripts/check-cxx-standard-floor.py` reads its floor
from `packages/api/nros-cpp/CMakeLists.txt` via a regex on
`target_compile_features(nros-cpp-headers INTERFACE cxx_std_NN)`. It never opens
or names `component_node.hpp`; the mentions are docstring and error-message
prose. The gate is unaffected.

Each retargeted gate carries a negative control. The guard-probe got a new
STALE-KEY check (a `PROBE_CONTEXT` entry naming a file that does not exist is
now a failure) — verified to flag the old `component_node.hpp` key and to find
zero stale keys today. The `emit_cpp.rs` assertions now pin BOTH directions: the
placement-new include is present AND `component_node.hpp` is absent.

### W4's follow-on — `_in` carried two meanings, and the group family is now `_in_group`

LANDED 2026-09-09. W4's `_in` suffix (above) collided with the one RFC-0047 had
been using since phase 273. Two families, one name, on one class:

| meaning | first parameter | overloads before |
| --- | --- | --- |
| in a callback group | `const CallbackGroup&` | 7 |
| ours-only / storage-free | `const char*` or `uint64_t` | 4 |

The compiler was never in doubt — the first-argument types are disjoint and
neither converts to the other — so this is not W4's silent-collision hazard
again. It is the reader's version: the name did not say which family a call had
joined, and one overload,
`create_subscription_in<M, C, &C::method>(group, topic, qos)`, was in both.

**The group family renames to `_in_group`; `_in` keeps the ours-only meaning.**
The predicate is the first parameter's TYPE: a creation verb whose first
parameter is a callback group ends `_in_group`. Reasoning and the deprecation
decision are recorded in RFC-0089, "The `_in` rule, amended" — in short: `_in`
already reads as "in place" everywhere else here (`Executor::open_in`), the
group form is the one with something to name, and the C ABI beneath already
spelled it `_in_group` (`nros_cpp_timer_create_in_group`,
`nros_executor_add_{timer,subscription}_in_group`) so the C++ member calling it
now agrees with what it calls.

Renamed, measured by first-parameter type and not by name:

* C++ `Node` — `create_timer_in_group` ×2, `create_subscription_in_group` ×2
  (the out-ref callback-style and the storage-free member-pointer),
  `create_publisher_in_group` ×1.
* C++ `NodeWithTimers` — `create_timer_in_group` ×2, plus the
  `using Node::create_timer_in_group` that keeps the base overloads visible.
* Rust `NodeCtx` — `create_timer_in_group`, `create_timer_on_clock_in_group`,
  `create_subscription_in_group`, `create_publisher_in_group`. Rust never had
  the ambiguity (no overloading), but three languages naming one concept is one
  vocabulary or it is three, and the C ABI had already chosen the word.

Unchanged, because none of them takes a group: `Node::create_publisher_in`,
`Node::create_subscription_in`, `NodeWithTimers::create_wall_timer_in`, and the
three `NROS_SUBSCRIBE` / `NROS_CREATE_WALL_TIMER` macro expansions.

**No deprecated alias.** The only tag in the repo, `nros-v0.5.0`, contains no
occurrence of any of these names in code; every consumer is in-tree (7 call
sites, 4 in the `realtime-cpp*` workspaces and 3 in `nros-cpp/tests/compile/`,
zero in `book/`, zero in codegen) and moves in the same commit; and a deprecated
`create_timer_in(group, …)` forwarder would put both meanings back into one
overload set, which is the condition the rename removes.


### One thing W4 did not decide

Deleting `adopt_launch_seed_` removed the tree's **last `if constexpr`**. The
C++17 floor (`cxx_std_17`, raised deliberately by issue 1118) now has no live
justification in the header set — the remaining three mentions are comments.
Lowering it is a separate decision with its own blast radius and is NOT part of
W4; the fact is recorded in `nros.hpp` beside the surviving helper so whoever
takes it up finds it.


### What the pre-W4 analysis predicted, and how it held up

Kept because a corrected prediction is more useful than a deleted one. The
pre-W4 pass called the delta **14 members, not a rename**, and that was right:
`get_name`, `get_namespace`, `get_logger`, `node()`, `create_callback_group` and
the scalar parameter facade were already on the merged type and were deleted
outright, and `adopt_launch_seed_` went with its twin
`rclcpp::detail::adopt_executor_param_seed` kept as the surviving one.

Item by item:

1. **The `create_publisher` collision — REAL, wrong trigger.** See blocker 1
   above: `("chatter", 10)` binds upstream, `("chatter", rclcpp::QoS(10))` binds
   ours. Resolved by rename, as ruled.
2. **The 24-byte error latch — CONFIRMED, and it stays unconditional.** Measured
   `sizeof(nros::Node)` 200 -> 224.
3. **The 192-byte timer pool — CONFIRMED, and the framing was too small.**
   `sizeof(ComponentNode)` is 55,784 bytes; the pool is 0.3 % of it and the
   inline `ParameterServer<256, 8, 4096>` is ~55.5 KB. No consumer uses that
   facade, so it did not move. The pool became `NodeWithTimers<N>`.
4. **"Do NOT add a virtual destructor" — FOLLOWED.** None was added. Derivation
   works without one; only `delete base_ptr` is UB and nothing does that.
5. **`check_declared_depth` and the member-pointer families moved as-is** — with
   `_in` names, and the `std::vector<T>` parameter arm did NOT move, because it
   belonged to the parameter facade that stayed behind.

**RFC-0047's "one component, several named nodes" DOES NOT EXIST** — a
correction PR #773 landed, and W4 preserved no such capability. `ComponentNode`
owned exactly one node; what the subnode packages exercise is one node with
several NAMED CALLBACK GROUPS, which the merged type already carried. The merge
is TWO constructors, not three.

**No subnode package builds freestanding**, which is why that acceptance became
issue 1247 rather than a check W4 could run.

Codegen moved with it: `entry.cpp.jinja` dropped the include, the `is_rclcpp_node`
branch in `emit_cpp.rs` is unchanged (every symbol `node_body.jinja` names —
`NodeHandle`, `ok()`, `error_what()`, `error_code()`,
`detail::report_component_failure` — survived the merge), and the two goldens
moved with the template.

## Work items

* **W1 [cpp] — the out-of-line hosted block. DONE (2026-09-08).** `void* hosted_` replaces every
  hosted-only member; `detail::NodeHosted` is allocated lazily on the first
  hosted-shape call and never on a freestanding target.
  *Acceptance:* `check-cpp-capability-layout` passes with the hosted members
  present, and `sizeof(rclcpp::Node)` is identical with and without
  `-DNROS_CPP_STD`.
  *Met:* 200 bytes in both configurations, against 192 / 3752 for the two
  separate types on `origin/main`. `std::enable_shared_from_this` had to be
  deleted to get there (a hosted-only base with a `weak_ptr` member IS a
  capability-dependent layout); `shared_from_this()` survives as a method and
  the divergence is ledgered.

* **W2 [cpp] — construction. DONE (2026-09-08).** `Node(const char*)` + `init()` + `ok()`
  freestanding; upstream's `std::string`/`NodeOptions` constructors hosted.
  *Acceptance:* the `-nostdinc++` lane compiles a TU that constructs a node and
  creates a publisher; a hosted TU compiles upstream's constructor verbatim.
  *Met:* `tests/compile/one_node_type_freestanding.cpp` (ThreadX minimal libcpp)
  and `one_node_type.cpp` `upstream_construction_verbatim()`, both wired into
  `just check cpp`.

* **W3 [cpp] — one name, two signatures. DONE (2026-09-08).** The out-ref `create_*` family and
  the `shared_ptr` family coexist as overloads on the one type;
  `create_wall_timer` gains the member-binding template overload, retiring
  `bind_timer`.
  *Acceptance:* a component binds a member function with no allocation; a
  ported file's `create_publisher<M>("chatter", 10)` compiles hosted and FAILS
  TO COMPILE freestanding, with a diagnostic naming the out-ref overload.
  *Met:* `one_node_type.cpp` for the binding (plus a `static_assert` on its
  shape), `ported_create_publisher_freestanding_probe.cpp` for the refusal —
  and the lane GREPS the diagnostic for the out-ref signature, because "it
  failed" is also what a typo produces. All 22 in-tree `bind_timer` call sites
  moved in the same commit.

* **W4 [cpp] — `ComponentNode` deleted. DONE (2026-09-09), with the freestanding
  half split out as W12 (issue 1247).** `packages/api/nros-cpp/include/nros/
  component_node.hpp` is gone; its members are on `nros::Node`, its pool is the
  opt-in `nros::NodeWithTimers<N>`, and its macros are in `component.hpp`.
  RFC-0044 is amended, not deleted — its Q2 boot-failure reasoning is `ok()`'s
  now. The two blocker rulings and what measuring them changed are below.
  *Acceptance:* zero `ComponentNode` in the tree — **met**; the RFC-0047 subnode
  packages build and run — **met** (both migrated to `NodeWithTimers<2>`);
  **one of them builds for a freestanding target** — **NOT met, and it was never
  a check.** No subnode package has ever had a freestanding fixture row (all
  three consumers are `platform = "linux"`), so this acceptance names a PORT
  that has to be written. It is **W12**, filed as issue 1247, rather than attempted here.

  **The plan W4 was built against** — `main`'s amendments of 2026-09-09,
  kept as the record of what the migration had to decide:

  The first version of this item also required that "RFC-0047's
  several-named-nodes survives as a documented ours-only capability on it".
  **That clause is struck: the capability does not exist and the citation was
  wrong.** RFC-0047 is callback groups; `ComponentNode` owns exactly one
  `Node node_` (`component_node.hpp:734`); both subnode packages open with "ONE
  ComponentNode with TWO callback groups". Several named nodes per IMAGE is real
  and belongs to the executor under **RFC-0046** (`Executor::create_node` /
  `node_builder`), one node per component. Preserving a capability that is not
  there would have added a constructor and a ledger row for nothing — RFC-0089
  "The several-named-nodes capability, corrected" carries the measurement, and
  "W4's RFC-0047 acceptance criterion is deleted, not weakened" below is the
  same ruling reached independently.

  **Amended 2026-09-09 — three resolutions the migration was missing.**

  **(a) The ours-only creation family takes the `_in` NAME, not just its
  signature.** On the merged type a value-returning `create_publisher(const
  char*, const QoS&)` and upstream's `shared_ptr`-returning
  `create_publisher(const std::string&, const QoS&)` are overloads, and a string
  literal binds OURS — array-to-pointer decay is a standard conversion, reaching
  `std::string` needs a user-defined one, and the standard conversion wins.
  Measured on gcc 11.4 and clang 14 at `-std=c++14` and `-std=c++17`: four for
  four, no diagnostic. So a ported line compiles and gets a different type,
  lifetime and failure channel. RFC-0089 §"The `_in` rule" is the general form;
  for W4 it means the migration renames rather than overloads, onto the suffix
  the tree already spells (`node.hpp:878`, `:909`, `:926`). The out-ref
  `create_*` family is NOT affected — it differs in ARITY, so the compiler names
  the line.

  **(b) The two inline members split, and they split for different reasons.**
  Measured on the layout `component_node.hpp` declares:

  | member | bytes | after |
  | --- | ---: | --- |
  | the sticky error latch (`has_error_` / `error_what_` / `error_code_`, `:747-749`) | **24** | stays, UNCONDITIONAL |
  | the inline timer pool (`Timer timers_[NROS_COMPONENT_MAX_TIMERS]`, `:735`) | **192** at the default 8 | a template parameter, defaulting to **0** |

  The latch stays because on a `-fno-exceptions` target it IS the error channel,
  not a diagnostic convenience: it is the only route by which a `create_*`
  failure inside a constructor reaches the generated entry, which reads `ok()`,
  `error_what()` AND `error_code()` and returns the code
  (`packs/entry/cpp/node_body.jinja`). RFC-0089 correction 5 also makes it
  STICKY — false if the node failed to create *or* if any later `create_*` /
  `declare_parameter` failed — so making it conditional would make the error
  channel a build configuration, which is the shape RFC-0089 refuses everywhere
  else.

  The pool goes because RFC-0089 correction 6 already removed its reason: it
  exists only for the storage-LESS `create_wall_timer` overload, and with the
  out-ref form the caller owns the cell. Nodes that never use the storage-less
  overload should pay nothing.

  **A default of zero is a real constraint on the implementation, not a knob
  value.** `Timer timers_[0]` is not ISO C++ (issue 1131, and the `#error` floor
  at `component_node.hpp:144` from issues 1015/1167), so the parameter must
  select a base or member that HOLDS NOTHING at `N == 0` rather than a
  zero-length array — an empty specialisation, not a smaller array. The
  `NROS_COMPONENT_MAX_TIMERS` macro and its `#error` floor (`:144`, issues
  1015/1167 — the guard sits BELOW its own default or it fires on every build)
  move or die with the header, in this commit. The ledger row
  `cpp:ComponentNode::create_wall_timer` names the pool in its reason and is
  re-read at the same time.

  **(c) The freestanding acceptance was two claims and only one of them is a
  check.** Splitting them, because a check and a port do not belong in one
  acceptance:

  * *stays here:* **the merge keeps the freestanding compile probe green** —
    `check-cpp`'s `-nostdinc++` header-parse lane (ThreadX `cxx-compat`, and
    Zephyr's minimal libcpp when the west workspace is present) and
    `check-cpp-capability-layout`'s freestanding arm. This is a regression
    check on work W4 does, and it can fail on the day W4 lands.
  * *moves to W12:* **a subnode package BUILDS for a freestanding target.**
    (W12 LANDED 2026-09-11 — FreeRTOS/mps2-an385, no `nros-cpp` change needed;
    see W12 below.)
    Measured, **no embedded fixture derives `ComponentNode` today** — the only
    `SHAPE rclcpp` packages are the two `subnode_pkg`s and every fixture
    consuming them is `platform = "linux"` (RFC-0089 correction 8). So this is
    a new port, not a check on an existing one, and holding W4's acceptance
    open on a fixture nobody has written is how a work item stops being
    landable.

  **Gates whose subject W4 deletes are retargeted in the same commit**
  (RFC-0089 §"A gate whose subject disappears passes vacuously"): the
  `declared_qos_depth.cpp` / `_probe.cpp` pair (already an acceptance below) and
  `check-cxx-standard-floor`'s docstring, which names `component_node.hpp`'s
  `if constexpr` as the reason the floor is 17. Each needs its negative control
  re-run, not merely its pattern moved.

* **W5 [cpp] — `get_logger()` follows ROS 2. DONE.** The `"nros.compat"` sentinel is
  replaced by a logger named for the node (RFC-0089 decision 1).
  *Acceptance:* two nodes in one image emit records under distinct logger names.
  *Met:* the accessor is built from the node's name and `one_node_type.cpp`
  pins that plus the `nros_logger_t` conversion that kept every native call
  site compiling. The runtime half is asserted on captured `Record`s by
  `two_nodes_on_one_executor_emit_under_their_own_names` (nros-node: two nodes
  on one executor) and `two_cpp_nodes_emit_under_their_own_names` (nros-cpp:
  the accessor `rclcpp::Node::get_logger()` calls). Writing them measured that
  the claim was FALSE — three of the four node-logger accessors resolved every
  node to `DEFAULT_LOGGER`; see "W5's runtime half landed, and the claim was
  FALSE when it was written" above.

* **W6 [loudness] — the two items the design creates. DONE (first half; second half is documentation, as the item allows).** `[[nodiscard]]` on
  `Result` (`NROS_NODISCARD` for C++14) so a discarded `rclcpp::init(argc,
  argv);` warns; and a story for a hand-written `main` that never checks
  `Node::ok()`.
  *Acceptance:* a TU that discards `init()`'s result fails a `-D warnings`
  lane. The `ok()` half may end as documentation — if so, say so in the book
  rather than leaving it implied.
  *Met:* `NROS_NODISCARD` on both halves of the channel — `Result` and
  `ResultOf<T>`, and the deprecated `Expected<T>` that still names it; measured
  blast radius
  inside the headers was exactly one discard. **The `ok()` half IS
  documentation**, and it is now stated on BOTH pages a hand-written `main`
  reaches: `book/src/getting-started/porting-a-cpp-node.md` §"`Node::ok()` —
  nano-ros cannot throw, so YOU have to ask" (landed with the merge) and
  `book/src/getting-started/first-node-cpp.md` §"If you write the
  `rclcpp::Node` constructor instead". The second was the real gap — the
  first-node page teaches a hand-written `main` and showed only the out-ref
  `nros::create_node` + `NROS_TRY_RET` form, whose `Result` IS checked, so a
  reader who wrote the rclcpp constructor instead was never told there was a
  question to ask. Also in `Node`'s constructor doc and the ledger row
  `cpp:Node::ok`. A compiler cannot force a hand-written `main` to ask.

* **W7 [migration] — `nros::Node` deprecated. LANDED 2026-09-11.** The item
  could not mean what it said until the namespace flipped, because a
  deprecation attaches to the ALIAS and the alias was `rclcpp::Node` — the name
  a user is supposed to write. Both halves landed together: the flip (the class
  is `rclcpp::Node`, `nros::Node` is the alias, the tenth of RFC-0089's table)
  and then the deprecation WITH the migration in one commit, which is what this
  doc's "Not in scope" requires. 258 in-tree C++ spellings migrated; the
  attribute is unconditional, and the evidence for not arming it is in the "W7
  LANDED" section above. Probe:
  `packages/api/nros-cpp/tests/compile/node_deprecation_probe.cpp`, greped for
  the replacement. `one_node_type.cpp` is the one TU deliberately left
  un-migrated — its subject IS both spellings naming one type — and its lane
  line carries `-Wno-deprecated-declarations` saying so.
  DELETION is a later wave: the alias stays for at least one release, for the
  out-of-tree copies of `examples/templates/**` that `nros-v0.5.0` shipped.

* **W11 [rust] — the `spin` family takes upstream's names. LANDED 2026-09-09.**
  Queued here as W12 while it was still work; `main` landed it as W11 before
  this branch rebased, so the number and the record are `main`'s. RFC-0089
  §"The `spin` family: upstream's names get upstream's contracts" settles the
  shape and the section "W11 [rust] — the spin family takes upstream's names —
  LANDED 2026-09-09" at the end of this file records what shipped, including
  the two things the plan here did not have: `SpinOptions::poll_interval` as a
  hand-written `Default` (the derive is `Duration::ZERO`, a busy-poll that
  passes every functional test) and `stop_on_first_error`. This closed the
  SEVENTH porting difference W10 recorded and did not own.

  One item the landed section leaves owed and this branch inherits: the port
  test `packages/testing/nros-tests/tests/rclrs_talker_port.rs` arrives with
  W10 (PR #753), not with `main`, so its `EXPECTED` entry for line 10 becomes
  `executor.spin(SpinOptions::default())?;` once both are in one tree. The
  RENAME half of that difference is gone; the COUNT does not drop, because ours
  is `?` where upstream is `.first_error()?` — the no-allocator divergence, not
  a naming one.

* **W12 [cpp, fixture] — a subnode package builds for a freestanding target.
  LANDED 2026-09-11 (issue 1247 resolved).**
  Split out of W4's acceptance on 2026-09-09, because it is a NEW PORT and not a
  check on existing work. Measured: no embedded fixture derives `ComponentNode`
  today — the only `SHAPE rclcpp` packages are the two `subnode_pkg`s under
  `examples/workspaces/realtime-cpp{,-subnode-portable}` and every fixture
  consuming them is `platform = "linux"` (RFC-0089 correction 8). The capability
  itself is proved — correction 1 compiled a derivation under
  `-std=c++14 -fno-exceptions -fno-rtti -ffreestanding -nostdinc++` against the
  ThreadX shim, with `__is_polymorphic` false for base and derived — so what is
  missing is a consumer, which is exactly why this is a work item and not an
  assumption.
  *Acceptance:* one subnode package has a `fixtures.toml` row on a non-`linux`
  platform and BUILDS there; the fixture's coordinate is in a lane
  (`row_coord()` / `row_artifact_root()`), so it is neither unattributable nor
  silently skipped. A fixture that only ever resolves STALE is not this
  acceptance met — read the `probe:` lines.

  **LANDED 2026-09-11 — and the port needed no `nros-cpp` change, which is the
  one result worth saying first.** The capability was a recorded measurement
  with no consumer; adding the consumer neither contradicted nor extended it.
  The build is `workspace-cpp-freertos-realtime-subnode-portable`, FreeRTOS on
  mps2-an385 (`arm-none-eabi-g++` 13.2, `thumbv7m-none-eabi`), and it compiled
  and linked on the first attempt.

  **Platform chosen from what the workspace already provisions**, as issue 1247
  asked: the tree has a C++ realtime FreeRTOS/mps2 family
  (`workspace-{c,cpp}-freertos-realtime`), so the toolchain, the board
  descriptor, the netstack and the SDK keys were all in place and the port added
  no provisioning. It is also the only candidate that is genuinely FREESTANDING
  in the sense correction 1 measured — `threadx-linux` is a Linux process and
  would have proved much less.

  **The workspace is `realtime-cpp-subnode-portable`, not `realtime-cpp`**, a
  departure from issue 1247's "smallest path". Two reasons, both about what the
  fixture then measures. `realtime-cpp` already spends `[image.freertos]` on its
  3-node `configure`-shape launch, so a subnode image there is a *second* image
  over a workspace whose other packages come along for the configure; the
  portable workspace holds exactly `subnode_pkg` + `deploy_bringup`, so the
  image is the subject and nothing else. And the portable workspace exists to
  prove the RFC-0047 coupling is deploy-side — which is exactly the claim the
  port re-measures against a cross toolchain: `src/subnode_pkg/` is BYTE-FOR-BYTE
  unchanged, and the whole port is `[tiers.fast.freertos] priority = 3` /
  `[tiers.bulk.freertos] priority = 1` (RFC-0079 `pool.app = [1, 3]`, below the
  transport band at 4) plus an `[image.freertos]` block and a `[board_config]`
  row.

  **What the generated entry turned out to be, and it is not the siblings'
  shape.** A node whose callback GROUPS span tiers cannot take `run_tiers` —
  per-tier setup functions construct whole *nodes* — so `Plan::executor_shape`
  returns `ExecutorShape::SchedContexts` and the entry is
  `FreertosBoard::run_components` plus
  `nros_cpp_create_sched_context_from_policy` ×2 /
  `nros_cpp_bind_node_name_sched` / `nros_cpp_bind_group_sched` ×2, against the
  `FreertosBoard::run_tiers` that `workspace-{c,cpp}-freertos-realtime` emit. So
  the row covers a second embedded executor shape, not a third copy of the first
  one. The two sched contexts carry `os_pri` 3 and 1 — the tier table reached
  the bake.

  *Measured on the linked image* (`arm-none-eabi-size` / `-nm -C`):
  text 528 492, data 3 168, bss 3 629 616. `subnode_pkg::SubNode::SubNode(
  nros::NodeHandle)` is defined; both group-bound timer trampolines
  (`create_timer_in_group<…::on_ctrl>`, `…::on_telem>`) are present; and the
  image carries **ZERO** `vtable for` / `typeinfo for` symbols — RFC-0089
  correction 1's `__is_polymorphic` claim re-measured at LINK scope over a whole
  firmware image rather than as a `static_assert` in one TU.

  *Lane:* `row_coord()` = `freertos,cpp,zenoh`, which `lane-coords` puts in
  **tier2-nightly** (pairwise) and not in tier 2 (1-wise picks `freertos,c,zenoh`) —
  the same lane as its `workspace-cpp-freertos-realtime` sibling.
  `row_artifact_root()` is `<ws>/build/freertos-zenoh-mps2-an385-freertos/cmake`,
  shared with no other row. Verified in both directions: under tier-2 coords the
  resolver prints `[SKIPPED:lane] … is at coordinate freertos,cpp,zenoh`, and
  `fixtures-manifest.py list-workspaces --coords-from <nightly>` contains the row.

  *Not STALE, and that is a measurement.* A workspace row resolves through
  `require_prebuilt_workspace_binary`, whose verdict is the
  `.nros-workspace-fixture.<id>.inputsig` comparison — NOT the mtime probe in
  `fixtures::staleness`, so there is no `probe:` accounting line on this path and
  the acceptance's wording does not literally apply. What was checked instead is
  the same property from both sides: appending one line to `SubNode.cpp` turns
  all three tests into `BuildFailed("… is stale: …inputsig")`, and reverting it
  returns them to green with no rebuild. A verdict that can be flipped is not a
  default.

  *Consumer:* `packages/testing/nros-tests/tests/subnode_freestanding_link.rs`
  (3 tests) reads the ELF. **BUILD-ONLY — no `matrix::CELLS` row**, deliberately:
  the acceptance is a build, nothing boots the image under QEMU, and the
  coordinate is already modelled by the sibling `cell(FreertosMps2, Cpp, Zenoh,
  RealtimeTiers, Workspace, Runtime)`, so `fixture_rows_all_modeled_by_matrix`
  is satisfied without inventing a duplicate cell. Each assertion was falsified
  before landing (mutate the method name, the vtable filter, a sched symbol —
  each fails with its own message).

  *One thing the build surfaced that is NOT this item's:* the mps2 link emits
  `implicit declaration of function 'nros_freertos_net_register_drain_task'`
  from `packages/boards/nros-board-freertos/c/freertos_c_entry.c:196`. It is
  pre-existing and fires on every FreeRTOS image, not only this one.

## What stays invented — REVISED after review (2026-09-05)

The first version of this table had four entries. Reviewed against the recorded
upstream surface, **two were mislabelled** (RFC-0089 §"Review of the invented
parts"):

| item | verdict |
| --- | --- |
| `rclcpp::Timer` | **invention, kept flat. LANDED 2026-09-08 (phase-430 W7): the HIERARCHY is deleted, and the `TimerBase` NAME is KEPT — not withdrawn, and not deprecated (2026-09-09).** This row twice said otherwise. "Withdrawn" was written believing nothing used `TimerBase`; three source files and three doc snippets did, one of them vendored UNMODIFIED to show upstream source compiling. "Deprecated for one release" was the next answer and is also wrong: `just colcon-parity` — the only lane that builds our templates against REAL rclcpp headers — reported `'Timer' in namespace 'rclcpp' does not name a type` the moment the alias went, because upstream has `TimerBase` and no `Timer` while we have `Timer` and no `TimerBase`. Without the alias the intersection of "compiles under real ROS 2" and "compiles under nano-ros" is EMPTY for any ported node holding a timer, and a deprecation would promise to break that on purpose. RFC-0089 "The alias rule" states the general form; `timer.hpp:274` carries the measurement. The hierarchy stays refused, and the study of WHY follows unchanged. Studied: upstream's hierarchy exists so its executor can hold a type-erased handle while the timer stores its functor inline. Neither reason survives — our `Timer` is a HANDLE and the callback lives in the arena as a raw fn ptr, so the executor never holds a C++ timer and a base would carry a vtable NO dispatch uses. And `GenericTimer`'s clock parameter corresponds to ROS-time/simulated-time timers, which we do not have: `create_timer` takes no clock and the executor schedules on one monotonic `nros_platform_clock_ns`. Inventing our own one-leaf hierarchy would reproduce the same empty promise. `create_wall_timer` already carries the accurate verb. |
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
  **DONE 2026-09-07.** `init.rs` is ungated; `Context` (`alloc` floor — its
  `locator`/`rmw` are owned `String`s), `ContextSource { Env, Launch, Baked }`,
  `InitError` (+ `DomainIdOutOfRange`) and `InitOptions` compile on every
  target; `env` gates only `read_env_context` / `init` / `init_with_args` /
  `init_with_launch*` / `Context::new`. ONE `option_env!` site for the pair:
  `Context::baked()`, with `from_baked(locator, domain)` as the injectable
  parse and `with_locator_override` for the harness's `-testargs` override —
  and because a leaf's `cargo:rustc-env` never reaches a dependency's build,
  `nros`'s own `build.rs` now calls `nros_zephyr_build::bake_nros_config()`
  (the issue-0460 `$DOTCONFIG` channel). Both entry macros call
  `$crate::Context::default_from_env()` then `ctx.config(name)`; the
  `nros::main!` Zephyr arm thereby GAINS the baked domain it had dropped
  (issue 0161's class — it ran domain 0 whatever Kconfig said). Measured:
  `cargo test -p nros --features env,std --lib init` 19 passed;
  `--no-default-features --features alloc --lib init` 9 passed (the `alloc`
  floor, `freestanding_default_from_env_is_the_bake` among them); clippy
  `-D warnings` clean at both floors and for `--all-targets`. The Zephyr
  `examples/zephyr/rust/talker` leaf (`zephyr_component_main!`) checked with
  the west build's own recorded cargo step against this tree
  (`aarch64-unknown-none`, `DOTCONFIG` from the `build-rust-talker-zenoh`
  configure): `nros`'s build-script output carries
  `rustc-env=NROS_LOCATOR=tcp/127.0.0.1:7400` / `NROS_DOMAIN_ID=0`, so the
  bake reaches the constructor. Ledger: `rust:Context::default_from_env`
  `rename` -> `divergence`+`adopt`; `rust:init` -> `extension` (the C++
  anchor); `rust:Context::from_env` / `rust:Context::new` / `rust:InitOptions`
  -> `divergence`+`adopt-bounded`; new `rust:Context::baked` and
  `rust:ContextSource::Baked` (`extension`).

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

  **OWNED 2026-09-09: that later wave is W11, and it LANDED the same day.**
  RFC-0089 §"The `spin` family" settles the shape — `spin(SpinOptions) ->
  Result<(), NodeError>` and `spin_forever` for the diverging `-> !` form — and
  W11 landed it on `main`, at which point the port writes `spin` and this
  difference is CLOSED. The count of differences does not drop with it: the
  ported line still differs, because ours is `?` where upstream is
  `.first_error()?`.

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
(`get_logger`). W0, W7 and W8 are landed.


## W11 [rust] — the spin family takes upstream's names — LANDED 2026-09-09

`Executor::spin` meant `spin(Duration) -> !`. That squats on rclrs's name with a
different contract, so a ported rclrs `main` had to write `spin_blocking` — and
the `rclrs_talker_port` measurement (PR #753) counts it as the ONE edit on the
tutorial's happy path that no real difference forces. Every other line the port
changes is a real difference the compiler names: the crate a glob comes from,
the `main` error type, a `mut`, a `?` where upstream has `.first_error()?`.

Take upstream's names; keep the RTOS-specific forms under honest ours-only ones.

| verb | signature | disposition | was |
| --- | --- | --- | --- |
| `spin` | `(&mut self, opts: SpinOptions) -> Result<(), NodeError>` | **adopt-bounded** — rclrs's name, argument and meaning | `spin_blocking` (27 sites migrated) |
| `spin_once` | `(&mut self, timeout: Duration) -> SpinOnceResult` | ours-only, unchanged — the tick primitive; a blocking wait with a budget, which no upstream has (RFC-0089 §2) | — |
| `spin_some` | `(&mut self, max: Duration) -> Result<(), NodeError>` | **adopt-bounded** — rclcpp's drain verb, NEW | — |
| `spin_forever` | `(&mut self, opts: SpinOptions) -> !` | **extension** — a bare-metal entry has no shutdown source and nothing to return to, and `!` drops the epilogue | `spin(Duration) -> !` (2 in-file sites) + `spin_default()` (0 sites, deleted) |

**Nothing is deprecated, because nothing needed to be.** The two old spellings
were reachable only from inside this tree — `spin_blocking` at 27 sites (18 of
them doc comments and test names), the `-> !` `spin` at 2, both inside
`spin.rs` — so the old names are gone outright rather than carried as
feature-armed `#[cfg_attr(…, deprecated)]` aliases (PR #753's precedent, needed
only because this workspace's `-D warnings` makes a deprecation a hard error at
every site). `spin_default()` had ZERO call sites and its 50 ms is now a field.

**`SpinOptions` is where the divergence sits**, deliberately, so a ported file
meets it in one type it already names instead of on the `spin` line:

* `poll_interval: Duration` (default `DEFAULT_SPIN_POLL_INTERVAL`, 10 ms — the
  value `spin_blocking` carried as a private const). RFC-0002 makes this a poll
  loop, so the quantum is real and is the granularity at which `cancel()`,
  `timeout` and `max_callbacks` are OBSERVED. rclrs's executor has no such
  quantum and no such field. `Default` is hand-written: `Duration::default()`
  is ZERO, and the derive would have made every `SpinOptions::default()` a
  busy-poll that still passed every functional test
  (`spin_options_default_poll_interval_is_the_named_constant` is the guard).
* `stop_on_first_error: bool` (default `false`). rclrs's `spin` returns
  `Vec<RclrsError>` and the caller writes `.first_error()?`; there is no
  allocator, so ours returns the FIRST error and a one-error channel has to say
  when it stops looking. The value is REAL — `spin_once_capturing` threads the
  failing callback's `TransportError` out through the caller's frame, so
  nothing was added to `SpinOnceResult` (which is `Copy` and public) or to
  `Executor` (which every no-alloc image pays for).

### `spin_some` did NOT absorb the period verbs, and the reason is not scope

`spin_period` / `spin_one_period` / `spin_one_period_timed` stay ours-only.
They are RFC-0002's fixed-period dispatch and their contract is the opposite of
a drain's:

* **They PACE.** `spin_period` sleeps to an *accumulated* deadline
  (`next += period`, never `now + period`) so a 100 Hz loop does not drift;
  `spin_one_period_timed` measures the pass and sleeps off the remainder and
  reports `overrun`. `spin_some` executes what is ready, returns, and never
  sleeps — folding the two would mean a `spin_some` that blocks, which is the
  one thing the verb promises not to do.
* **`spin_one_period` returns a VALUE the caller acts on** — `remaining_ms`,
  for a caller with no clock to perform the sleep itself. `spin_some`'s
  `Result<(), NodeError>` has nowhere to put that, and widening it would make
  the drain verb's return type about pacing.

So they keep their names and their `bucket: ours-only` ledger row. rclrs has no
counterpart for any of the three: it spins as fast as work arrives.

### Envelopes, both stated in the doc comments

* `spin_some(max)`: `max` is checked BETWEEN passes, so it is a floor on when
  the call returns and not a ceiling — one long callback overruns it, and there
  is no preemption here to make it otherwise. `Duration::ZERO` means no limit
  (rclcpp's own default) and is the spelling a clockless build must use; a
  non-zero `max` with no clock is `NodeError::NotInitialized`, issue 0709's
  rule at its third site. It re-drains until a pass does no work — rclcpp's
  `spin_all` answer rather than its `spin_some` one — because our `spin_once`
  already dispatches the whole ready set in one pass, so the only work a single
  pass could leave behind is work a callback enqueued.
* `spin_forever(opts)`: only `poll_interval` and `stop_on_first_error` can mean
  anything; `timeout` / `only_next` / `max_callbacks` are ENDING conditions and
  this verb does not end. Passing one logs at `LOG_ERR` on entry rather than
  being dropped — a discarded ending condition is exactly the "compiles and
  differs" case RFC-0089 exists to make loud, and a `-> !` signature has no
  other channel.

### Gating

`spin_once`, `spin_some` and `spin_forever` are ungated and reach a `no_std`
no-alloc target; `spin` and the period verbs keep the `alloc` gate they had.
`SpinOptions`'s import in `spin.rs` stopped being `#[cfg(feature = "alloc")]`
for `spin_forever`'s sake.

### What this leaves for the next wave

* **The port test.** `packages/testing/nros-tests/tests/rclrs_talker_port.rs`
  is on PR #753's branch (`phase-427-w10-executor-opens-node-named`), NOT on
  main, so it was not reachable from this work. Its `EXPECTED` entry for line
  10 must be re-stated once both land: the ported line becomes
  `executor.spin(SpinOptions::default())?;`, so the RENAME half of that
  difference is gone. **The count does not drop** — the line still differs,
  because ours is `?` where upstream is `.first_error()?`, which is the
  no-allocator divergence and not a naming one. Whoever rebases owes that
  comment the edit; the `assert_eq!` on the changed-line SET still passes
  unchanged.
* **C++.** Its spin family was already conformed by issue 0338
  (`Executor::spin` blocks until shutdown, the bounded form is `spin_for`) and
  RFC-0089 §2 settled that `spin_once(timeout)` STAYS as a kept invention. What
  is missing there is `Executor::spin_some` as an ADDITION — the ledger row
  `cpp:Executor::spin_some` had `status: blocked-needs-decision` naming a
  decision since taken twice, and now reads `blocked-needs-cpp-wave` with the
  work spelled out. One genuine sweep gap was found and filed rather than
  fixed, because `nros.hpp` is the header PR #755 is rewriting: **issue 1245** —
  issue 0338 renamed `Executor::spin(ms)` to `spin_for` and left the FREE
  `nros::spin(duration_ms, poll_ms)` bounded overload behind.

Two items joined after that move (2026-09-09), and `main` has since settled how
they are numbered. The `spin` family rename this branch queued as W12 LANDED as
**W11** — the section above is `main`'s, written when it landed, and it is the
same shape RFC-0089 §"The `spin` family" settles. So the item split out of W4's
acceptance, the freestanding subnode port, is **W12**; it depends on W4, and W11
depended on neither and is done.

## W1 is REVERSED by RFC-0096 / phase-442 (2026-09-09)

W1 moved to phase-438 on 2026-09-08 and is now reversed outright.

**What survives.** "One node type" holds where this phase aimed it:
`ComponentNode` goes, and the core has exactly one node type. That was the
owner's stated intent — the node here is always linked into the final image,
so there is no dynamic-composition distinction to model.

**What does not.** `void* hosted_` was the artefact of trying to give ONE type
TWO shapes — a freestanding layout and a hosted one — and a type with
capability-gated members is a type with two layouts. That is the defect
RFC-0096 removes, not a mechanism to keep. The measurement that made it look
necessary (`sizeof(rclcpp::Node)` invariant at 3728 both ways) was real; the
need for it was not, once the API stopped having two shapes to reconcile.

**W2–W7 should be re-read before being implemented.** Several assume the
hosted/freestanding split phase-442 removes:

* **W2 (construction)** — "upstream's `std::string`/`NodeOptions` constructors
  hosted" is no longer a category. `NodeOptions` becomes freestanding as it
  stands (22 of its 23 members need no `std`), so there is one constructor set.
* **W3 (one name, two signatures)** — the acceptance requires a ported
  `create_publisher<M>("chatter", 10)` to compile hosted and FAIL freestanding.
  Under RFC-0096 it must compile EVERYWHERE; that acceptance inverts.
* **W4 (`ComponentNode` deleted)** — survives, but its RFC-0047 acceptance was
  already found vacuous: "one component, several named nodes" does not exist in
  the tree.
* **W5 (`get_logger` follows ROS 2)** — unaffected.
* **W6 (loudness)** — its `NROS_NODISCARD` half landed as W8. The `ok()` half
  is unaffected.
* **W7 (`nros::Node` deprecated)** — LANDED 2026-09-11, before RFC-0096, and it
  is what RFC-0096 D1 would otherwise have had to do first: the class is in
  `rclcpp::` and the `nros::` spelling warns. Nothing here is re-read; the
  deletion of the alias is the wave RFC-0096 can assume.
