# Phase 444 — RMW fix-up: what the contract report shows, and what the issue records hid

**Status (2026-09-11). Not started. Opened from a review of the RMW report; supersedes
phase-393, which is archived in the same change. Scope grew on 2026-09-11 in two
ways: W6 and W7 add two RMW-level defects that a downstream image found (#1268 and #1269),
and § "The ROS 2 gap list" gathers the user-API gap reviews in one place, after a
truth pass on their ledger. Order of work: W1, W6, W7, W4, W5, then W2 and W3.**

Implements RFC-0054 (the C headers are the ABI SSoT). Continues phase-393 (archived).

## Why this phase exists

Phase-393 closed on *"the contract work this doc scoped is finished — what remains is
VERIFICATION"*, and the tools still agree. That claim holds. What it did not cover is
everything between "the slot exists" and "the slot is right": per-backend coverage the
aggregate hides, correctness issues with no home or the wrong home, and issue records
that lag the code.

## The report, measured 2026-09-10

| tool | reading |
| --- | --- |
| `just check rmw-api-parity` | 88 contract symbols, **0 gap**, 20 declined with reasons, 7 answered by an inert slot |
| `just check rmw-abi-shape` | 65 mirrored: 15 identical, 39 with declared arg differences, 9 grouped, **0 undeclared differences, 0 missing slots** |
| `just check rmw-slot-producers` | 68 slots: 58 produced, 4 default, 6 inert |

Phase-393's own status block had drifted from this (it read produced 53, default 7,
inert 14, and listed as inert the on-new-* callbacks, content filter and network-flow
slots that phase-407 declined and removed). Per backend, of 68 slots:

| backend | filled | graph |
| --- | --- | --- |
| cyclonedds | 39 | **1 / 12** |
| rust-adapter (every Rust backend) | 42 | 11 / 12 — a trampoline per slot, so this row cannot answer per backend |
| xrce | 24 | 0 / 12 — declared `UNSUPPORTED`, tested |
| uorb | 18 | 0 / 12 |

## The records lagged the code

Of the 23 open issues in the `rmw` area, **9 had a `fix(...)` commit naming them**. A fix
commit is evidence, not a verdict: #1127 has one and its lane still stops before the
cells. Each was read against its own acceptance:

| issue | verdict | basis |
| --- | --- | --- |
| #1008 | **closed here** | `3941b569a2` deleted `is_server_ready`; `wait_for_service` takes `Ok(true)` only (`handles.rs:2192`); `cffi/tests/server_available.rs` updated in the same commit |
| #1237 | **closed here** | its own Status section: wake slot declined on the board, before/after measured |
| #0969 | **closed here** | both receive paths take wire CDR from the serdata (`subscriber.cpp`, and `take_typed_wire` via `41195b84f8`); cost measured in the issue; publish half was #0970 |
| #1088 | **open — half fixed** | `bd9948e23d` reserves the slot before the destructive take, so nothing is lost; the adapter still maps `WOULD_BLOCK` to `taken = false` + `OK`, which the issue's own Fix rules out; no regression test |
| #0902 | open | two leak arms fixed (`b56e3d50a8` and its predecessor); the 20–90 % symptom never re-measured |
| #1039 | open | 1 of 4 revised-acceptance boxes ticked |
| #1139 | open | "Acceptance, still unmet" |
| #1127 | open | the live-peer lane still stops in its fixture build |
| #0852 | open, **not this phase** | Zephyr transport priorities; 1 of its 5 fix items landed |

## Work items

### W1 — #1088's other half

The Cyclone `service_take_request` adapter still collapses `WOULD_BLOCK` to
`taken = false` with `OK`. That is now contract-correct about *consumption* — the sample
stays on the reader for the next take — but a saturated server is indistinguishable from
an idle one, which is the half the issue's Fix names. Surface exhaustion (a distinct
code or a counted, logged condition) and add the regression test the fix never got.

**Acceptance.** A test with more than `kRequestSlots` requests outstanding that asserts
none is lost AND that exhaustion is observable; it fails against the current adapter.

### W2 — #0902, measured

The mechanism fixes landed without a re-measurement of the symptom. This needs a router
and a live peer, which is why it has not happened.

**Acceptance.** The goal completion rate, measured on a freshly built image over enough
runs to separate it from the 20–90 % the issue recorded.

### W3 — Cyclone's graph reader (11 of 12 slots)

Cyclone fills `get_node_names` and leaves eleven graph slots `nullptr`
(`nros-rmw-cyclonedds/src/vtable.cpp`). Phase-381 W5 scoped one slot, and #0791 and #1137
are both resolved — #1137 correctly ruled the eleven `UNSUPPORTED` answers intended — so
**nothing owns the rest**. A Cyclone node appears in `ros2 node list` and cannot say
whether anything subscribes to its topics: the asymmetry phase-381 named as the defect.
Needs a reader for `ros_discovery_info`, which `graph.cpp` only writes.

**Acceptance.** `rmw-slot-producers` shows cyclonedds graph 12 / 12, and
`native-graph-rust-cyclone-r2n` asserts beyond node names against a live peer. This is
phase-sized and may be split out.

### W4 — gates whose reach is narrower than their rule

* **#1092** — `rmw-abi-shape` licenses a deviation without pinning it; the "39 declared"
  figure above rests on those declarations.
* **#1219** — RFC-0071's `check-rmw-agnostic` was never written.

### W5 — #1021, carried from phase-393

zenoh-pico 1.8.0 does not build with `Z_FEATURE_MATCHING=0`, which Zephyr passes.
Phase-393 adopted it as a backend-build contract; archiving 393 moves it here.

The fix belongs in the zenoh-pico fork. The agent commits it and rebases it onto the
fork's patch line; the maintainer pushes it, and the superproject pin moves after that.

### W6 — #1268, parameter services never start on Cyclone

A downstream image found this: Autoware Safety Island, four C++ component nodes on one
Cyclone executor, with `param_services` declared. All 21 parameters are declared and the
image boots. But `ros2 service list` shows none of the six `rcl_interfaces` services per
node, and `ros2 param` finds nothing. Registration fails with `Unsupported` and is
retried silently on every spin. Zenoh serves the same image correctly, so this is a
backend defect, not a parameter-store one. It is also the RMW half of phase-426's
promise that `ros2 param list` sees the image's parameters.

**Acceptance.** The issue's Fix shape and Acceptance: the six service descriptors are
registered with Cyclone whenever `param-services` is on; a failed registration is
reported once through `nros_log` and not retried; a Cyclone cell reads a parameter back
with `ros2 param get` against a live peer.

### W7 — #1269, the same image shows a different node set on each RMW

The requirement is that `ros2 node list` (and everything that addresses a node by name:
`ros2 param`, `ros2 node info`, launch introspection) sees the nodes the image composes,
whichever RMW it was built with. Today the answer depends on the transport. Cyclone
publishes one `ros_discovery_info` entry for the whole participant, not one per node
record. XRCE has not been measured.

This overlaps W3: both are about Cyclone's graph, W3 reading it and W7 writing it
correctly. Do W7 first, because a reader built against a one-entry-per-participant
writer would measure the wrong thing.

**Acceptance.** One matrix test builds one multi-node image per RMW and asserts the same
`ros2 node list`, so the next divergence fails a test rather than a user.

## A lesson the review surfaced

`#[must_use]` went on `Executor::declare_parameter` (phase-428 W6) with a check of
`cargo check -p nros-node --features std`. The two callers that dropped the value were in
another crate, behind `param-services`, inside a module gated on `rmw-cffi` — never
compiled by that check, which therefore passed. Every `nros` build with those features
then failed on main, including the live-peer lane (fixed in PR #855). A check that does
not enable the features that compile the callers checks nothing: the same "clean over code
it never built" shape as the lane that counted skips as passes.

## The ROS 2 gap list (2026-09-11)

The rest of this phase is about the RMW layer. This section is about the layer above
it: the user APIs a ported ROS 2 node calls. It is here so that the gap reviews are
listed in one place. Each group below keeps its existing home phase; nothing here
moves ownership.

### Where the reviews live

| review | what it measures | status |
| --- | --- | --- |
| RFC-0036 | the catalogue of deliberate divergences from rclc / rclcpp / rclrs | living; gated through the ledger |
| RFC-0089 | the compile-or-conform rule and the four dispositions | settled |
| [phase-379](phase-379-api-parity-with-ros2-client-libraries.md) | `scripts/api-parity.py`: every public item in all three languages, correlated against rclc+rcl (Humble), rclcpp and rclrs v0.7.0; one ledger row per difference in `docs/reference/api-parity-ledger/` | measurement done; its gap rows are the queue |
| [phase-417](phase-417-ros2-api-adoption.md) | taking ROS 2's names, stages 2–5 | in flight |
| [phase-428](phase-428-api-porting-principle-sweep.md) | the class the correlator cannot see: a name we share with upstream whose behaviour differs | the RMW sweep and the user-API findings (W5) are recorded; the status line still says the user-API sweep is "planned" |
| `rmw-api-parity` / `rmw-abi-shape` | our RMW layer against upstream `rmw` | 0 gaps; the remainder is W1–W7 above |

### The truth pass

Last time, 9 of 23 RMW issue records turned out to be already fixed. So before anyone
plans from the ledger, every `gap` row was checked against the live report and then
against the code.

| outcome | rows | basis |
| --- | --- | --- |
| still a missing name (`theirs-only`) | 123 | the correlator finds nothing under that name on our side |
| **closed and deleted** | **20** | the name shipped; see below |
| **re-verdicted `gap` to `divergence`** | **5** | it shipped with a shape a platform constraint forces |
| a behaviour defect under a shared name, re-checked in code and still present | 22 | listed under "Behaviour" below |
| kept on an `ours-only` or `differs` key, and still true | 4 | `rust:BOOT_SET_NAMESPACE` (`nros::main!` still bakes `None`), `rust:init_with_args` (the `--ros-args` parse is still owed), `rust:Session::serialization_format` (the trait still falls back to `"cdr"`), `c:lifecycle_change_state` (still no `~/transition_event` publisher) |

The 20 closed rows:
- C: `node_get_domain_id`, `node_resolve_name`, `difference_times`, `timer_get_time_since_last_call`, `timer_is_canceled` and `timer_is_ready`. C took rcl's spellings.
- C++: `Logger::get_name`, `get_logger` and `FutureReturnCode`.
- C++, where phase-427 made `rclcpp::` the home: `ClockQoS`, `ClockQoS::ClockQoS`, `ParameterEventsQoS` and `ParametersQoS`.
- C++, where phase-427 merged the node types: the four `Node::*_parameter` methods.
- C++ `ParametersQoS::ParametersQoS`: the parameter services now use `parameters_default()` (issue 0793).
- Rust: `log_*` and `NodeHandle::logger`. The facade re-exports `nros_log` now.

The five re-verdicts:
- `c:` and `cpp:` `get_fully_qualified_name`: the caller supplies the buffer, the no-allocator form.
- `cpp:LifecycleNode::shutdown`: the hardcoded transition 5 is fixed, and only the `Result` widening remains.
- `cpp:Timer::cancel` and `reset`: `rclcpp::TimerBase` is now `::nros::Timer`.

**The class is now gated.** A `gap` says "ROS 2 has it and we do not", so once someone
ships the name the key correlates `same`. Because a `same` row needs no ledger entry,
`--check` never looked at it again. `api-parity.py --check` now refuses a `gap` whose
subject correlates (`same` or `systematic`) and that carries no disposition
(`stale_gaps`). The disposition separates the two kinds: phase-428 filed behaviour
defects as `gap` on shared names deliberately, and every one of them carries one.
Negative control: against the pre-pass ledger the gate reports exactly the 18 closed
rows it can reach. The other two closed rows are on an `ours-only` key or a glob, which
it cannot reach. Bound: `just api-parity` runs without `--check`, and issue 1066 is that
no workflow runs `--check` at all. The gate holds locally and nowhere else until 1066
is fixed.

### What is left, measured

**149 `gap` rows:** C 53, C++ 61, Rust 35.

| shard | gaps |
| --- | --- |
| pubsub | 32 |
| param | 27 |
| service | 19 |
| graph | 18 |
| lifecycle | 15 |
| log | 8 |
| timer | 8 |
| exec | 6 |
| action | 5 |
| other, qos | 3 each |
| init, node | 2 each |
| boot | 1 |

33 rows carry a disposition:
- the 22 behaviour defects;
- 2 of the rows on `ours-only` keys;
- 9 missing names whose row already says what a porting user gets.

The other 116 rows record a missing name and nothing else.

### Fix-up work, grouped by home

1. **Behaviour: compiles and differs** (phase-428's findings, home phase-417 stage 3).
   These are the ones a ported node hits without any warning. Each was re-read in code
   on 2026-09-11:
   - C executor:
     - `rclc_executor_trigger_one` reads the entity pointer as an index.
     - `spin_some` returns TIMEOUT on an idle tick.
     - The period spins ignore `set_timeout`.
   - C `rcl_*_is_valid` and the name accessors reject working states: a REGISTERED client, and polling services and subscriptions.
   - C `rcl_node_is_valid` never consults the support object.
   - C `rcl_timer_fini` and `rcl_guard_condition_fini` are not idempotent.
   - C `nros_log_severity_t` is numbered 0–5 against rcutils's 10–50.
   - C `timer_get_time_until_next_call` has no error channel and cannot express overdue.
   - C++ `Publisher::publish` has no `initialized_` guard.
   - C++ `Executor::spin_once` defaults to 10 ms, polls on -1 and drains the ready set.
   - C++ `Client::wait_for_service` defaults to 5 s and cannot wait forever.
   - C++ `RCLCPP_FATAL` lowers to ERROR, and issue 1019: the `RCLCPP_*` family drops the logger.
   - Zenoh `assert_liveliness` sends nothing on the wire.
   - Rust `Session::serialization_format` guesses `"cdr"`.
   - The `--ros-args` parse is still owed in all three languages (refused loudly today).
2. **Parameters, 27 rows.** The phase-426 SSoT has commits for all six work items (the
   C++ stores are gone and one executor-owned store serves the services), although its
   status line still reads "Planned". What remains:
   - Descriptors, ranges and read-only in C and C++.
   - `add_on_set_parameters_callback`.
   - `undeclare` and `delete`.
   - `describe_parameter` and `list_parameters`.
   - The `rclcpp_lifecycle` copies of the parameter surface.
   - Issue 1203: `Executor::parameter` bypasses the reserved-parameter hook.
   - Issue 0793: its C half.
   - W6 above is the RMW end of the same promise.
3. **Pubsub, service and graph introspection, about 69 rows.**
   - `get_actual_qos` on every entity.
   - Matched counts: `get_subscription_count` and `get_publisher_count`.
   - `*_info_by_topic`.
   - `can_loan_messages`.
   - `get_gid`.
   - Name accessors on the Rust side.
   - The per-node graph queries in C and Rust.
   - Client `prune_*`.
   - Most of these need the backend to answer: Cyclone's graph slots are W3 here, and
     issue 0814 is lending, which has never run on hardware.
4. **Lifecycle, 15 rows.**
   - No `~/transition_event` publisher (REP-2002).
   - No `LifecyclePublisher` or managed-entity protocol.
   - `register_on_*` differs across our three languages.
   - `get_transition_graph`.
   - `get_clock` and `now` on the lifecycle node.
5. **Our three languages disagree** (phase-417 stage 4, issue 0788):
   - Logging: named loggers and per-logger levels in C and C++, and `rosout`.
   - Actions: a goal-id type in C++.
   - Executor: `cancel` and `is_spinning`.
   - Guard conditions: one owner.
6. **C surface** (phase-417 stage 5):
   - Typed subscription delivery into caller-owned storage.
   - `rcl_compat.h` return codes.
   - rclc-shaped presets.
   - A typed service path.
7. **C++ as one freestanding API** (phase-442 W0–W10, opened; phase-427 W4
   `ComponentNode` deletion, not started). Issues 1020 (the C++ lane measured the wrong
   surface), 1225 and 1245 belong here.
8. **Rust facade:**
   - Issue 0783: `RclReturnCode` is not exported.
   - Issue 0784: `nros::` serves three audiences.
   - `Context::domain_id`.
   - `Node::get_clock`.
   - `Time::to_ros_msg`.
9. **Semantics the correlator cannot see and that phase-428 has not ledgered:** issue
   1041, where a repeating timer catches up after a stall and rcl skips.

## What this phase does NOT do

* Fix the user-API gaps listed above. Each group has a home phase; the list is here to
  keep them in one place, not to take them over.
* Change the contract. 0 gap stays 0; the 20 declined stay declined with their reasons.
* Verify produced slots against live peers in general — phases 433 and 441.
* Platform build issues (#1039 NuttX, #0852 Zephyr priorities).
