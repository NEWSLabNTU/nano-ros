# Phase 427 — one node type, named `rclcpp::Node`, compiling freestanding

**Status (2026-09-09). W1, W2, W3, W5, W6 and W8 LANDED (W5's runtime half and W6's book page closed 2026-09-09); W4 NOT STARTED; W7 blocked and re-scoped — see "What landed, and what it measured" below.** Implements RFC-0089 §"The node API, proposed
under the governing principle". Preconditions are met by phase-417: the
`pump()` blocker is gone, `check-cpp-capability-layout` measures the layout
rule, and the `-nostdinc++` lane can see a freestanding regression.

## What this is

Three C++ node shapes collapse to one type:

| today | after | status |
| --- | --- | --- |
| `nros::Node` — out-ref creation, freestanding, 103 files | one type, both spellings | **DONE** |
| `rclcpp::Node` — hosted, `shared_ptr`, derivable | the same type, hosted members out of line | **DONE** |
| `nros::ComponentNode` — derivable, entry-supplied handle, 5 dirs | DELETED; its handle becomes a constructor | **NOT STARTED** |

Both spellings name ONE class — `std::is_same<rclcpp::Node, nros::Node>::value`
is asserted by `tests/compile/one_node_type.cpp`. Which of the two is the
definition and which the alias is settled the OTHER way from RFC-0089's end
state, for a measured reason recorded below; neither is deprecated yet, so the
remaining call sites are still optional to migrate.

## What landed, and what it measured (2026-09-08)

W1, W2, W3, W5 landed together (one merge is not divisible), W6/W8 landed
first because the merge needed the macro, and phase-430's W6 and W7 landed with
them because they are the same headers. W4 is untouched. W7 turned out not to
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

### The namespace direction is the opposite of the RFC's end state, and it is measured

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

### W7 cannot mean what it says, and the blocker is the line above

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

## W4 — NOT STARTED, and what it has to decide first

W4 is the largest item and it is untouched. Attempting it inside this change
would have shipped a half-merged `ComponentNode`, which is worse than either
half. What the next person needs, measured rather than assumed:

**The delta is 14 members, not a rename.** Already on the merged type and
deletable outright: `get_name`, `get_namespace`, `get_logger`, `node()`
(now the identity), `create_callback_group`, the scalar parameter facade and its
`std::string` overloads, and `adopt_launch_seed_` (whose twin
`rclcpp::detail::adopt_executor_param_seed` already lives in `nros.hpp` — keep
ONE). Genuinely missing, and each needing a decision:

1. **The `create_publisher` RETURN SHAPE COLLIDES, and the merge is what creates
   the collision.** `ComponentNode::create_publisher<M>(const char*, const QoS&)`
   returns `Publisher<M>` BY VALUE; the merged node's
   `create_publisher<M>(const std::string&, const QoS&)` returns a
   `shared_ptr`. On one type, `create_publisher<M>("chatter", 10)` — a string
   LITERAL — binds the `const char*` overload and returns by value, so a ported
   `auto pub = node->create_publisher<M>("chatter", 10); pub->publish(…)` stops
   compiling, or worse binds differently than the author expects. This is
   compile-and-differ manufactured by the merge itself, and it has to be
   designed away (drop the value-returning form, or give it a different verb)
   before anything else moves.
2. **The error latch is 24 unconditional bytes.** `has_error_` + `error_what_` +
   `error_code_` cannot live in the hosted block — the boot-halt mechanism
   (`NanoRosEntityInventory.cmake`, RFC-0044 Q2) is exactly what a freestanding
   image needs. Decide whether every node pays for it.
3. **The timer pool is 192 unconditional bytes.** `Timer timers_[8]` +
   `timer_count_`. Same question, larger number, and `NROS_COMPONENT_MAX_TIMERS`
   is a knob with an `#error` floor that would move onto the node.
4. **Do NOT add a virtual destructor.** Upstream's `rclcpp::Node` has one, but
   the generated entry placement-news each component and never destroys it
   through a base pointer, so a vtable here would be 8 bytes on every
   freestanding node with no dispatch that uses it — the same argument that
   deleted `TimerBase` one file over. Derivation works without it; only
   `delete base_ptr` is UB, and nothing does that.
5. `check_declared_depth`, the member-pointer `create_subscription` /
   `create_subscription_in` family, and the `std::vector<T>` parameter arm all
   move as-is.

**RFC-0047's "one component, several named nodes" DOES NOT EXIST.** This is a
correction, not a scoping note: `ComponentNode` owns exactly one `nros::Node`,
created once in its constructor, and `packages/cli/nros-cli-core/src/
entity_inventory.rs` states the invariant — "a `ComponentNode` constructor is
one `Node::create` is one node NAME". What the subnode packages actually
exercise is **one node with several NAMED CALLBACK GROUPS bound to different
sched contexts** (`create_callback_group` + `create_timer_in` +
`create_subscription_in`), which is RFC-0047's real subject and which the merged
node already carries. The phase table's row should say so; there is no
several-named-nodes capability to preserve.

**No subnode package builds freestanding today.** All three consuming
`fixtures.toml` rows are `platform = "linux"`. The workspace that HOSTS
`subnode_pkg` (`examples/workspaces/realtime-cpp`) already has nuttx, freertos
and zephyr rows, but they select the `configure`-shape packages via their own
launch files. The smallest path to W4's "one of them builds for a freestanding
target" is a new `[image.*_subnode]` pointing at `subnode_system.launch.xml`;
`subnode_pkg/CMakeLists.txt` carries no platform restriction.

**Four gates are keyed on the file by PATH or by namespace** and will need
moving with it: `scripts/api-parity.py`'s `CPP_TRANSLATION_UNITS` includes
`component_node.hpp` as its own TU and `extract_cxx` RAISES on any clang error,
so deleting the file is a hard red; `scripts/check-c-array-guard-probe.py` keys
a table on the literal path; `scripts/check-cxx-standard-floor.py` names it as
the C++17 floor's justification (`adopt_launch_seed_`'s `if constexpr` is the
tree's only one); and 17 ledger rows are keyed `cpp:ComponentNode::*`.

Codegen moves with it too: `entry.cpp.jinja` emits the include,
`node_body.jinja` placement-news the class from a `::nros::NodeHandle`,
`emit_cpp.rs` has the `is_rclcpp_node` branch, and two goldens record the
output.

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

* **W4 [cpp] — `ComponentNode` deleted. NOT STARTED.** See "W4 — NOT STARTED,
  and what it has to decide first" above: the `create_publisher` return shape
  COLLIDES on the merged type, the error latch and timer pool are unconditional
  bytes on every node, RFC-0047's "several named nodes" turns out not to exist
  (it is several named CALLBACK GROUPS), and four gates are keyed on the file by
  path or namespace. Its 5 directories move to the one
  type; RFC-0047's several-named-nodes survives as a documented ours-only
  capability on it. RFC-0044 is amended, not deleted — its Q2 boot-failure
  reasoning becomes `ok()`'s.
  *Acceptance:* zero `ComponentNode` in the tree; the RFC-0047 subnode packages
  build and run; **one of them builds for a freestanding target**, which is the
  test of whether the merged type still fits.

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

* **W7 [migration] — `nros::Node` deprecated, then deleted. BLOCKED, and the
  item does not mean what it says.** After the merge `nros::Node` is the
  DEFINITION and `rclcpp::Node` the alias (see the namespace-direction section
  above), so a deprecation would land on the name a user is supposed to write.
  What shipped: `NROS_CPP_DEPRECATED_MSG` exists and is used on the one
  ours-only name this phase retired, `nros::bind_timer`. The `Node` half waits
  on phase-428's namespace flip, and whoever lands it must migrate the 218
  in-tree `nros::Node` sites in the same commit — a bare attribute warns at all
  of them at once, which this doc's own "Not in scope" forbids.

## What stays invented — REVISED after review (2026-09-05)

The first version of this table had four entries. Reviewed against the recorded
upstream surface, **two were mislabelled** (RFC-0089 §"Review of the invented
parts"):

| item | verdict |
| --- | --- |
| `rclcpp::Timer` | **invention, kept flat — the `TimerBase` alias is WITHDRAWN.** `TimerBase` is a name that promises children, and aliasing a concrete class to it sells a taxonomy we do not have. Studied: upstream's hierarchy exists so its executor can hold a type-erased handle while the timer stores its functor inline. Neither reason survives — our `Timer` is a HANDLE and the callback lives in the arena as a raw fn ptr, so the executor never holds a C++ timer and a base would carry a vtable NO dispatch uses. And `GenericTimer`'s clock parameter corresponds to ROS-time/simulated-time timers, which we do not have: `create_timer` takes no clock and the executor schedules on one monotonic `nros_platform_clock_ns`. Inventing our own one-leaf hierarchy would reproduce the same empty promise. `create_wall_timer` already carries the accurate verb. **LANDED 2026-09-08 (phase-430 W7), with one amendment: the HIERARCHY is deleted and the NAME survives one release as a DEPRECATED alias for `rclcpp::Timer`. "Withdrawn" was written believing nothing in the tree used `TimerBase`; three source files and three doc snippets did, one of them vendored UNMODIFIED to show upstream source compiling. A retirement alias is the two-step this document prescribes, not the permanent ported alias it refused — see "What landed" above.** |
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
  ported `executor.create_node("talker")` has nowhere to go. RFC-0047's
  several-named-nodes is the existing capability this lands on.
  *Acceptance:* the rclrs talker tutorial ports with exactly two edits (`?` on
  `create_executor` and `spin`); `rust:Context::create_executor` and
  `rust:Executor::create_node` carry `adopt-bounded`/`adopt`.

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
* **W7 (`nros::Node` deprecated)** — subsumed: RFC-0089 already settled that
  `nros::` is phased out entirely and ours-only names take `rclcpp::` too,
  which RFC-0096 D1 makes structural.
