# Phase 417 — ROS 2 user-API adoption

**Status (2026-09-13). In flight. Implements RFC-0089. Re-measured against the
tree, because the line this replaces was dated 2026-09-04 and nine days of the
campaign had landed underneath it — including three stage-3 waves this very
document records as LANDED.** A status line that contradicts its own body is the
defect phase-419 W2's gate exists for; it caught none of this, because every
contradiction here was prose against prose.

| stage | state (measured 2026-09-13) | what it was measured against |
| --- | --- | --- |
| 0 — the instrument | **LANDED** | `disposition` is a live ledger field (16 `gap` rows carry one); `just check api-parity` runs `--require-disposition` (`just/check/lanes.just:47`) |
| 1 — cheap unblockers | **LANDED** | acceptance met; `cpp-port-minimal-publisher/src/minimal_publisher.cpp` is upstream's file |
| 2 — the node surface | **W2.b, W2.c, W2.d LANDED; only W2.a open** | `set_parameter<T>` on the node (`node.hpp:829`, `:846`); `create_service`/`create_client` (`node.hpp:734`–`775`); `Rate`/`WallRate` (`nros.hpp:1062`–`1125`). W2.a is issue 0793's **C half** — the legacy `nros_param_*` store is still exported and still disjoint. The C++ half closed in phase-426 W4 |
| 2b — graph forwarders | **LANDED** | zero `cpp:Node::*` graph rows remain `gap`; the 18 graph gaps are all `c:` and `rust:` |
| 3 — loudness | **W3.a, W3.c, W3.e, W3.f, W3.g, W3.h LANDED; W3.b and W3.d open** | see the stage-3 items; W3.f measured at `qos.hpp:743` (`ParametersQoS` reads the QoS table) and `options.hpp:295`–`315` (the inert setters `static_assert`) |
| 4 — our three languages agree | **OPEN — this is the remaining body of work** | 132 `gap` rows, below |
| 5 — C | **OPEN** | `c:` rows across `graph`, `service`, `pubsub`, `log`, `timer` |
| 6 — the rename | **BOTH STEPS LANDED** | `rclcpp_compat.hpp` is gone; `deprecate-legacy-names` is retired (`nros-node/Cargo.toml:48` records it in the past tense); the `nros_node_init` forwarder is retired (`rcl_compat.h:299`) |

**Stage 6 was never gated on stage 4 or 5, and it went first.** The old status
line said the rename was "gated on stage 3 AND on the structural blocker"; both
cleared, the rename landed, and stages 4 and 5 are what is left. So the campaign
did NOT run in its written order, and the document has to say so rather than let
a reader infer that stages 4–5 are also done because stage 6 is.

**The correction track is CLOSED.** Issues 1012 and 1022 are resolved and
archived; W-C1, W-C2 and W-C3 have nothing outstanding. Only 1042 (the rclrs pin
moved and nine rows went false) is still open on that track.

**What stage 0 revealed, and it reframes the phase.** Admitting the compat shim as
a fourth TU made the ported surface look better — `same` 84 → 110 — while the
defects were unchanged. `ParametersQoS` and `NodeOptions::use_intra_process_comms`
now correlate `same`: the first returns `QoS(10)` where upstream is `KEEP_LAST,
1000`, the second stores its argument and is never read. Correlation compares names
and shapes and neither differs, so **the instrument cannot see the defect it is
measuring.** A row can be `same` by shape and `refuse-loud` by disposition at once,
which is why `disposition` is not bookkeeping — it is the only thing between a
compatibility number and a false one.

Goal: a ROS 2 node written against rclcpp / rclc / rclrs compiles and behaves
against nano-ros, or fails loudly. **End state is that our API carries ROS 2's
names** — `nros::` stops being the spelling a user writes — and
`rclcpp_compat.hpp` is gone because its content moved into the headers it was
shimming.

This phase is also the **home for every correction, migration and retirement
job** in the API campaign: the tracks are listed below, and each names its
issue. An issue with no work item is an issue nobody is accountable for.

Two ordering principles, both from RFC-0089.

**The rename is cheap and cosmetic; the compatibility is the work.** The rename
lands last, after the shapes match, because before then it would relabel the
gaps without closing one — and would spend the property that makes mismatches
visible while the mismatches are still there.

**The Rust API is the implementation source of truth** (RFC-0019/0020). Every
item below carries *which layer implements this?*, and the order within an item
is Rust → FFI slot → C → C++. Ergonomics — aliases, forwarders, diagnostics,
conversions — may live in the wrapper; behaviour may not. Items marked
**[wrapper]** are pure delegation over behaviour Rust already has and is the
cheapest work here; items marked **[rust-first]** need the Rust side to grow
before either wrapper can expose anything, and planning them as C/C++ tasks
would mis-cost them by an order of magnitude.

## Stage 0 — measure the thing we are changing (issue 1020) — LANDED

**Issue 1020 is resolved and archived.** The text below describes the tree as it
was; both of the files it names are gone (`rclcpp_compat.hpp` deleted in stage 6
step A, `component_node.hpp` in phase-427 W4), which is a stronger outcome than
the item asked for: there is no separate shim left to admit as a fourth TU.
W0.b's `disposition` field is live and gated (W3.e).

It said: blocks everything. The C++ lane reads `nros.hpp`, `component_node.hpp`
and `nros.hpp -DNROS_CPP_STD`, and filters to namespace `nros`, so the 589-line
compat shim contributes zero rows. Every "how far are we" number is about the
native API rather than about what a ported file reaches.

* W0.a — admit `rclcpp_compat.hpp` as a fourth TU with namespace `rclcpp`, and
  decide the three questions issue 1020 records: does a `rclcpp::` alias
  resolving to a `nros::` type correlate as `same`; are "how close is the
  native API" and "what does a ported file hit" one surface or two; how is the
  shim's std-only reachability marked.
* W0.b — add a `disposition` field to the ledger (adopt / adopt-bounded /
  refuse-loud / absent) per RFC-0089's consequence section, and a gate that a
  `declined` row declares one.

**Acceptance:** the C++ report distinguishes native-API distance from
ported-file distance, and every `declined` row says what a porting user gets.

## Stage 1 — the cheap unblockers

Small, independent, and they unblock the first lines of nearly every ported
file. Roughly 75 lines total.

* W1.a **[wrapper]** — nested `SharedPtr` / `ConstSharedPtr` / `UniquePtr` on `Publisher`,
  `Subscription`, `Service`, `Client`, `Timer`, and on `rclcpp::TimerBase`.
  `detail::SharedPtrTrait` (`rclcpp_compat.hpp:96`) was written for this and is
  dead code — one occurrence in all of `packages/api/`, its own definition.
  Unblocks `rclcpp::Publisher<T>::SharedPtr member_;`, which is close to
  universal in rclcpp source.
* W1.b — **ALREADY DONE; verify and close.** `write_b8_alias_header`
  (`packages/cli/cargo-nano-ros/src/lib.rs:1858`) is called from three sites
  (`:1706,1748,1790`), and `rosidl-bindgen` has the same. This item was written
  from the ledger, which measures the native API and cannot see generated
  headers. Confirm on a fresh `nros sync` and strike it.
* W1.c **[wrapper]** — `NROS_CPP_STD`-gated `std::string` interop on `FixedString<N>`
  (`operator=`, conversion, `operator==`), plus ungated `size()`/`empty()`.
* W1.d **[wrapper]** — forward `now()`, `get_clock()`, `get_name()`, `get_namespace()` on the
  shim `Node`; alias `rclcpp::Time`/`Duration`/`Clock`. All four already exist
  on `nros::Node` (`node.hpp:217,223,249,260`).

W1.a and W1.c also gate stages 6's lifecycle and action work, which hand
entities back and therefore need the nested pointer types first.

**Acceptance:** `examples/templates/cpp-port-minimal-publisher/` compiles with
its source byte-identical to upstream's tutorial. Today its README claims
"verbatim" and three lines differ — W1.a and W1.c are exactly those three.
That claim becomes true or the stage is not done.

## Stage 2 — the node surface

Where a real node stops being a tutorial. Each item is independently useful.

* W2.a **[wrapper]** — **one parameter store. C++ DONE; the C half is all that
  is left, and it is the only open item in stage 2.** Rust always had the single
  store; this is thin-wrapper COMPLIANCE work, not new capability.

  **What closed.** The C++ side went in
  [phase-426](phase-426-parameters-rust-ssot.md) W4: `ComponentNode`'s private
  `ParameterServer` went with the type (phase-427 W4 merged the node types,
  `component_node.hpp` no longer exists), `rclcpp::Node`'s inline member is
  gone, and the standalone `nros::ParameterServer<Cap>` was DELETED rather than
  forwarded — the unfiled C++ twin this item said to file turned out to be a
  third store, and deleting it is what `check-example-parameter-stores` now
  keeps out of `examples/`.

  **What is open.** C still exports the legacy `nros_param_*` caller-storage
  store beside the executor-owned `nros_executor_*_param_*` family, and nothing
  joins them — issue 0793's C half, verbatim. Deliberate for now: phase-426 W5
  retained the export and moved every shipped example off it, so the defect is
  no longer SHOWN, only still reachable. Closing this item is a decision
  (retire the family, or make it a view onto the one store) plus the sweep.
* W2.b **[wrapper]** — **LANDED (the include half).** rclcpp-shaped
  `declare_parameter<T>` / `get_parameter<T>` / `set_parameter<T>` /
  `has_parameter` on the node reachable from the umbrella. The implementation
  already existed at `component_node.hpp:568-659`; the blocker was that
  `nros.hpp` did not include `component_node.hpp` (15 of 46 headers were
  unreachable from the umbrella). Fixed by an unconditional
  `#include "nros/component_node.hpp"` at `nros.hpp:62` — freestanding-safe,
  because the header's `<string>` use is gated on `NROS_CPP_STD` (issue 0112)
  and its placement-new shim on Zephyr's stub `<new>`. `ComponentNode`,
  `NodeHandle`, `nros::detail::report_component_failure` and the
  `NROS_SUBSCRIBE` / `NROS_COMPONENT` macros now all resolve from
  `<nros/nros.hpp>` alone (verified by compiling a TU that includes nothing
  else, against a negative control that includes only `node.hpp` and fails).
  **Two things this did NOT do at the time**, and BOTH have since closed —
  W2.b is now fully landed. What was outstanding: `set_parameter<T>` was absent
  (`ComponentNode` had no setter of any kind, and the only C++ setter was
  `ParameterServer<Cap>::set_parameter<T>` on the store), and the methods
  arrived under `ComponentNode` rather than on the node, so the six
  `cpp:Node::*_parameter*` ledger rows stayed `gap` on the native bucket.
  Measured 2026-09-13: `set_parameter<T>` is on the node
  (`node.hpp:829`, `std::string` overload `:846`), `ComponentNode` no longer
  exists (phase-427 W4 merged it), and the four `cpp:Node::*_parameter` rows
  were CLOSED AND DELETED in the 2026-09-11 truth pass. Stale restatements of
  the old blocker were swept 2026-09-07 (phase-427): `scripts/api-parity.py`,
  `docs/issues/README.md`, issue 0793, and seven ledger rows in `node.json` /
  `param.json`.
* W2.c **[wrapper]** — **LANDED.** `create_service` / `create_client` on the
  node (`node.hpp:734`, `:741`, `:753`, `:758`, `:765`, `:775`, in both
  poll-style and callback-style forms); `async_send_request` returns something
  the existing `spin_until_future_complete` accepts.
  `nros::Client<S>` and `nros::Future` already existed.
* W2.d **[wrapper, carefully]** — **LANDED** at `nros.hpp:1062`–`1125`, as a
  forwarder and not a sleep loop. One envelope is recorded there and is worth
  repeating: `Rate` and `WallRate` are the SAME clock here (both monotonic), so
  `WallRate` is faithful and `Rate` is `WallRate` under a second name — upstream
  measures `Rate` on ROS time. Original item follows.

  `rclcpp::Rate` / `WallRate` as a FORWARDER
  onto `nros::spin(remaining_ms, poll_ms)` (`nros.hpp:175`), which drives the
  executor. The obvious fifteen-line C++ class with its own sleep loop is both
  RFC-0020 violation class 2 (a polling loop that spins the executor from
  inside the wrapper) and the thing RFC-0021 forbids. Same capability, and only
  one of the two shapes is admissible.

**Acceptance:** a node that declares a parameter, calls a service and reads the
clock compiles unmodified. Measured by a new ported template, not by inspection.

## Stage 2b — graph forwarders **[wrapper]** — LANDED

`nros::Executor` already shipped the whole graph surface — `get_node_names`,
`count_publishers`/`count_subscribers`, the four `*_names_and_types_by_node`
forms, `get_{publishers,subscriptions}_info_by_topic` — over FFI that exists.
The 18 rows were open because `Node`/`LifecycleNode` did not forward to it, not
because the capability was missing. They forward now (`node.hpp:1051`, `:1084`,
and the `*_by_node` / `*_info_by_topic` block following), and a
`TopicEndpointInfo` value type exists.

**Do not read the "18 graph rows" in the ledger as this stage.** There are still
exactly 18 `gap` rows in `graph.json`, which is a coincidence of count: not one
of them is `cpp:Node::*`. Eight are `c:` (the four `*_names_and_types_by_node`
forms, the two `*_info_by_topic`, and `wait_for_{publishers,subscribers}`) and
ten are `rust:Node::*`. Those belong to stage 5 and stage 4 respectively. Most
also need the BACKEND to answer — Cyclone fills 1 of 12 graph slots, which is
[phase-444](phase-444-rmw-fix-up.md) W3.

## Stage 3 — the loudness pass (the safety gate)

RFC-0089's rule applied to everything already adopted. **This stage gates the
rename**: until it is done, taking more upstream names increases the number of
ways a ported program can compile and differ.

* W3.a — issue 1019: `RCLCPP_*_STREAM` discards its message; the family routes
  to a sink that is a no-op on every embedded target. Fix or refuse loudly.
  **LANDED 2026-09-11**, together with three more loudness items found beside it
  — see the issue table below for 1019, 1302 and 1303:
  * `cpp:RCLCPP_FATAL` — the family routes at `NROS_LOG_*`, carries the logger,
    and FATAL stops lowering to ERROR.
  * `cpp:Publisher::publish` — the one entry point on the class with no
    `initialized_` guard; it reinterpreted zeroed storage as an `RmwPublisher`.
    The class was swept, not just the site: it was the only unguarded one.
  * `cpp:Executor::spin_once` — the DEFAULT (10 ms where upstream blocks) and the
    `-1` SENTINEL (clamped to a 0 ms poll) were the two silent differences, and
    both are refused now: the signature half with a `static_assert`, the value
    half at the call. The drain-all/execute-one question is NOT closed by this
    and belongs to `cpp:Executor::spin_some`, per RFC-0089's own instruction not
    to read its `spin_once` decision as settling it.
  * `cpp:Client::wait_for_service` and its sibling
    `rclcpp_action::Client::wait_for_action_server` — a 5000 ms default standing
    in for upstream's "wait forever". One refusal concept, two sites, fixed
    together.

  Two things this work measured that the next loudness item needs: a runtime
  refusal longer than ~160 bytes is DROPPED by `nros_log`'s format buffer rather
  than truncated (so it reaches the console as a lone ellipsis —
  `rclcpp::detail::RUNTIME_REFUSAL_MAX` now `static_assert`s against it), and a
  `decltype(...)` assertion cannot see a refusal at all, because it does not
  instantiate a template body. `spin_verbs.cpp` carried exactly that shape.
* W3.b **[rust-first]** — **HALF DONE; the honouring half is one of the three
  stage-3 items still live.** `rclcpp::init(argc, argv)` dropped `--ros-args`
  silently, turning a remap into a wrong-topic bug at runtime. The REFUSAL
  landed with W3.c — the two-argument form now fails loudly in all three
  languages (C++ measured at `nros.hpp:252`: "The TWO-ARGUMENT form is
  REFUSE-LOUD (RFC-0089 W3.b)") — so a ported program no longer differs
  silently. What is still
  owed is HONOURING them on posix boards, tracked as the ledger row
  `rust:init_with_args`. That is remap resolution — name construction, RFC-0020
  violation class 4 — so the parser belongs beside `nros::resolve_name`, not in
  a wrapper.
* W3.c — **LANDED.** The REFUSE-LOUD diagnostics are written: **69 rows
  collapsed into 17 messages**, a refusal being per-concept and not per-symbol.
  Measured 2026-09-13, the `NROS_RCLCPP_REFUSE_*` family has 51 uses across
  eight headers, 22 of them the shared `NodeOptions` message.
* W3.f — **LANDED. Both live inversions are gone.** They were: `ParametersQoS()`
  returning `QoS(10)` where upstream is `KEEP_LAST, 1000`, and ten `NodeOptions`
  setters storing their argument and never reading it ("intentionally inert
  today"). Today `ParametersQoS` reads the one QoS table
  (`qos.hpp:743`, `detail::qos_table::parameters()`), and every `NodeOptions`
  option — setter AND getter — is a `static_assert` refusal
  (`options.hpp:295`–`315`); the type and its default constructor survive,
  because `rclcpp::NodeOptions{}` claims nothing. The getters refuse alongside
  the setters deliberately: a getter reporting `false` for a policy nothing
  implements is a claim about a switch that does not exist. Adopting the named
  QoS profiles at all was ADOPT **only** with each profile's values transcribed
  from upstream and a table-driven test against `rmw_qos_profile_*` — the shim
  got two wrong, which is the evidence for why the test was not optional.
* W3.d — **OPEN.** Document the ADOPT-BOUNDED envelopes in the doc comments,
  starting with `create_wall_timer` (period accuracy is the spin cadence,
  because the timer is polled rather than driven by the executor). The item as
  first written cited `Node::pump()`, which no longer exists; the envelope it
  describes does. Several envelopes have since been written at their call sites
  (`Rate`/`WallRate` at `nros.hpp:1088`, `cpp:Publisher::publish`'s
  `adopt-bounded` row) — what is owed is the SWEEP, not a first instance.
* W3.e — **LANDED.** The gate is `scripts/api-parity.py --check
  --require-disposition`, run by `just check api-parity`
  (`just/check/lanes.just:47`). No adopted upstream name may lack a disposition,
  and the 2026-09-11 truth pass added `stale_gaps` beside it: a `gap` row whose
  subject now correlates and that carries no disposition is refused, so a
  shipped name cannot leave a stale `gap` behind it. Sixteen `gap` rows carry a
  disposition today.
* W3.g — **LANDED 2026-09-11. The C half: five measured behaviour defects behind
  adopted rcl names.** Every one compiled exactly as upstream's does and
  differed, which is the case this stage exists for; each is now fixed with a
  unit test that failed against the code it replaced.
  * **The `is_valid` FAMILY and its name accessors** (rows `c:client_is_valid`,
    `c:service_is_valid`, `c:subscription_is_valid` + the three
    `get_service_name`/`get_topic_name` halves, all six DELETED). All six tested
    `state == ..._INITIALIZED` exactly while three entity kinds have further
    LIVE states — client `REGISTERED` (the ordinary `nros_executor_add_client`
    path) and `POLLING`, service `POLLING`, subscription `POLLING` — so the
    upstream guard `if (!rcl_client_is_valid(&c)) bail;` rejected a working
    entity and the accessor handed NULL to whatever `%s` printed it. Fixed as a
    CLASS: one `is_usable()` per entity type, consumed by both the predicate and
    the accessor, on the publisher too (whose answer does not change — it has no
    third live state — so that a future state has one place to be added).
  * **`c:node_is_valid`** (DELETED) — it read the node's own state and, only
    when multi-session, the executor generation, and never `node.support`, so it
    returned TRUE over a session `rclc_support_fini` had dropped. It consults
    `nros_support_is_valid` now. The doc comment said "two questions" where there
    are three; both moved together. `c:node_is_valid_except_context` was declined
    on the reason "the split state is not reachable" — it IS, which is what this
    measured, so that row's `why` is corrected rather than its verdict.
  * **`c:timer_fini`** (DELETED) — `rcl/timer.h` promises verbatim that a NULL or
    already-invalid timer "will not fail"; ours returned `INVALID_ARGUMENT` and
    `NOT_INIT`, two values upstream promises never to produce, to a caller
    `RCL_WARN_UNUSED` asks to check. Idempotent now. The sibling
    `c:guard_condition_fini` is the other member of this class and is somebody
    else's change.
  * **`c:log_severity_t`** (AMENDED to `adopt-bounded`) — rcutils numbers
    `UNSET=0, DEBUG=10 … FATAL=50` with deliberate gaps, and ours was a dense
    `0..=5` with no catch-all, so a ported rcutils constant arrived as an invalid
    `#[repr(u8)]` discriminant into an exhaustive match: instant UB, and `0`
    meaning `UNSET` silently meant TRACE. Now on rcutils's line, mirrored
    `#[repr(transparent)]` over `c_int` against the header's `int`-sized `enum`
    (with the `static_assert` that was missing anywhere in `include/` or `src/`),
    resolved by band so every integer is defined. The row stays open for one
    envelope: rcutils's `UNSET` means *inherit*, and our facade has no
    inheritable level.
  * **`c:timer_get_time_until_next_call`** (DELETED, closes issue 1049) — it
    returned `uint64_t` and took the clock IN, so the error channel was gone
    (five cases shared the value `0`) and overdue was not expressible; it also
    computed from `nros_timer_t::last_call_time_ns`, which no dispatch path
    updates, so the answer was permanently `0` for any timer the executor was
    running. It has upstream's shape now — `nros_ret_t (const timer *, int64_t *)`
    — and forwards to the arena like its three W5.c siblings.
* W3.h — **LANDED: the four C executor / guard-condition gaps, and their
  siblings.** Each was an upstream name that compiled and differed, each is now
  ADOPTED, and each carries a unit test that fails against the pre-fix body
  (mutation-checked, all seven at once).
  * `c:executor_trigger_one` — `obj` is the ENTITY POINTER now, as in rclc.
    Getting there needed the trigger ABI to carry entity identity at all: the
    predicate is handed `nros_executor_trigger_handle_t[]`
    (`{ entity, data_available }`) instead of a bare `const bool *`, the C
    executor records one entity pointer per handle slot at all ten `add_*`
    sites, and `rclc_executor_set_trigger` installs a bridge that joins the
    internal readiness snapshot to that table. The index form survives as
    `nros_executor_trigger_index` — ours-only, ledgered `extension`.
  * SIBLING found by the sweep: `rclc_executor_trigger_all` returned `false`
    for an empty handle array where upstream's loop does not execute and
    returns `true`.
  * `c:executor_spin_some` — a completed cycle is `NROS_RET_OK` whether or not
    a callback ran; rclc discards `rcl_wait`'s `RCL_RET_TIMEOUT`. The idle/busy
    distinction is NOT re-exposed under another name: both in-tree readers of
    it were removed as wrong (issues 0324/0355) and nothing else wanted it.
  * `c:executor_spin_period` + `c:executor_set_timeout` — both period spins
    wait for `timeout_ns` and spend `period_ns` on the sleep, so
    `rclc_executor_set_timeout` is honoured by all three spin verbs.
  * `c:guard_condition_fini` — idempotent. The row's EVIDENCE BOUND is closed:
    `rcl/src/rcl/guard_condition.c` @ humble errors only on NULL and returns
    `RCL_RET_OK` when `impl` is NULL. SIBLINGS, each verified against its own
    upstream source rather than by analogy: `rcl_node_fini` (rcl says
    "Repeat calls to fini or calling fini on a zero initialized node is ok")
    and `rclc_executor_fini` (rclc says the same, and our `== UNINITIALIZED`
    guard let a second call reach `drop_in_place` on zeroed storage) are now
    idempotent too. `rcl_clock_fini` is NOT: upstream returns
    `RCL_RET_INVALID_ARGUMENT` for an uninitialised clock, so ours erroring is
    correct and it was left alone. `rcl_timer_fini` is the one sibling not
    touched here — it is phase-417's other stage-3 assignment.

**Acceptance:** every upstream name we define either behaves or fails to
compile. Demonstrated by an expected-failure probe per REFUSE-LOUD item, in the
shape `check c` / `check cpp` already use.

## Stage 4 — make our own three languages agree

**This is the remaining body of work in the campaign**, with stage 5. A
per-language drop-in claim is undermined while our own surfaces disagree about
the same capability. 37 such disagreements are catalogued; C++ is the odd one
out in 15. The sweep that catalogued them is issue 0788, homed in phase-381.

* W4.a **[wrapper]** — **LANDED 2026-09-13.** Parameters: setter, type query,
  undeclare, descriptors (ranges, read-only) in all three.

  **This item OWNED the 27 `param.json` `gap` rows**, and stating that was the
  point of saying so here: `param.json` is the largest single-shard queue left,
  and until 2026-09-13 three documents each implied a different owner for it.
  It is not [phase-426](phase-426-parameters-rust-ssot.md)'s — that phase made
  the store SINGLE and per-node, which is done, and its "Not in scope" excludes
  callbacks explicitly. It is not
  [phase-444](phase-444-rmw-fix-up.md)'s — that section is an index and says so.
  The rows are missing cross-language SURFACE on top of one store, which is
  exactly what this item is. Measured 2026-09-13 before the work, they grouped
  as follows; the OUTCOME column is what landed the same day.

  | group | rows | outcome | notes |
  | --- | ---: | --- | --- |
  | descriptors, ranges, read-only, constraints | 7 | all 7 closed | `c:add_parameter_constraint_{double,integer}`, `c:add_parameter_description`, `c:set_parameter_read_only`, `rust:ParameterRange`, `rust:ParameterRanges`, `rust:ParameterBuilder::constraints` |
  | `undeclare` / `delete` | 2 | both closed | `c:delete_parameter`, `cpp:Node::undeclare_parameter` |
  | `describe` / `list` / type query | 4 | all 4 closed | `cpp:Node::describe_parameter{,s}`, `cpp:Node::get_parameter_types`, `cpp:Node::list_parameters` |
  | plural and `_or` forms | 5 | 3 closed, 2 `declined` | `cpp:Node::{declare,get,set}_parameters`, `set_parameters_atomically`, `get_parameter_or` |
  | set-parameters callbacks | 2 | both closed | `cpp:Node::{add,remove}_on_set_parameters_callback` — was out of scope for phase-426 by construction ("adding a callback path before the store is single would be a third implementation"). The store IS single now, so the reason has expired and the work lands here |
  | the `rclcpp_lifecycle` copies | 1 | **still open — W4.f** | `cpp:LifecycleNode::*parameter*`, a glob row; it moves with stage 4's lifecycle work, not ahead of it |
  | types and errors | 6 | all 6 closed | `cpp:ParameterType`, `rust:Parameters`, `rust:Node::use_undeclared_parameters`, `rust:DeclarationError`, `rust:RmwParameterConversionError`, `c:executor_add_parameter_server_with_context` |

  Two issues sit inside this item rather than beside it: **0793**'s C half is
  W2.a above and **1203** is phase-426's. The 0793 ordering claim — that the
  second C store must go before a C descriptor API is worth writing — turned
  out to be false, and usefully so: the new C verbs go on the EXECUTOR-owned
  store only, which is the one the six `rcl_interfaces` servers read, so
  nothing here depends on what happens to the legacy family. Adding them to
  the legacy store is what would have needed 0793 settled first, and that is
  precisely what was not done.

  **What landed.** **`param.json`'s `gap` rows: 27 → 1**, and the one left is
  `cpp:LifecycleNode::*parameter*`, the glob that moves with W4.f rather than
  ahead of it. The shard is 171 rows: 74 divergence, 44 declined, 27 extension,
  24 rename, 1 gap, 1 their-rename.

  Everything lands in `nros_params::ParameterServer` and the wrappers forward.
  A wrapper that kept a descriptor table of its own would be the second store
  phase-426 spent six work items removing, in a smaller spelling.

  | what shipped | where |
  | --- | --- |
  | descriptors as post-declare mutators — rclc's four verbs | `ParameterServer::set_parameter_{description,read_only,integer_range,float_range}`, `Executor::*_on`, `nros_executor_{add_param_description,add_param_constraint_*,set_param_read_only}[_on]`, `nros_cpp_node_*`, `rclcpp::Node::declare_parameter<T>(name, default, descriptor)` |
  | `additional_constraints`, which `to_rcl_descriptor` and `write_descriptor` had always sent EMPTY | `ParameterDescriptor::additional_constraints`, `ParameterBuilder::constraints` |
  | describe / list / type query, locally | `Executor::{describe_parameter,list_parameters,get_parameter_type}[_on]`, `nros_executor_{describe_param,list_params,get_param_type}[_on]`, `rclcpp::Node::{describe_parameter,list_parameters,get_parameter_type,get_parameter_types}` |
  | undeclare | `Executor::undeclare_parameter[_on]`, `nros_executor_delete_param[_on]`, `rclcpp::Node::undeclare_parameter` |
  | `set_parameters_atomically`, served since phase-382 and callable from no language | `ParameterServer::apply_atomically` (which the service handler now forwards to, so the wire and the local call cannot disagree), `Executor::set_parameters_atomically[_on]`, `rclcpp::Node::set_parameters_atomically` |
  | the accept/reject hook, on the store `ros2 param set` reaches | `ParameterServer::add_on_set_parameters_callback`, fired in `apply`; `nros_executor_set_param_callback[_on]`; `rclcpp::Node::add_on_set_parameters_callback` |
  | `rclcpp::ParameterType`, `ParameterDescriptor`, `ParameterWrite` — C++ named none of them | `nros/node_parameters.hpp` |
  | six Rust facade exports that were capabilities nobody could NAME | `ParameterRange`, `IntegerRange`, `FloatingPointRange`, `UndeclaredParameters` (which now writes), `DeclarationError`, `ValueConversionError` |
  | `try_declare` → `DeclarationError`, where `declare` returned a bare `bool` for "taken", "full" and "out of range" alike | `nros_params::server` |

  **How a descriptor crosses the FFI without allocating.** Text goes IN as a
  borrowed `const char*` and comes OUT in a caller-owned `char*` of stated
  capacity — the shape `nros_executor_get_param_string` already had — and the
  scalars are NULLable out-params. There is NO descriptor struct on either
  boundary: one would carry either a pointer into the store (a borrow no C or
  C++ caller can honour) or an inline buffer, and the second would put
  `NROS_MAX_PARAM_DESCRIPTION_LEN` into a public ABI and make a build knob a
  LAYOUT, which `check-cpp-capability-layout`'s rule forbids. Descriptor prose
  TRUNCATES with `FULL` where a string VALUE refuses, because a prefix of a
  description is still usable and a shortened frame id is a wrong answer.
  C++'s `rclcpp::ParameterDescriptor` is a value that BORROWS its text, and
  `describe_parameter` reads one back into a caller buffer split in half.
  `list_parameters` writes into a caller-owned rectangle and always reports the
  TOTAL that matched, so "ask the size, then read" works.

  **The callback registry is bounded, and the number is 4.** rclcpp's is
  unbounded because it allocates and hands back an owning
  `ParameterCallbackHandle`; ours is a fixed array in the store and the handle
  is an opaque `u16` token, so unregistering is explicit. A slot is at most
  four machine words — 128 bytes on a 64-bit host beside a 285,184-byte default
  table — and four is one hook per node of a four-node composed image
  (RFC-0047's shape). The (N+1)th registration REFUSES; it never evicts. It is
  deliberately NOT a build knob: every other parameter capacity sizes a table
  of the USER's data, this one sizes a table of the image's own code, which is
  known by reading the source. The hook runs on `apply` — the one seam the six
  `rcl_interfaces` servers, both wrappers and `Executor::set_parameter` share —
  and AFTER the store's read-only / type / range rules, so it never sees a
  write those already refused. `SetParameterResult::Rejected` is its verdict.
  phase-426's "adding a callback path before the store is single would be a
  third implementation" was right and its reason has expired: the store is
  single, and this is one registry.

  **What the wave MEASURED that reading would not have.** Giving
  `additional_constraints` the description's knob doubled the DERIVED
  parameter-service buffer for every image, used or not — the describe reply's
  bound prices CAPACITY, not use — taking the eight-parameter reference island
  from 2,741 bytes to 4,789 and past the 4,096 configured fallback. A field
  nobody had set would have decided the buffer's size. So it has its own knob,
  `NROS_MAX_PARAM_CONSTRAINTS_LEN`, default **0**, on the same ladder; an image
  that wants the text states it, and text that does not fit is truncated and
  REPORTED through `take_truncated_descriptions` rather than dropped in
  silence. The same pass found `write_descriptor` — the LIVE streaming
  serializer, where `to_rcl_descriptor` beside it is the by-value test oracle —
  hardcoding `""` for the field, so a constraint attached in any language would
  have reached no `ros2 param describe`.

  **Acceptance, as run.** `nros-params` 80 unit tests (8 new) at
  `NROS_MAX_PARAM_CONSTRAINTS_LEN` = 0 and 256; `nros-node --lib --features
  std,param-services` 459, likewise at both. `executor_param_descriptors.c`, a
  COMPILE+LINK+RUN on the shared stub backend wired into `just check c` — a
  descriptor verb that kept the range in a table of its own, or forwarded to
  the legacy `nros_parameter_server_t`, would compile, link and pass every
  declaration check while the write it was supposed to refuse went through,
  which is RFC-0089's "compiles and differs". Measured against two mutations
  (skip the hook in `apply`; drop the current-value check when attaching a
  range): 5 failures, 0 when correct. `param_descriptor_surface.cpp` compiles
  the whole C++ surface in BOTH arms — hosted and `-nostdinc++` against the
  ThreadX shim — which is what keeps the no-STL promise honest.

* W4.b **[mixed]** — **LANDED, in two halves, and the item's own prose was
  stale by the time it was picked up.** Actions.

  **The first half had already shipped.** `nros::GoalUUID` (the goal-id VALUE
  type — standard layout, trivially copyable, `==`/`<`/`is_zero`, `sizeof ==
  16`, raw `const uint8_t[16]` overloads kept beside every `GoalUUID` one),
  `ActionServer<A>::succeed`/`abort`/`canceled`, and Rust's `cancel` →
  `canceled` all landed inside commit `806b2e223` ("stages 2/3/4"), which the
  item was never marked for. **Verify the item against the tree first** — this
  is the fourth W4 wave in a row to find its brief partly stale, and here it
  was three of three bullets. The deprecation question the brief asked
  (feature-armed vs deleted outright) was settled the third way: `cancel` is an
  INHERENT method with no in-tree callers, so an always-armed
  `#[deprecated]` forwarder costs nothing under `-D warnings` and is what
  shipped. Measured again here: `just check fast` is green with it armed.

  **The second half is the five `action.json` `gap` rows**, which is what
  "Five `action.json` rows" actually named, and none of them is in the prose
  above. All five are closed:

  | row | closed by | before → after |
  | --- | --- | --- |
  | `c:action_client_get_action_name` | `rcl_action_client_get_action_name` | `gap` → `same` (row deleted) |
  | `c:action_server_get_action_name` | `rcl_action_server_get_action_name` | `gap` → `same` (row deleted) |
  | `c:action_server_goal_exists` | `nros_action_server_goal_exists` | `gap` → `same` (row deleted) |
  | `cpp:Client::action_server_is_ready` | `rclcpp_action::Client<A>::action_server_is_ready` | `gap` → `same` (row deleted) |
  | `c:action_expire_goals` | `nros_action_expire_goals` | `gap` → `divergence` + `adopt-bounded` |

  Shard counts: `gap` 5 → **0**, `extension` 77 → 81, `divergence` 203 → 204.
  The C lane's `same` went 108 → 111 and the C++ lane's 144 → 145. `just check
  api-parity` ends "every divergence carries a ledger entry".

  **Two of the five were open on a claim about our own code that is FALSE**,
  which is issue 1022's class inside the ledger:

  * `c:action_server_goal_exists` read "ours answers the stronger question —
    `nros_action_get_goal_status` returns `NROS_RET_NOT_FOUND` for a goal the
    arena has retired … close enough that it may be a documentation fix". It is
    WEAKER, not stronger, and measured: a status lookup reads the ACTIVE set,
    and `complete_goal_raw` removes a goal from it the moment it terminates —
    so for a goal whose result is still retained for a `get_result` the status
    lookup says NOT FOUND while rcl says the goal EXISTS. That window is
    exactly when a client is expected to fetch. `ActionServerCore::goal_exists`
    is active-set OR retained-result; the test
    `goal_exists_covers_active_and_retained_results_not_just_active` walks the
    three states and the middle assertion is the one the documentation fix
    would have got wrong. Negative control: drop the retained half and it fails
    on that exact assertion.
  * `c:action_client_get_action_name` read "our Rust has
    `nros_core::ActionServer::name`". It could not be CALLED — `nros` re-exports
    `nros_node`'s live `ActionServer`/`ActionClient` under those spellings, so
    the `nros_core` markers were shadowed and unreachable through the façade,
    and measured they had ZERO references anywhere in the tree. **Deleted**
    (PR #806's precedent: in-tree-only reach, measured, so a hard delete rather
    than a forwarder). That is the FIRST withdrawal
    `check-codegen-version-surface` has seen: `NROS_CODEGEN_VERSION` 5 → **6**,
    `NROS_CODEGEN_VERSION_MIN` stays 2 (nothing generated ever named them, so a
    version-5 tree still runs), plus the two NuttX committed snapshots (issue
    1115's two-file rule) and the codegen golden.

  **What the accessor cost, and why the earlier note did not apply.** An
  earlier wave deleted `Server<A>`'s `char action_name_[256]` as dead state and
  left "if an accessor is wanted, it belongs on an FFI getter over the name the
  runtime already owns". Measured, the runtime does NOT own it for this tier:
  `CppActionServer` holds a raw arena handle, `nros_cpp_action_server_create`
  takes `_action_name` and drops it, and `CallbackMeta` carries no name. So the
  copy is back on both callback tiers WITH a reader, at the same 256 bytes
  `Subscription::get_topic_name` already pays — and all four action classes now
  read ONE bound, `nros::ACTION_NAME_MAX`, instead of two private literals and
  two absences. A borrowed `const char*` was rejected: a hosted caller may hand
  `create_action_server` a pointer from a temporary.

  **`c:action_expire_goals` is the one that stays a difference, and it is now
  DECIDED rather than open.** `nros_action_expire_goals(server, size_t
  *num_expired)` ships and both tiers answer it. ADOPT-BOUNDED with two named
  refusals: no `result_timeout` (the `no_std` core takes no time source, so
  there is nothing to measure one against) and no `expired_goals` array (an
  entry is reclaimed once its result has been DELIVERED, so "which goals
  expired" is a question about a clock we do not have). The memory bound
  upstream's timer exists to give is already held by issue 0796's on-demand
  slab compaction; what this buys is EAGERNESS.

  **New ours-only rows, all `extension`:** `cpp:Client::get_action_name`,
  `cpp:Server::get_action_name`, `cpp:Server::goal_exists`,
  `rust:ActionServer::goal_exists`, `rust:ActionServer::expire_goals`.
  `rclcpp_action` has no `get_action_name` on either class (measured against
  `docs/reference/api-surface/rclcpp.json`) and no `goal_exists` at all, so
  these agree with rcl rather than with rclcpp — which is the shape this stage
  wants, since the disagreement being closed is among OUR OWN surfaces.

  **Acceptance.** Behavioural where the change is behavioural:
  `goal_exists_covers_active_and_retained_results_not_just_active` (nros-node,
  RUN, negative control measured) and `accessors_refuse_an_unusable_server` /
  `get_action_name_refuses_an_unusable_client` (nros-c, RUN — a name accessor
  that handed back the zeroed buffer would return an empty C string that reads
  like a real name; negative control measured at 1 failure). Signature-shaped
  where the change is a signature: `action_goal_uuid.cpp` grew section 6,
  asserting `const char*` for the two name accessors and `bool` for the two
  predicates — a `Result` there would compile at every call site and mean
  something else — plus that both polling tiers' `ACTION_NAME_MAX` IS
  `nros::ACTION_NAME_MAX`.
* W4.c **[wrapper]** — **LANDED.** executor: `cancel`/`is_spinning` in all three,
  under rclcpp's own names. Before: C had `nros_executor_stop` mutating a
  C-side state enum, Rust's `halt`/`is_halted` were `alloc`-GATED (so a
  core-only image could not stop its own executor at all), and C++ could not do
  it — `Executor::spin` exited only on `shutdown()`, which calls
  `nros_cpp_fini` and destroys the session. After: one flag in Rust;
  `nros_executor_cancel` / `nros_executor_is_spinning` (C, with
  `nros_executor_stop` left as a deprecated `static inline` forwarder) and
  `Executor::cancel` / `is_spinning` (C++ and Rust) forward to it.
  ADOPT-BOUNDED — `cancel` returns at the NEXT POLL BOUNDARY, so it returns
  before spinning has stopped and `is_spinning` is the observable that says
  when it has; `is_halted` (cancel REQUESTED) stays a distinct question from
  `is_spinning` (a loop is RUNNING), and all four combinations are reachable.

  **The acceptance is BEHAVIOURAL and the last wave's probe was not**
  (2026-09-13). `executor_cancel.cpp` is a `static_assert` TU that delegates
  the behaviour to `spin.rs::cancel_tests` — right about where the flag lives
  (RFC-0019), incomplete here: the Rust test drives `Executor::spin` directly
  while C++ reaches the loop through `nros_cpp_spin`, and no `static_assert`
  can tell "the loop returned and the session is still open" from "the loop
  returned because the session died", which is the exact pair W4.c exists to
  separate. `executor_cancel_runtime.cpp` is the RUN: it links
  `libnros_cpp.a` plus the shared C stub RMW backend, observes
  `is_spinning()` TRUE from another thread (so "the loop ended" cannot be
  satisfied by a loop that never started), cancels, asserts `spin()` RETURNS,
  and then asserts the session survived — `ok()` still true, the node created
  before the cancel still answers its name, and **a NEW node can be created on
  the executor afterwards**, which is the cheapest question only a live
  session can answer. It also asserts cancel is not one-shot (a second spin
  runs and a second cancel ends it, because the flag clears on spin ENTRY).
  Negative controls, measured: `cancel` made a no-op → 1 failure plus a FATAL;
  `cancel` made to call `shutdown()` → 6 failures. On `just check cpp`.
* W4.d **[wrapper]** — **LANDED.** logging: named loggers, per-logger levels,
  throttle, sinks. C got the whole surface —
  `nros_log_get_logger` / `nros_logger_{get_name,set_level,get_level,is_enabled}` /
  `nros_log_add_sink` / `nros_log_throttle_admit` + the `NROS_LOG_*_THROTTLE`
  family — each a thin forwarder onto `nros-log` (RFC-0019); the façade
  re-exports `nros_log`, so `std::println!` is no longer the path of least
  resistance from it (issue 0589).

  **The C++ half was one accessor short until 2026-09-13**, and that was the
  last open `gap` row in the stage: `cpp:Logger::set_level` read "A ported
  node can raise or lower a logger in rclcpp and cannot here." It can now —
  `rclcpp::Logger::set_level(Level)` plus the two readers C and Rust already
  had, `get_level()` and `is_enabled()`. `Level` is upstream's nested enum and
  its values ARE `nros_log_severity_t`'s, so the cast is an identity and there
  is no second number line to drift. ADOPT-BOUNDED twice over: the return type
  widens to `Result` because RFC-0018 forbids exceptions (and `Result` is
  `NROS_NODISCARD`, so a ported `logger.set_level(x);` warns), and a `Logger`
  with a NULL handle — built from a name alone, or from an uninitialised node
  — is REFUSED rather than redirected. `detail::log_handle` does redirect and
  is right to, because a RECORD with no owner belongs in the catch-all; a
  THRESHOLD WRITE with no owner does not, since redirecting it moves the level
  of every unnamed logger in the image while reading at the call site as if it
  had moved the named one. That is issue 1019's third defect pointing the
  other way, and it is the assertion a negative control measured at 2 failures.

  **Acceptance, in all three languages: two nodes in ONE image log under
  DISTINCT names.** Rust's is
  `nros-log/tests/named_loggers_levels_and_throttle.rs`; C's is
  `nros-c/tests/run/named_logger_levels.c` and C++'s
  `nros-cpp/tests/compile/logger_names_and_levels_runtime.cpp`, both new here,
  both RUNS on their `just check` lane. Runs rather than signature probes
  because every shape of this defect compiles — three of four accessors used
  to resolve through a lookup-only `get_logger` answering the catch-all for
  any unregistered name, and two handles and one handle have the same TYPE.
  Each asserts the levels through an INSTALLED SINK rather than through the
  getter, because a `set_level`/`get_level` pair over one shared cell agrees
  with itself while dropping the wrong records. Each also pins the SHARED
  HELPER, which is what keeps the wrapper a wrapper: the handle
  `nros_log_get_logger(name)` / `rclcpp::get_logger(name)` answers must be the
  handle the NODE accessor produced, so a second table beside
  `nros_log::resolve_logger` is a failure rather than a coincidence.

  **Not in this item, and both already closed elsewhere:** the severity
  numbering (ours was 0–5 against rcutils's 10–50) moved onto rcutils's line
  in stage 3 with a width pin, issue 1330; and `RCLCPP_FATAL` lowering to
  ERROR was issue 1019, closed 2026-09-11. The `_THROTTLE` family stays
  REFUSE-LOUD in C++ for its own recorded reason (issue 1302).

  **Ledger rows corrected rather than merely closed** — four of them stated
  facts about our code that W4.d had already made false, which is issue 1022's
  class: `c:log_levels_add_logger_setting` said `<nros/log.h>` exposes no
  `nros_logger_set_level`; `c:logging_output_handler_t` said a C caller cannot
  install a sink at all and that C records carry no location and no time;
  `c:logging_configure_with_output_handler` inherited the first of those as
  its whole reason; `c:logger_t` said we decline per-logger level maps and an
  installable output handler. `cpp:Node::get_logger` said ours returns
  `const void*`, which phase-427 W5 changed to `rclcpp::Logger` — and the
  return type is the one fact that row exists to record. Three `exec.json`
  rows said C exports no `is_spinning` reader and that the three languages
  spell the capability three ways; W4.c settled both.
* W4.e **[rust-first]** — **guard conditions: one owner and one creation shape.
  LANDED 2026-09-18.** The ledger recorded the choice as genuinely open
  ("Nothing about no_std picks between these"); the decision is **the NODE is
  the owner, ONE call, callback bound at creation, in all three languages.**

  **The arena constraint does not pick a winner** — the executor owns the flag,
  a handle is an index, registration is one-way, and all three shapes satisfy
  that through the node's executor. What picked it: C's separate `set_callback`
  is the after-the-fact installation we already REFUSE in C++ at
  `cpp:GuardCondition::set_on_trigger_callback`, so one constraint was enforced
  in two languages and contradicted in the third; and a porting user has a node
  in hand, not an executor.

  | | before | after |
  | --- | --- | --- |
  | C | `nros_guard_condition_init(gc, SUPPORT)` → `_set_callback` → `nros_executor_add_guard_condition` | `nros_node_create_guard_condition(node, out, cb, ctx)` |
  | C++ | `NODE.create_guard_condition(out, cb, ctx)` | unchanged — it was the target |
  | Rust | `EXECUTOR.register_guard_condition(closure)` | `NodeCtx::create_guard_condition(closure)`, over the kept primitive |

  Retired with **no forwarders** (stage 6 step B): `nros_guard_condition_init`,
  `nros_guard_condition_set_callback`, `nros_executor_add_guard_condition` and
  its `nros_executor_register_guard_condition` alias. Measured blast radius was
  four C references, two of them the one example, which is migrated.

  **Both defects the item was written on HELD, and the second was worse than
  stated.**

  1. *C's shape was three calls and the object was inert until the third.*
     `nros_guard_condition_init` only zeroed fields; `handle_id` stayed
     `SIZE_MAX` and a trigger landed on a local byte no executor watches. The
     one in-tree C caller — `examples/native/c/custom-platform` — made the first
     two calls and never the third, so its SIGINT handler had been signalling
     nothing for four phases while every call returned `NROS_RET_OK`.
  2. *`nros_guard_condition_trigger` was not exported — because it did not
     exist.* `grep -c` in `nros_generated.h` was 0, and so was a tree-wide grep
     of every `.rs`/`.c`/`.h`. The only firing verb was rcl's
     `rcl_trigger_guard_condition`, which we have exported all along — so the
     ledger was wrong in BOTH directions at once: `c:trigger_guard_condition`
     said a porting user must rewrite rcl's spelling (we ship it, and the
     correlator buckets the item `same`), and named ours as a symbol nothing
     defined. `nros_guard_condition_trigger` is the definition now and rcl's
     forwards to it, kept and NOT deprecated under RFC-0089's alias rule.

  **Three things the work measured that the item did not predict.**

  * **`is_triggered` / `clear` had to move to the arena flag, or they would have
    become permanently false.** They read the local byte in
    `nros_guard_condition_t`, which only the UNREGISTERED fallback path ever
    wrote — so the moment creation became registration, C's polling readers
    would have answered `false` for every guard condition that exists. All three
    languages read the arena flag now, and C++ and Rust GAINED the pair, which
    closes the "OUR LANGUAGES DISAGREE" the `c:guard_condition_is_triggered` row
    had carried ("a polling C++ or Rust user cannot do what a polling C user
    can"). The bound is part of the API: the executor consumes the flag on
    dispatch, so the reader answers "set and not yet dispatched".
  * **A wake landing BETWEEN spins made the next spin drive its FULL timeout** —
    the opposite of what the wake flag exists for, and the opposite of what the
    swap-and-clear's own comment has claimed since Phase 104.C.6 ("skip the
    blocking wait … and poll every session non-blockingly"). `spin_once` read
    `if !was_woken && has_async_wake` and sent the woken case to the `else`.
    Measured on the C probe: **400 ms to dispatch before, 0 ms after.**
  * **No C executor can be woken out of a parked transport read, on any
    backend.** `has_async_wake` is set by `install_wake_signal_on_primary`,
    which the three Rust `Executor::open*` paths call and the C path does not —
    `nros_executor_init` reaches `from_session_ptr_in`, which assembles over a
    session it BORROWS from the support context. So a C trigger is dispatched at
    the spin boundary, bounded by the spin's own budget, which is what the probe
    asserts. Not fixed here, and deliberately: `Executor::drop` does not clear
    the wake callback (the runtime cb's own safety comment says it must), and on
    the C path the session OUTLIVES the executor (`nros_executor_fini` then
    `nros_support_fini`), so installing it would leave a backend holding a
    callback into freed wake state. That is a separate defect, named rather than
    smuggled in.

  **Acceptance is behavioural, not a compile assertion.**
  `packages/api/nros-c/tests/run/node_guard_condition.c`, on `just check c`:
  creation registers (`handle_id != SIZE_MAX`, handle count +1), a trigger from
  another thread runs the callback **on the executor's thread** with the context
  the creation call bound, the flag travels through the arena and the dispatch
  consumes it, and the wake is measured **against a negative control** — an
  identical spin with nothing pending, which the stub backend sleeps out.
  Measured: **idle 400 ms, pre-spin wake 0 ms, parked-trigger dispatch within
  the spin's budget.** The stub backend gained the `set_wake_callback` slot it
  needed to be parkable at all (`supports_wake_callback` is literally "is this
  slot non-NULL"), and `timer_clock_source` stays green with it.
  Rust: two cases in `executor/tests.rs`. C++: `guard_condition_surface.cpp`
  INSTANTIATES the class (non-template methods on a non-template class are
  parsed and never compiled otherwise), hosted and `-nostdinc++`.

  Ledger: `c:guard_condition_set_callback` (divergence→declined/absent, and it
  carried the open question), `c:guard_condition_init` and
  `c:executor_add_guard_condition` (new, declined/absent),
  `c:node_create_guard_condition`, `rust:Node::create_guard_condition`,
  `rust:Executor::register_guard_condition_on`,
  `{cpp,rust}:GuardCondition::{is_triggered,clear}` (new extensions),
  `c:trigger_guard_condition` + `c:guard_condition_trigger` (both false, both
  corrected), `c:guard_condition_is_triggered`, `c:guard_condition_clear`,
  `c:guard_condition_callback_t`, `cpp:GuardCondition::set_on_trigger_callback`,
  `cpp:GuardCondition::is_valid`, `cpp:Node::create_guard_condition`,
  `rust:Executor::register_guard_condition`. Upstream's free-standing
  constructor from a `Context` stays REFUSED (`cpp:WaitSet`).
* W4.f **[wrapper]** — **lifecycle, 15 rows, added 2026-09-13** because the
  shard is the third largest and no work item named it. `register_on_*` differs
  across our three languages (which is squarely this stage's subject), and the
  rest are missing: no `~/transition_event` publisher (REP-2002), no
  `LifecyclePublisher` or managed-entity protocol, no `get_transition_graph`,
  and no `get_clock`/`now` on the lifecycle node.

## Stage 5 — C

* W5.a **[rust-first]** — **typed subscription delivery. LANDED 2026-09-04
  (the surface) and 2026-09-13 (the callers).** rclc delivers a deserialised
  message on a path with no allocator, by having the caller own the storage:
  `rclc_executor_add_subscription(executor, subscription, void *msg, callback,
  invocation)` with `void (*)(const void *)`. Ours had no `msg` slot and
  delivered `(const uint8_t *, size_t)`, so every ported callback body had to be
  rewritten — the largest porting cost this campaign measured.

  **Both facts the item rested on held, checked before building on them.** The
  generated `<Msg>_deserialize(<Msg>*, const uint8_t*, size_t)` writes into
  caller storage and returns 0/-1 (`packs/c/message.c.jinja:109`), with no
  allocation for a bounded type; the typed publish half ships as
  `<Msg>_publish` (`message.h.jinja:166`) and every C talker in the tree calls
  it. So the shape the item predicted is the shape that was buildable.

  **What the surface is.** `nros_executor_add_subscription_typed{,_sized}`
  (`packages/api/nros-c/src/executor.rs:1798,1834`) over
  `Executor::add_arena_subscription_c_typed_callback`, whose arena entry is the
  raw one plus two words (`msg`, `deserialize`) and whose buffering is the same
  `BufferStrategy`. Codegen collapses the one extra argument away, so a ported
  line is rclc's six in rclc's order:
  `<Msg>_executor_add_subscription(&exec, &sub, &msg, cb, &ctx, ON_NEW_DATA)`.
  A refused decode does NOT invoke the callback — it drops the sample, counts a
  `subscription_errors` and logs through `nros_log` — because dispatching would
  hand the callback the PREVIOUS message dressed as the new one.

  **What 2026-09-13 added, and why the item was not closeable without it.** The
  surface had NO in-tree C caller: all five C listener examples still declared
  `subscription_callback(const uint8_t*, size_t, void*)` and hand-called
  `std_msgs_msg_string_deserialize`, and the only exercise of the typed macro
  was a compile probe. A delivery path nothing runs is one nobody would notice
  breaking. All five are ported (native, threadx-linux, rv-virt-threadx,
  qemu-armv7a-nuttx, mps2-an385-freertos — one transform, five identical
  files), each leaf gaining the `nros-codegen.toml` its C++ sibling has had
  since issue 1098: `std_msgs/String` is `string data` with no IDL bound, and
  the six-argument form passes `_RX_MAX_SERIALIZED_SIZE`, which codegen POISONS
  for an unbounded type rather than inventing. Measured with the cap:
  `STD_MSGS_MSG_STRING_RX_MAX_SERIALIZED_SIZE 272`, `char data[256]`.

  **The one argument the type token could not vouch for.** Five of the six come
  from one token and cannot disagree; `msg` is `void *` at the FFI and accepted
  any object pointer silently, so the wrong struct's storage would compile and
  then be overwritten with a message of another type every dispatch — with no
  allocator anywhere to notice. The generated macro now routes it through
  `1 ? (msg) : (<Msg>*)0`, a conditional whose branches must be compatible
  pointers: a warning in C naming both types, an error in C++, and the
  const-discard diagnostic kept because nothing casts. Negative control:
  `tests/compile/typed_subscription_storage_mismatch_probe.c`, compiled
  `-Werror` in `check c` and required to FAIL.

  **Measured.** `native_example_pubsub_e2e::case_2_c_zenoh` PASS on fresh
  fixtures; by hand against the C talker, `I heard: [Hello World: 1..6]` — the
  payload, not just the marker. `just check c` green including both probes;
  `check no-std` green on thumbv7m-none-eabi and riscv32imc (the point of the
  exercise is a path with no allocator); `nros-node --lib` 420 passed,
  including `typed_c_subscription_delivers_into_caller_storage` and
  `typed_c_subscription_refused_decode_is_loud_and_undelivered`.

  **Ledger.** The item said the reason blamed an allocator rclc does not have
  either, and it was right: `c:executor_add_subscription_typed` retracts it in
  its own `why`. Two rows beside it were still wrong on 2026-09-13.
  `c:subscription_callback_t` said the callback binds to the entity at creation
  (stage 6 moved it to registration) and cited
  `executor-owns-no-entity-storage`, a rule RFC-0089 measured as defined
  nowhere — re-verdicted `divergence` → `extension`, because the byte callback
  is now the byte TIER beside a faithful typed one rather than a stand-in for
  it, which is `cpp:bind_subscription_raw`'s verdict one language over.
  `c:typed_subscription_callback_t` was `blocked-needs-decision`; RFC-0089's
  ALIAS RULE (2026-09-09, five days after the row) decides it — upstream HAS
  `rclc_subscription_callback_with_context_t`, so the name is not ours to
  decline — and it is now `settled-pending-owner`, because the alias belongs in
  `<nros/rcl_compat.h>`, which is W5.b's surface.

  **That open item is CLOSED 2026-09-18, by W5.e, and what it found was worse
  than what it predicted.** `rcl_compat.h` section 6 did tell a reader the
  typed entry point does not exist ("the honest move is to name the shape a
  faithful alias needs and wait for it") and did sketch
  `nros_subscription_typed_callback_t` as the thing to wait for — the shipped
  name with two words swapped. What the note did not say is WHEN: the section
  and the entry point landed in the SAME COMMIT (`6da5349d`, 2026-09-04, W5.a
  and W5.b together), so the file has never once described the tree it shipped
  in. A reader following it would have concluded a capability was missing and
  written their own CDR, which is the exact failure this campaign exists to
  remove. The `..._with_context` prediction was also wrong as predicted: ours
  carries the deserialiser, so a forwarder is SEVEN arguments, and the six-in-
  rclc's-order line exists only because codegen knows the type and a header
  cannot. Section 6 is rewritten against the shipped header, the alias
  `rclc_subscription_callback_with_context_t` LANDED there (an alias of our
  typedef NAME, not a second copy of the function type — issue 0160's class),
  and `c:typed_subscription_callback_t` moves `settled-pending-owner` →
  `landed`. A fourth correction fell out: every line number section 6 cited
  into `<nros/nros_generated.h>` pointed at unrelated code, because that header
  is cbindgen output and every regeneration moves it (4 of 4 stale; section 2's
  `:2547` was out by 105 lines). Citations there name identifiers now.
* W5.b **[wrapper]** — **LANDED, and it had already landed once: this item was
  written as if it were open for nine days after its own fix shipped.**
  `<nros/rcl_compat.h>` maps `RCL_RET_*` onto ours — both `RMW_RET_*` and
  `RCL_RET_*` spellings, six codes, with the two drift guards that make the
  mapping hold: `(a)` six `_Static_assert`s pinning ours to the literals they
  have, so a renumbering fails at the header rather than at a consumer that
  stored the old value, and `(b)` five asserting we did NOT take rcl's values,
  which is the mutation RFC-0089 names as forbidden. Held by
  `tests/compile/rcl_compat_aliases.c` (the mapping is INJECTIVE — a `switch`
  over the labels, where C makes a duplicate case a hard error) and
  `rcl_compat_collision_probe.c` (a TU holding the real `<rcl/types.h>` too is
  REFUSED, not silently resolved to one of two numberings).
  The item's own premise is also fixed: `nros_ret_t`'s doc comment said
  "Compatible with `rcl_ret_t` for familiarity" and now leads with "NOT
  value-compatible with `rcl_ret_t` — only `OK` (0) agrees", carrying the table.
  W5.d's pass corrected the header's stale self-description (it still described
  that comment as wrong in the present tense, citing a line number the generated
  header had moved past) and took one NAME stage 6 had left behind —
  `nros_timer_get_time_until_next_call` is `rcl_timer_get_time_until_next_call`,
  no forwarder, because it already had rcl's exact signature and the bullet
  recording that the rename was owed had sat in §4 since stage 6.
  **BREAKING for C callers of the old spelling.**
  Ledger rows retired: 0 — a value mapping has no correspondent to ledger; its
  acceptance is the two probes.

  **One part of this file was still wrong, and it was the part W5.d's pass did
  not reach: SECTION 6 (fixed 2026-09-18, W5.e).** The mapping and the
  self-description were corrected; the section that tells a reader what the
  executor's registration does NOT alias was left describing 2026-09-03. It
  said the typed subscription entry point did not exist and to wait for a name
  it sketched — which had shipped in the SAME COMMIT as the section, in the
  other word order — and predicted a "direct six-argument alias" that cannot
  exist at six. It now reads off the header, carries the alias the ledger asked
  for (`rclc_subscription_callback_with_context_t`), and cites identifiers
  rather than line numbers, because all four line numbers it carried into the
  cbindgen-generated `<nros/nros_generated.h>` pointed at unrelated code. Detail
  in W5.a's closing note and in W5.e.
* W5.c **[wrapper]** — **LANDED.** The node accessors and four of the six timer
  accessors landed in earlier waves (`rcl_node_is_valid`,
  `nros_node_get_domain_id`, `nros_node_get_fully_qualified_name`,
  `nros_node_resolve_name`, `rcl_timer_is_canceled`, `rcl_timer_is_ready`,
  `nros_timer_get_time_since_last_call`, `rcl_timer_get_time_until_next_call`).
  W5.c's remaining two are now in, as the same forward onto the arena's
  `TimerHeader` through the `(handle_id, _executor)` pair
  `rclc_executor_add_timer` installs:
  * `rcl_timer_exchange_period(const timer *, int64_t new_period,
    int64_t *old_period)` — rcl's spelling, arity and order. Writes BOTH copies
    of the period: the arena's `period_us`, which is what
    `arena::timer_try_process` compares `elapsed_us` against, and
    `nros_timer_t::period_ns`, which is the registration input and what
    `nros_timer_get_period` reports — leaving the second stale would have made
    the getter disagree with the dispatcher. `elapsed_us` is NOT rewound, which
    is rcl's own behaviour, so a period lowered below the elapsed count fires at
    once. Rust half: `Executor::exchange_timer_period_us`.
  * `rcl_timer_get_next_call_time(const timer *, int64_t *)` — the ABSOLUTE
    point, on the clock the timer is scheduled against: its `nros_clock_t *` for
    a `nros_timer_init_on_clock` timer, the platform steady clock for the wall
    timer `nros_timer_init` creates. Computed as `now + remaining` over the SAME
    derivation its relative sibling uses (`time_until_next_call_ns_of`), because
    two derivations of one answer is how the pair would come to disagree.
  The one node row left as `gap` is `c:node_get_graph_guard_condition`, which is
  NOT a thin forwarder over executor state — it is the graph family, and the
  vtable slot is `None` in every backend.
  **Ledger rows retired: 2** (`c:timer_exchange_period`,
  `c:timer_get_next_call_time`); one row added
  (`rust:Executor::exchange_timer_period_us`, `divergence` — the arena owns
  timer state, so it sits on the executor where rclrs puts it on `Timer`).
* W5.d **[wrapper]** — **LANDED, and the "~18 declined rows" estimate was spent
  by stage 6 rather than by this item.** Five of the six `_init_default`
  constructors became NATIVE entry points at stage 6
  (`rclc_node_init_default`, `rclc_publisher_init_default`,
  `rclc_subscription_init_default`, `rclc_service_init_default`,
  `rclc_client_init_default`), so by the time W5.d ran the theirs-only preset
  set was SIX, not eighteen. Five adopted as `static inline` forwarders in the
  per-module headers — the shape `nros_difference_times` in `<nros/timer.h>`
  already had, so no symbol and no writable data:
  * `rclc_publisher_init_best_effort` / `rclc_subscription_init_best_effort` →
    `NROS_QOS_SENSOR_DATA`. Read from rclc's source (`10eadcc`), not inferred:
    rclc passes `rmw_qos_profile_sensor_data`, so the DEPTH drops to 5 as well
    as the reliability changing. "The default with reliability flipped" is the
    plausible wrong answer.
  * `rclc_service_init_best_effort` / `rclc_client_init_best_effort` →
    `nros_qos_services_best_effort()`, a sixth `static inline` that copies
    `NROS_QOS_SERVICES` and flips one field, which is literally what rclc's two
    bodies do. Depth stays 10. ROS 2 names that value nowhere, so the shared
    helper is ours-only and ledgered `extension`; two hand-written copies of it
    would be issue 0160's class at header scale.
  * `rclc_action_client_init_default` → `nros_action_client_init` with
    arguments 3 and 4 SWAPPED (rclc puts the typesupport third). A distinct NAME
    rather than a reorder of ours, per RFC-0089's rule: both are pointers, so a
    caller who guessed would get a warning and a client holding a type
    descriptor as its name.
  `rclc_action_server_init_default` is the one refused, and its ledger row now
  carries its own reason rather than a see-also: rclc reads `support` for
  exactly one thing (`&support->clock`) and ours takes no clock, so a faithful
  five-argument forwarder would carry an INERT parameter; and rclc binds the
  goal/cancel handlers at `rclc_executor_add_action_server`, which we do not
  have, so forwarding to the callback-free polling init would compile and
  dispatch nothing. Its disposition moved `adopt` → `absent`, which is what a
  name nothing declares actually gives a porting user.
  **Ledger rows retired: 5** (`c:publisher_init_best_effort`,
  `c:subscription_init_best_effort`, `c:service_init_best_effort`,
  `c:client_init_best_effort`, `c:action_client_init_default`); one row added
  (`c:qos_services_best_effort`, `extension`).

  **Acceptance for W5.b/c/d, measured.** The C lane's `same` count moved
  101 → 108 and `theirs-only` 354 → 347 (`scripts/api-parity.py --lang c`);
  `just check api-parity` ends "every divergence carries a ledger entry".
  `tests/compile/preset_constructors.c` joins the stage-5 loop in `just check c`
  and carries its own
  `#pragma GCC diagnostic error "-Wincompatible-pointer-types"`, because the one
  preset with behaviour is an argument swap and C reports a swapped pointer as a
  warning even under `-Wall -Wextra` — mutation-checked: a forwarder that drops
  the swap is a hard error there, and it is rc=0 without the pragma. The
  presets' QoS claims are Rust unit tests
  (`qos::tests::the_rclc_best_effort_presets_choose_between_two_different_profiles`),
  because a `static inline` has no symbol to call and the claim that can go
  wrong is which profile was chosen. The exchange's runtime behaviour is
  `executor::tests::exchanging_a_timers_period_changes_the_cadence_without_rewinding_it`,
  mutation-checked against a version that rewinds `elapsed_us`.
* W5.e **[rust-first]** — **typed service/client delivery. LANDED 2026-09-04
  (the surface) and 2026-09-18 (the callers) — and, exactly like W5.b, this
  item was written as if it were open for fourteen days after its own surface
  shipped.**

  **The premise was already false when it was written, and the item said so
  itself, in the commit that shipped it.** This bullet read "`service.h.jinja`
  generates only `_get_type_name`/`_get_type_hash` today, so every C service in
  the tree is raw bytes with hand-written CDR". `6da5349d` (2026-09-04) carries
  a section headed "W5.e — typed service/client" whose first line is "My brief
  said the service pack emits only `_get_type_name`/`_get_type_hash`. FALSE" —
  and it landed 12 entry points. The bullet was never updated. The SECOND half
  of the sentence stayed true for two more weeks, which is why the stale
  premise cost nothing until someone measured it: every C service in the tree
  WAS still raw bytes. That is the half this wave fixes.

  **What the surface is** (`packs/c/service.h.jinja`, shipped 2026-09-04). Per
  service, `static inline`, no symbol and no writable data:
  `<Srv>_service_handler_t` (caller-owned request + response storage, the typed
  callback, `last_error`/`error_count`), `<Srv>_service_handler_init`,
  `<Srv>_service_handle_request` (the `nros_service_callback_t` trampoline),
  `<Srv>_service_init`; and the client mirror `<Srv>_client_handler_t`,
  `<Srv>_client_handler_init`, `<Srv>_client_handle_response`,
  `<Srv>_client_set_response_callback`, `<Srv>_client_send_request`,
  `<Srv>_client_take_response`, `<Srv>_client_call`. Plus one Rust entry point,
  `nros_service_typed_report_error`.

  **The two-payload seam, and the three answers it forced.** A subscription has
  one payload arriving; a service has two, travelling in opposite directions,
  so W5.a's shape does not transfer unchanged.
  * *Who owns each buffer* — the CALLER owns both, which is rclc's answer
    (`rclc_executor_add_service(exec, svc, &req, &res, cb)`), and it is what
    keeps an allocator off the path. They travel as ONE block rather than two
    arguments because our registration carries exactly one context word; that
    is also why no executor-side `_typed` variant was added, unlike W5.a's.
    A subscription's byte callback has nowhere to put a decoded message, so
    W5.a had to widen the FFI; a service callback already receives a
    caller-visible reply buffer, so the only missing storage is the two payload
    STRUCTS and a `void *context` can carry them.
  * *A failed request decode* — REFUSED, not dispatched: `_fini` the partial
    decode, count it, emit an ERROR record, return `false`. Measured, not
    assumed: `srv_raw_try_process` sends a reply only on `ok && resp_len > 0`
    (`executor/arena.rs`), so `false` puts NOTHING on the wire and the peer
    times out rather than receiving an answer to a request nobody parsed. The
    same `bool` catches an oversized response — forwarding a short `written`
    there would put a TRUNCATED CDR payload on the wire.
  * *How the response is published* — not by the handler. The handler fills the
    typed struct; the trampoline serializes into the executor's reply buffer
    and reports the byte count through `*response_len`. So the handler body
    names no buffer, no size and no CDR function, which is the whole point.

  **What 2026-09-18 added, and why the item was not closeable without it.** The
  surface had NO in-tree C caller — the only exercise was a compile probe, and
  the five identical `examples/*/c/service-server/src/main.c` still declared
  `bool service_callback(const uint8_t*, size_t, uint8_t*, size_t, size_t*,
  void*)` and hand-called `_request_deserialize` / `_response_serialize`. A
  delivery path nothing runs is one nobody would notice breaking (W5.a's own
  lesson, one entity over). Ported: 5 service servers, 5 blocking service
  clients, 1 callback client (`examples/native/c/service-client-callback`) —
  three transforms over eleven files, native / threadx-linux /
  mps2-an385-freertos / qemu-armv7a-nuttx / rv-virt-threadx. The servers also
  stop building an `nros_service_type_t` by hand: `<Srv>_service_init` names
  the type support, the trampoline and the handler together, so the three
  cannot disagree.

  **The negative control, and why it is a different one from W5.a's.** W5.a
  needed `1 ? (msg) : (<Msg>*)0` because the FFI takes storage as `void *`.
  The service seam has no such hole — the handler FUNCTION and the handler
  BLOCK are both generated types — so a mismatch should already be ill-typed.
  That is a claim, and
  `tests/compile/typed_service_storage_mismatch_probe.c` measures it: a handler
  with the two payloads SWAPPED, and a `_client_handler_t` on the server seam.
  Compiled `-Werror` in `check c` and REQUIRED to fail, because C reports both
  as warnings. The swap is the mistake this seam invites and the one rclc's own
  `void (*)(const void *, void *, void *)` could not diagnose at all.

  **Ledger.** `c:service_callback_t` and `c:response_callback_t` re-verdicted
  `divergence` → `extension`, the same move `c:subscription_callback_t` made on
  2026-09-13 and for the same reason: the byte callback is now the byte TIER
  beside a faithful typed one rather than a stand-in for it. Both rows' stated
  reason was also stale — "bound to the entity at creation" describes an
  arrangement stage 6 removed, and the row it deferred to had already retracted
  that argument, so a `see` was inheriting a retraction.
  `c:typed_subscription_callback_t` `settled-pending-owner` → `landed` (the
  alias landed in `rcl_compat.h`, W5.e's other file).

  **Measured, live.** All three transforms were built as real example binaries
  (`just native _build-c-example`) and run against `rmw_zenohd` on an isolated
  endpoint. Typed client: `Result of add_two_ints: 42` for `17 25`; typed
  CALLBACK client: `Result of add_two_ints: 17` for `9 8`; the typed server
  printed `Incoming request / a: 17 b: 25` then `a: 9 b: 8` from its typed
  handler and exited `Total requests handled: 2` with no refusals — the PAYLOAD,
  through the generated trampoline, not a marker. Compile-side: `just check c`
  rc=0 with both probes; the three sources compile `-Wall -Wextra -Werror`
  against the REAL generated `example_interfaces` header, not only the golden
  corpus; `arm-none-eabi-gcc 13.2 -ffreestanding -Os -mcpu=cortex-m3 -Werror`
  compiles the typed service header and the object's undefined-symbol set is 16
  generated/`nros_*` entry points with ZERO allocator or libc symbols, which is
  the no-allocator claim measured rather than stated. `just check api-parity`
  rc=0; `api-parity-ledger` rc=0; `codegen-version-surface` OK at version 5 /
  259 items UNCHANGED — no pack was edited and no new runtime identifier is
  named, so `NROS_CODEGEN_VERSION` does not move and neither NuttX fallback
  snapshot nor the golden corpus needed carrying.

  **One more stale claim, MEASURED and left with a reason.**
  `nros_service_init_with_qos`'s doc says "`nros_executor_add_service` treats an
  absent callback as nothing to dispatch". It does not — `executor.rs` returns
  `NROS_RET_INVALID_ARGUMENT` when `get_callback()` is `None`, which is the
  better behaviour and the opposite of what the sentence promises. Left because
  the fix is a Rust doc comment plus a cbindgen regeneration of the 7000-line
  `nros_generated.h`, and three sibling agents were regenerating it the same
  day: the conflict would cost more than the sentence does. Take it in a quiet
  window.

  **Measured, left, and named rather than smoothed over:** the two ZEPHYR C
  service examples are NOT ported and are on a different surface
  (`<nros/component.h>`, `nros_cpp_service_server_register`, not
  `nros_service_t`). They are also the worst CDR in the tree — `read_i64_le(req
  + 4)` against a hardcoded 4-byte encapsulation header, with no generated type
  in sight. Giving the component seam a typed handler is its own item; this one
  would have had to widen a second surface to claim it.

## The structural blocker — RESOLVED by phase-417 itself (corrected 2026-09-05)

**This section described a blocker that no longer exists, and nothing scheduled
the node-type merge because the two documents gating it still described the
pre-fix world.** Recording what it said, and what removed it.

It said: the shim `rclcpp::Node` dispatched subscription callbacks and wall
timers from its own `pump()`, driven only by `rclcpp::spin(node)` /
`spin_some(node)` (citing `rclcpp_compat.hpp:428-451,454-470`). A ported file
that instead called `nros::spin_once()`, `nros::spin()`, or drove an
`nros::Executor` got **zero callbacks and no diagnostic** — and after a rename
the two node types would share a name, making it strictly worse.

**That file and that method are gone.** `rclcpp_compat.hpp` was deleted in stage
6 step A; `pump()` was deleted by the one-dispatch-path pass. `nros.hpp:939-948`
says so in its own words — *"`pump()` IS GONE… there is nothing left for a
node-local sweep to do and mixing spin spellings is harmless"* — and
`nros.hpp:622-631` records the change at the call site: every entity a
`rclcpp::Node` creates is now arena-registered through
`nros::create_subscription_raw`, the same call the native path makes, so the
executor dispatches it whichever spin verb the caller drives.

So the precondition the merge was waiting on has been satisfied. What remains is
not a blocker but two open questions, and they are different in kind:

* **The instrument.** `just check cpp`'s "freestanding syntax" probe was host
  `c++ -ffreestanding` against the host's COMPLETE libstdc++, so it could not
  fail on anything a merge risks. A `-nostdinc++` lane against the real
  ThreadX and Zephyr shims is now in `check-cpp`, and it found a live red on
  its first run (`::std::abort` unexported by the ThreadX `cstdlib`, breaking
  every ThreadX C++ image since `rclcpp::init` entered the umbrella header).
* **`sizeof(nros::Node)`.** A merged node whose layout depends on a
  preprocessor capability probe is an ODR hazard, and those probes have now
  been measured wrong three separate times. That is a risk decision, not a
  reading — see the RFC.

Two further claims this section rested on were also measured and are false.
`rclcpp::Node` is **not hosted-only**: it is declared on bare-metal NuttX
armv7a and riscv32, and the real partition is `-nostdinc++` versus not, which
only Zephyr and ThreadX-RISCV64 fall on. And five of W-B5's six C++ items —
`spin_once`, `Timer`, the lifecycle set, `GoalResponse`/`CancelResponse` — have
**zero upstream occurrences**, so they are ours-only names for capabilities ROS
2 does not have: retireable never, not later, and never contingent on this
merge. Only `nros::Node` ever was.

## Stage 6 — the rename, in two steps

Only after stages 1–5 and the structural blocker above. Mechanical once the
shapes match; the two steps exist so the irreversible half happens once.

**Step A — alias (reversible).**

* W6.a — declare the ROS 2 spellings as first-class names IN the API headers;
  delete `rclcpp_compat.hpp` as a separate file. Both spellings work.
* W6.b — `#error` when upstream rclcpp's include guard is already defined. Not
  because we expect a build to link both — we do not, they interoperate over
  the wire and the host tooling is a separate process — but because the guard
  is one line and the failure it prevents is silent.

**Step B — replace (irreversible for out-of-tree consumers).**

* W6.c — deprecate `nros::` / `nros_` spellings, per the settled policy:
  `NROS_DEPRECATED_MSG` static inline for C, `[[deprecated("…")]]` for C++,
  and nothing for Rust trait methods (a rename there breaks implementors, and
  a compile error is what a backend author wants).
* W6.d — migrate the in-tree call sites: **110 C++ and 75 C example files**,
  the six compat-built templates, and the book.
* W6.e — remove the deprecated spellings as ONE batch, with a changelog entry.
  Carry phase-379 W7 step 4's two warnings: **C cannot portably deprecate a
  `typedef`** (MSVC rejects the attribute, `[[deprecated]]` is C23), so renamed
  C types disappear silently for anyone who never rebuilt; and a defaulted
  trait method's alias is weaker than it looks, because an out-of-tree
  override is silently ignored rather than warned.

**Acceptance:** the compat shim is gone, ported templates still build, and no
in-tree source names a spelling upstream does not have — except where a
disposition says why.

## The cheapest work is wrapper work, and there is a lot of it

Of the 37 capabilities where our own three surfaces disagree, roughly half are
ones **Rust already has and one or both wrappers never exposed** — named
loggers and per-logger levels, parameter type queries, undeclare, descriptors
and ranges, QoS equality, GID attribution, name resolution. The behaviour
exists and is tested; the wrapper is a forwarder. Under RFC-0019 that is both
the cheapest work in this phase and the work that reduces the most divergence
per line, which is why stage 4 is not deferred behind the C-side stages.

## Stage 6 step B — surveyed 2026-09-04, its own PR

Step A landed: the ROS 2 spellings are first-class, `rclcpp_compat.hpp` is
deleted, and every `rcl_compat.h` FUNCTION alias became an identity and was
deleted with it. Step B retires the old spellings. **It is the only irreversible
step for an out-of-tree consumer**, so it is one batch, one PR, one changelog
entry — never opportunistic.

### The size, measured rather than estimated

| surface | sites | files | shape |
| --- | ---: | ---: | --- |
| Rust logging macros | 208 | 50 | pure rename |
| C symbols (43 deprecated names) | 206 | 40 | pure rename, **except the one below** |
| C++ `nros::` → `rclcpp::` | 645 | 119 | pure rename |
| docs / book | — | 57 | prose + code blocks |
| the forwarders themselves | 110 | 12 headers | deletion |

Largest concentrations: `examples/native/{c,rust}` (27 files), the five RTOS
example families (25), `packages/testing/nros-tests` (12).

### One site is not mechanical, and it is the whole risk

`nros_node_init` is the single argument REORDER in the campaign:

```
nros_node_init        (node, support, name, namespace_)     <- 42 call sites
rclc_node_init_default(node, name, namespace_, support)
```

A sweep that renames without reordering produces code that **compiles with a
warning and passes the wrong values** — C diagnoses an incompatible pointer
argument as a warning even under `-Wall -Wextra`, which is the measurement that
forced RFC-0089's "a C reorder ships with a rename beside it". Here the rename
IS the guard: the old identifier disappears, so a missed site fails to compile
rather than silently mis-binding.

Surveyed for tractability: **all 42 are single-line, zero multi-line**, and the
shape is uniform (`nros_node_init(&x, &y, "name", "/")`). So a regex codemod is
honest here — but it must be a codemod that reorders, not a `sed s/old/new/`,
and the two must not be run as separate passes.

### Work items

* **W-B1 [rust]** — flip `nros-log/deprecate-legacy-names`, then fix the 208
  sites it names across 50 files. The flip comes FIRST because
  `warnings = "deny"` makes it enumerate the work instead of a grep guessing at
  it.
* **W-B2 [c]** — the 43 deprecated C names across 206 sites / 40 files,
  **including the `nros_node_init` reorder in the same pass**. Rename and
  reorder must not be separate passes: the rename is what makes a missed
  reorder fail to compile.
* **W-B3 [cpp]** — **LANDED, and the survey was wrong.** Not 645 pure renames:
  measured 697 code occurrences over 120 files, of which only **288 (41 %) are
  pure renames**. 409 stay `nros::`, and the reason is the campaign's own rule
  rather than effort:
  * `nros::Node` — **109 sites, 103 files**. `rclcpp::Node` is a DISTINCT CLASS,
    not an alias: `make_shared` + shared_ptr-returning `create_publisher<M>`
    against ours' out-ref `create_node(node, …)`. It is also hosted-only, so it
    does not exist on the freestanding targets most of those files build for.
  * `nros::init` (13) — `rclcpp::init()` returns **void**; every call site
    consumes our `Result`. Renaming drops the error check.
  * `nros::Timer` (28), `nros::spin_once` (32) — no counterpart with that
    contract.
  * ~200 more with no `rclcpp::` counterpart at all: the `bind_*` family,
    `create_node`, `Seq`/`HeapString`, `GoalResponse`/`CancelResponse`/
    `GoalStatus`, the whole lifecycle set (there is no `rclcpp_lifecycle`
    namespace in our headers).

* **W-B5 [retire] — PRECONDITION, added after W-B3 measured it.** The C
  forwarders (43 names) and the five Rust logging forwarders CAN go: every one
  has a live replacement. The C++ `nros::` spellings for `Node`, `init`,
  `Timer`, `spin_once`, lifecycle and the action-response enums **cannot** —
  deleting them removes a capability rather than a spelling. Retiring those
  waits on the node-type merge that `nros.hpp`'s own comment defers to "a later
  step".
* **W-B4 [docs]** — 57 files in `book/` and `docs/` naming an old spelling in
  prose or a code block. No compiler checks these.

* **W-B6 [changelog]** — one entry, carrying phase-379 W7 step 4's two
  warnings.

### Order, and why

1. **Flip `nros-log/deprecate-legacy-names`.** The workspace sets
   `warnings = "deny"`, so arming the attribute turns all 208 Rust sites into
   hard errors at once. That is the point: it enumerates the work rather than
   trusting a grep.
2. **Migrate Rust, then C, then C++.** Each is a rename; the compiler names
   every site.
3. **The `node_init` codemod, alone, with its own review.** Not folded into the
   C rename sweep — it is the one change where a mistake is silent.
4. **Docs and book.** `just book` builds them; a stale code block is not caught
   by any compiler.
5. **Delete the forwarders** — 110 `NROS_DEPRECATED_MSG` declarations across 12
   headers, plus the five Rust forwarders, plus the feature flag itself.
6. **Changelog entry**, carrying phase-379 W7 step 4's two warnings: C cannot
   portably deprecate a `typedef`, so renamed C TYPES disappear with no warning
   for anyone who never rebuilt; and a defaulted trait method's alias is weaker
   than it looks, because an out-of-tree override is silently ignored.

### Acceptance

* every in-tree source builds with `deprecate-legacy-names` ON and zero
  forwarders present;
* `examples/templates/cpp-port-minimal-publisher` still byte-identical to
  upstream's tutorial;
* `rcl_compat.h` holds only the `RCL_RET_*` mapping and handle typedefs — the
  two things RFC-0089 says cannot dissolve;
* `just ci gate` green, and the ported templates build with **every** compat
  layer deleted.

## Correction track — CLOSED, except issue 1042

The measurement is the campaign's instrument; a wrong row is worse than a
missing one because it forecloses the fix. Three issues, ~140 rows, no code.

**W-C1, W-C2 and W-C3 are DONE: issues 1012 and 1022 are both resolved and
archived.** The items are kept below as the record of what was corrected and
why, because the `why` is a standing rule about the ledger rather than a job.
One issue on this track is still open and it is a different failure: **1042** —
nine rows went false when the rclrs pin moved 0.5.1 → 0.7.0 and nothing
re-measured them. Closing it means the ledger RECORDS the pin it was measured
against, so the next bump invalidates rows instead of silently outdating them.

* **W-C1 — issue 1012** (15 rows, resolved): prose names a symbol a rename retired.
  Fifteen describe the CURRENT tree with a dead spelling; a further 21 name one
  legitimately (a deprecated-alias row must). The distinguishing property is
  tense, which is why a sweep on the spelling alone breaks the correct ones.
  The issue's durable half — whether to gate it — needs the tense made
  machine-readable first.
* **W-C2 — issue 1022** (~95 rows, resolved): prose states something FALSE about our own
  code. "There is no runtime options struct" where seven ship;
  `nros_borrowed_str_t` for `nros_view_str_t`; `get_actual_qos` "would return
  its own input" when `set_qos_overrides` makes it differ; `cpp:` rows
  answering in C spellings; ~14 rows verdicted `declined` whose own prose
  describes a capability we have under another name.
* **W-C3 — issue 1022's systematic half** (resolved with it): a divergence justified by a
  constraint the compared surface refutes. Byte-oriented delivery is blamed on
  "no allocator"; rclc has no allocator on that path either and delivers typed
  by making the caller own the storage. RFC-0036 forbids recording a preference
  as a divergence and has no rule against recording a real divergence under a
  false cause. Closing W-C3 means adding one.

### The `gap` rows are the queue, and this phase counts them

**132 `gap` rows, measured 2026-09-13 straight off
`docs/reference/api-parity-ledger/*.json`: C 39, C++ 58, Rust 35.**

| shard | gaps | owner |
| --- | ---: | --- |
| pubsub | 29 | stage 4 / stage 5; most need the BACKEND to answer |
| param | 27 | **stage 4 W4.a**, which enumerates them |
| graph | 18 | stage 5 (`c:`, 8) and stage 4 (`rust:`, 10); the backend half is phase-444 W3 |
| lifecycle | 15 | stage 4 W4.f |
| service | 14 | stage 4 / stage 5 (W5.e) |
| log | 8 | stage 4 W4.d |
| timer | 6 | stage 4 / stage 5 (W5.c) |
| action | ~~5~~ **0** | **stage 4 W4.b — CLOSED**, the first shard to reach zero |
| qos | 3 | stage 4 |
| init | 2 | W3.b's honouring half, and `rust:Context::domain_id` |
| other | 2 | `rust:Session::serialization_format`, `cpp:State::label` |
| exec, node, boot | 1 each | `cpp:Executor::spin_once` (carries `refuse-loud`), `c:node_get_graph_guard_condition`, `rust:BOOT_SET_NAMESPACE` |

**The count is the phase's own, not an inherited one, and that is the fix for
what went wrong here.** This section previously carried phase-379's 158 and a
pointer to phase-444's 149, neither re-derived; the true figure moved twice
while the pointer stayed put, because the stage-3 loudness waves DELETE rows
when they land. `python3 -c` over the ledger takes a second and is the only
number worth writing down. 16 of the 132 carry a disposition; the rest record a
missing name and nothing else.

For context, the trajectory: phase-379 measured **158** (C++ 82, C 41, Rust 35);
the 2026-09-11 truth pass found 20 of those already closed and 5 mis-verdicted,
giving **149**; the three stage-3 waves that landed on 2026-09-11 removed a
further 17, giving **132**.

**The precedent for how one closes.** `node_get_fully_qualified_name` shipped
for all three languages in PR #567. What made it worth more than the row: the
tree already had three hand-rolled copies of the join, and collapsing them onto
one implementation found that they disagreed — `Node::fully_qualified_name`
returned `my_ns/my_node` for a namespace with no leading slash, which is not
fully qualified. A `gap` row is a prompt to look, and the defect is often
underneath rather than in the missing name.

Two decisions from it that generalise. First, the C form takes a caller buffer
plus an `out_len` rather than rcl's `const char *`, because rcl can return a
pointer only by STORING the joined string and caching it here costs
`MAX_NAME_LEN + MAX_NAMESPACE_LEN` per node to hold what two fields already
hold. Second, it returns `NROS_RET_FULL` and not a `NROS_RET_`-prefixed
`BUFFER_TOO_SMALL` — that constant lives on the RMW ABI
(`NROS_RMW_RET_BUFFER_TOO_SMALL`) and `nros_ret_t` has never defined a twin.
The doc comment that claimed otherwise was issue 1126, now closed: the sweep it
prompted found the same defect in `rmw_vtable.h` and two more places, so the
class is gated (`just check ret-code-citations`).

## Migration track — what moves, and in what order

Inherited from phase-379 W7, which owns steps 1–3 and is in flight. This phase
owns what follows.

* **W-M1** — 379 W7 steps 1–3 (collapse landed rows, execute the W6 verb
  decisions, the remaining open rows). **Stays homed in 379**; listed here so
  the sequence is readable end to end.
* **W-M2** — the disposition pass: every upstream item gets adopt /
  adopt-bounded / refuse-loud / absent (stage 0 W0.b), because a `declined` row
  that does not say what a porting user GETS is not actionable.
* **W-M3** — stages 1–5 above, in dependency order.
* **W-M4** — stage 6 step A (alias), then step B (replace).

## Retirement track — irreversible, batched, once

* **W-R1 — phase-379 W7 step 4**, in flight there: the `nros_param_*` family
  (25 forwarders), the service reply verbs, the QoS `*_raw()`/`*_ms()`
  accessors, `BoardConfig::zenoh_locator` and `ThreadxConfig::zenoh_locator`,
  the five `with_zenoh_locator()` builders.
* **W-R2 — `rclcpp_compat.hpp` itself** (stage 6 step A). Not a deletion of
  capability: its content moves into the headers. What retires is the file and
  the force-include.
* **W-R3 — the `nros::` / `nros_` spellings** (stage 6 step B). The only step
  in this phase that breaks an out-of-tree consumer who did nothing wrong.

Retirement happens once per batch, deliberately, with a changelog entry — never
opportunistically as each rename lands. That rule is phase-379's and it carries
forward unchanged.

## Issues homed here

Every issue this phase owns, with what closing it means. A mention is not an
owner. **Statuses re-read against `docs/issues/` on 2026-09-13** — five rows here
described open work over a resolved-and-archived issue, which is the table doing
the opposite of its job.

**OPEN and owned here: 0793, 1042, 1302, 1303.** That is the whole live set;
everything else in this table is kept as the record of what closing it meant.

| issue | track | closing it means |
| --- | --- | --- |
| 1012 | correction — **RESOLVED, archived** | 15 rows re-worded; a decision on whether tense becomes machine-readable |
| 1019 | correction + loudness (W3.a) — **CLOSED 2026-09-11** | the whole `RCLCPP_*` family routes at `NROS_LOG_*`, so it reaches `nros_log` (and therefore `LOG_ERR`/`printk`) on embedded, carries the logger it was handed, and emits `RCLCPP_FATAL` at `NROS_LOG_SEVERITY_FATAL`; `get_logger(name)` resolves the name through `nros_log_get_logger`. `_STREAM` stopped discarding its message earlier and now inherits the routing. `_THROTTLE` stays REFUSE-LOUD — the refusal is still right (upstream's `clock` argument is load-bearing and `nros_log_throttle_admit` measures on its own clock), but its stated REASON went stale when W4.d shipped `NROS_LOG_*_THROTTLE`: issue 1302. Held by `ros2_loudness_runtime.cpp`, a sink-installing run TU on `just check cpp`; negative control 12 failures / 0 records |
| 1302 | loudness (W3.a) — filed by it, **OPEN** | `NROS_RCLCPP_REFUSE_THROTTLE` names the clock argument as the constraint instead of claiming no C throttle exists, and the five `cpp:RCLCPP_*_THROTTLE` rows agree |
| 1303 | loudness (W3.a) — filed by it, **OPEN** | the two RUNTIME refusals (`init(argc,argv)`, `abort_failed_create`) stop emitting through the legacy no-op sink, so an RTOS image says why it aborted; needs a short runtime form each, and a decision on `failed_create_aborts.cpp`'s link model |
| 1020 | stage 0 — **RESOLVED, archived**. NOTE: phase-444's gap list files it under phase-442; it is resolved, so neither phase owes anything | the C++ lane sees the compat shim; native-API distance and ported-file distance are distinguishable |
| 1022 | correction — **RESOLVED, archived** | ~95 rows corrected; RFC-0036 gains the rule that a divergence's cause must not be one the compared surface also operates under |
| 0793 | stage 2 (W2.a) — **OPEN, C half only** | one parameter store in C. The C++ twin turned out to be a THIRD store and was deleted in phase-426 W4, so that half is closed; what remains is retiring or re-pointing the C `nros_param_*` family |
| 0829 | stage 5 — **RESOLVED, archived** (sentinel implementation, 2026-09-03) | `SYSTEM_DEFAULT` stops disagreeing with itself; folds into the named-profile transcription |
| 1042 | correction (homed 2026-09-11) — **OPEN; the only live item on the correction track** | nine rows went false because the rclrs pin moved 0.5.1 → 0.7.0 and nothing re-measured them. Closing it means the ledger records the pin it was measured against, so the next bump invalidates rows instead of silently outdating them |
| 0589 | stage 4 (W4.d) — **RESOLVED, archived** | the façade re-exports `nros_log`, so it is the easy path rather than `std::println!` |
| 1126 | correction — CLOSED | `nros_publisher_publish_streamed`'s doc (the raw entry point was never the site) stopped promising a `NROS_RET_`-prefixed `BUFFER_TOO_SMALL`; correct-the-doc won, no caller needs the two failures apart. The sweep found two more live sites and the class is now gated |

## What this phase does NOT promise

Not the whole rclcpp surface. Four upstream idioms account for most of what we
decline and three are incompatible with ours by construction: the wait-set /
`Waitable` executor (~128 rows, needs an allocator), runtime-queryable graph and
middleware (~55, needs discovery), runtime type erasure (~45, needs a dynamic
loader). The claim this phase can honestly reach is about PROGRAMS — the shapes
a ROS 2 node is actually written in compile and behave, everything else fails
loudly — and the in-tree ported templates are the measurement.


## Where the remaining work lives — re-measured 2026-09-13

Stage 6's rename is done for the call sites (step B) and the forwarders (W-B5).
The three phases this section spun out are **all finished**, which is a change
since it was written on 2026-09-05:

* **phase-427 — one node type. ARCHIVED 2026-09-11.** `rclcpp::Node` is the
  class, the three C++ node shapes collapsed, `ComponentNode` is deleted
  (measured: every remaining occurrence in the tree is prose about its removal).
* **[phase-426](phase-426-parameters-rust-ssot.md) — parameters get a Rust
  SSoT. W1–W6 all MET**, re-audited 2026-09-12. The document is still ACTIVE and
  archives on one condition: issue 1203, the only open issue it owns. Nothing in
  this phase waits on it.
* **phase-428 — the porting-principle sweep. ARCHIVED 2026-09-12.** Both sweeps
  ran; the findings are ledgered and the checkable ones gated. **Its FIXES were
  never its own and most of them are THIS phase's stage 3**, which is why three
  of those waves landed here on 2026-09-11.

**So the remaining drop-in-replacement work is stages 4 and 5 of this document,
and nothing else.** Stage 4 is the larger half by row count and the one a ported
program notices: our own C, C++ and Rust surfaces disagree about the same
capability in 37 places, and the queue is the 132 `gap` rows above — parameters
(W4.a, 27), lifecycle (W4.f, 15), logging (W4.d, 8), actions (W4.b, 5 -> 0, closed),
executor (W4.c) and guard conditions (W4.e). Stage 5 is the C surface: typed
subscription delivery, the `RCL_RET_*` mapping, rclc-shaped presets, a typed
service path. Everything else on this phase is bookkeeping: three issues
(1042, 1302, 1303) plus 0793's C half, and one changelog entry.

The `#3 batch` (34 `declined` rows the spelling decision flips to `adopt`) stays
here, and W-B6's changelog entry now has phase-427's two loudness items to carry
as well.
