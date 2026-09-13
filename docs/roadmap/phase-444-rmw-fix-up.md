# Phase 444 — RMW fix-up: what the contract report shows, and what the issue records hid

**Status (2026-09-12). SIX of seven items are done — W1, W4, W5, W6 and W7 are
complete and their issues archived (#1088, #1092, #1219, #1021, #1269, #1268),
and W3 landed on `main` in `43b6ec047`. Only W2 remains, and it is the one that
needs a live peer over ENOUGH RUNS to separate a rate from a verdict, which is
a different cost from the rest.**

Re-read against the issue files rather than against this line, which had said
"Not started" while five of its items landed:

| item | issue | state |
| --- | --- | --- |
| W1 | #1088 | done — archived |
| W2 | #0902 | **open** — needs a router and a live peer, over enough runs to separate the rate from the 20–90 % recorded |
| W3 | — | done — `43b6ec047`, the Cyclone graph reader; ten more slots |
| W4 | #1092, #1219 | done — both archived |
| W5 | #1021 | done — archived |
| W6 | #1268 | done — archived. Registered through the generic `register_type` seam (`5572267bb`), and the live acceptance run on a ROS 2 Humble peer: `list`/`get`/`set` on both nodes of a two-node Cyclone image |
| W7 | #1269 | done — archived |

Opened from a review of the RMW report; supersedes phase-393, which is archived
in the same change. Scope grew on 2026-09-11 in two ways: W6 and W7 added two
RMW-level defects a downstream image found (#1268 and #1269), and § "The ROS 2
gap list" gathers the user-API gap reviews in one place, after a truth pass on
their ledger. Remaining order of work: W2, alone.**

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

**Progress 2026-09-12 — all three done; the mechanism is upstream's, the
acceptance is this branch's.**

- [x] **Registered.** `5572267bb` (on `main`) made `create_param_srv` call
      `register_type::<Svc::Request>()` / `::<Svc::Reply>()` before
      `create_service`, exactly as a typed user service does. That was chosen
      over baking the descriptors into the backend the way
      `ParticipantEntitiesInfo` is: the generic seam already existed, every
      other service uses it, and it stays a no-op on zenoh and xrce. This
      branch had built the baked variant in parallel and **dropped it** — two
      mechanisms for one job would register the twelve twice against the same
      `NROS_CYCLONEDDS_MAX_TYPES` budget that `cyclonedds_type_sizing::infra_types`
      now sizes.
- [x] **Reported once, not retried** — landed with issue 1271
      (`ParamState::reconcile_failure_reported`), extended by `5572267bb` to
      name the node FQN and the service suffix and to mark `Unsupported`
      permanent. Verified, not re-done.
- [x] The 0745 comment's stated cause is corrected — `emit_c.rs` upstream, and
      both `service_trailer.jinja` packs plus three goldens here, which is what
      a generated entry actually carries.
- [x] **The live-peer half, which is what the issue's own Acceptance asks for.**
      All three verbs on BOTH nodes of a two-node Cyclone image, against this
      host's ROS 2 Humble peer: `ros2 param list` names `rate`, `get` reads 10
      (20 on the second node), `set … 42` reports success, and a second `get`
      reads 42 back — because a `set` that reports success and changes nothing
      is the same shape as the create this issue is about. All twelve services
      appear in `ros2 service list`, six per node. A SIBLING, not a new harness:
      `param-two-node-talker` takes an `rmw-*` feature and registers through
      `register_linked_rmw()`, row `param-two-node-talker-cyclone` and cell
      `native-params-per-node-rust-cyclone` sit beside their zenoh twins, and
      the four domain-addressed `ros2` helpers share one capture body with the
      locator-addressed originals. A second cell rather than a wider one, for
      issue 1269's stated reason — the zenoh case stayed green for the whole
      time every Cyclone parameter service failed to create.

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

## The ROS 2 gap list (2026-09-11, re-read 2026-09-13)

The rest of this phase is about the RMW layer. This section is about the layer above
it: the user APIs a ported ROS 2 node calls. It is here so that the gap reviews are
listed in one place. Each group below keeps its existing home phase; nothing here
moves ownership.

**It is an INDEX. That rule is now applied to the section itself, which it was
not on 2026-09-11.** An index points; this one restated, and a restatement goes
stale where a pointer does not. Three ways it did, all corrected below:

1. **It said phase-426's "status line still reads 'Planned'" and phase-428's
   "still says the user-API sweep is 'planned'". Both were already false when
   this section was committed** — `8527aaab8` fixed BOTH status lines at
   2026-09-10 19:39, and this section landed in `84926da56` at 2026-09-11 03:17,
   eight hours later. It rebased over the fix and kept its own prose.
2. **The 22 behaviour defects under "Behaviour" were re-read in code on
   2026-09-11 and were true that morning; 12 of the 15 bullets were FIXED
   later the same day** — `bbc4fb4a2` (15:04), `83b1b890c` (15:42) and
   `1d0c76d6e` (18:43), all three of them phase-417 stage 3, the home this
   section correctly names.
3. **The row counts moved with them.** 149 was right on 2026-09-11 and is 132
   today, because a loudness fix DELETES its row.

**This is the same failure mode as the WITHDRAWN survey table at the end of this
document**, and it is worth saying twice: a cross-document claim about another
phase's STATE is stale the moment that phase changes, it lands looking
authoritative, and no gate reads it. The fix is the same both times — point at
the owner and let the owner answer. Counts below were re-derived from the ledger
on 2026-09-13; anyone planning from them should re-derive again rather than
quote this line.

### Where the reviews live

| review | what it measures | status |
| --- | --- | --- |
| RFC-0036 | the catalogue of deliberate divergences from rclc / rclcpp / rclrs | living; gated through the ledger |
| RFC-0089 | the compile-or-conform rule and the four dispositions | settled |
| [phase-379](phase-379-api-parity-with-ros2-client-libraries.md) | `scripts/api-parity.py`: every public item in all three languages, correlated against rclc+rcl (Humble), rclcpp and rclrs v0.7.0; one ledger row per difference in `docs/reference/api-parity-ledger/` | measurement done; its gap rows are the queue |
| [phase-417](phase-417-ros2-api-adoption.md) | taking ROS 2's names | **in flight, and it is the only one still open.** Stages 0–3 and 6 are landed as of 2026-09-13; **stages 4 and 5 are the remaining body of user-API work in the whole campaign** |
| [phase-426](phase-426-parameters-rust-ssot.md) | one parameter store, keyed by node, six services per node | W1–W6 MET and re-audited; active only until issue 1203 closes |
| [phase-427](archived/phase-427-one-node-type.md) | one C++ node type | **ARCHIVED 2026-09-11** |
| [phase-428](archived/phase-428-api-porting-principle-sweep.md) | the class the correlator cannot see: a name we share with upstream whose behaviour differs | **ARCHIVED 2026-09-12** — both sweeps ran, findings ledgered and the checkable ones gated. (This row previously said its status line "still says the user-API sweep is planned"; that was false when written — see the preamble above.) |
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

**The live count lives in [phase-417](phase-417-ros2-api-adoption.md) §"The
`gap` rows are the queue", which derives it from the ledger.** Do not plan from
the number below; re-derive.

**132 `gap` rows on 2026-09-13:** C 39, C++ 58, Rust 35. Sixteen carry a
disposition; the other 116 record a missing name and nothing else.

| shard | gaps, 2026-09-11 | gaps, 2026-09-13 |
| --- | ---: | ---: |
| pubsub | 32 | 29 |
| param | 27 | 27 |
| service | 19 | 14 |
| graph | 18 | 18 |
| lifecycle | 15 | 15 |
| log | 8 | 8 |
| timer | 8 | 6 |
| exec | 6 | 1 |
| action | 5 | 5 |
| other, qos | 3 each | 2, 3 |
| init, node | 2 each | 2, 1 |
| boot | 1 | 1 |
| **total** | **149** | **132** |

**The second column is the point, not a correction.** 17 rows left in two days,
and every one of them left because phase-417 stage 3 fixed the defect and
DELETED the row — `exec` 6 → 1 is the C executor group below. A count in a
document is a measurement with a timestamp; this one had none for two days and
was quoted as current by two other phases.

### Fix-up work, grouped by home

1. **Behaviour: compiles and differs** — phase-428's findings, home
   **[phase-417](phase-417-ros2-api-adoption.md) stage 3**, which is where the
   list now lives. Twelve of the fifteen bullets this section carried were FIXED
   on 2026-09-11, hours after it was written, by the three commits named in the
   preamble; each is recorded with its evidence under phase-417's W3.a, W3.g and
   W3.h, and every affected ledger row is DELETED or re-verdicted. Read those
   items, not a copy here.

   **Still live, re-measured 2026-09-13, with the home for each:**
   - **Zenoh `assert_liveliness` sends nothing on the wire.** Confirmed:
     `nros-rmw-zenoh/src/shim/publisher.rs:420` only updates a local
     `last_assert_at_ms`, which cannot reach a peer. This is an RMW BACKEND
     defect rather than a user-API name, so it is the odd one in this group;
     ledger row `cpp:Publisher::assert_liveliness`.
   - **Rust `Session::serialization_format` falls back to a compile-time
     constant** (`nros-rmw/src/traits.rs:1524`) where the C-ABI adapter's
     vtable slot is `None` (`rmw/cffi/src/lib.rs:262`). Kept on its `differs`
     key in the truth-pass table above, which is its one home.
   - **The `--ros-args` parse is owed in all three languages** — refused loudly
     today, so it is no longer a silent difference. phase-417 **W3.b**'s
     honouring half; ledger row `rust:init_with_args`.
2. **Parameters, 27 rows — home
   [phase-417](phase-417-ros2-api-adoption.md) stage 4 W4.a**, which enumerates
   them by group and is the only place that should.

   The list previously sat here under a claim that phase-426's status line "still
   reads Planned". That claim was false when written (see the preamble) and the
   ownership it implied was wrong too:
   [phase-426](phase-426-parameters-rust-ssot.md) delivered ONE store, keyed by
   node, with six services per node — all six work items MET and re-audited — and
   its "Not in scope" excluded parameter callbacks by construction. These 27 rows
   are missing cross-language SURFACE on top of that store, which is W4.a.

   Two issues attach rather than belonging to the count: **1203** is phase-426's
   and is the only thing keeping that phase open; **0793**'s C half is phase-417
   **W2.a** (its C++ half closed in phase-426 W4). **W6 above is the RMW end of
   the same promise and is done.**
3. **Pubsub, service and graph introspection, 61 rows** (29 + 14 + 18) — home
   **phase-417 stage 4** for the `cpp:`/`rust:` half and **stage 5** for the
   `c:` half.
   - `get_actual_qos` on every entity.
   - Matched counts: `get_subscription_count` and `get_publisher_count`.
   - `*_info_by_topic`.
   - `can_loan_messages`.
   - `get_gid`.
   - Name accessors on the Rust side.
   - The per-node graph queries in C and Rust.
   - Client `prune_*`.
   - Most of these need the backend to answer, and THAT half is here: Cyclone's
     graph slots are W3 above. Lending is issue 0814, homed in
     [phase-433](phase-433-rmw-live-verification.md)'s **loans** row, and has
     never run on hardware.
4. **Lifecycle, 15 rows** — home **phase-417 stage 4 W4.f**, the work item added
   2026-09-13 because this shard previously had no owner named anywhere.
   - No `~/transition_event` publisher (REP-2002).
   - No `LifecyclePublisher` or managed-entity protocol.
   - `register_on_*` differs across our three languages.
   - `get_transition_graph`.
   - `get_clock` and `now` on the lifecycle node.
5. **Our three languages disagree** — home **phase-417 stage 4**; the catalogue
   is issue 0788, homed in
   [phase-381](phase-381-graph-queries-read-the-ros-graph.md).
   - Logging: named loggers and per-logger levels in C and C++, and `rosout`
     (W4.d, part-landed — the Rust façade re-exports `nros_log`, closing 0589).
   - Actions: a goal-id type in C++ (W4.b).
   - Executor: `cancel` and `is_spinning` (W4.c).
   - Guard conditions: one owner (W4.e).
6. **C surface** — home **phase-417 stage 5**:
   - Typed subscription delivery into caller-owned storage (W5.a).
   - `rcl_compat.h` return codes (W5.b).
   - rclc-shaped presets (W5.d).
   - A typed service path (W5.e).
7. **C++ as one freestanding API** — home
   [phase-442](phase-442-one-freestanding-rclcpp-api.md) W0–W10, opened. Issue
   **1245** belongs here and is open. Two corrections: **1020 and 1225 are both
   RESOLVED and archived**, so neither phase owes anything for them; and
   **phase-427 W4 is DONE, not "not started"** — `ComponentNode` is deleted, and
   phase-427 archived 2026-09-11.
8. **Rust facade** — home
   [phase-379](phase-379-api-parity-with-ros2-client-libraries.md), whose issue
   table owns both issues:
   - Issue 0783: `RclReturnCode` is not exported.
   - Issue 0784: `nros::` serves three audiences.
   - `Context::domain_id`, `Node::get_clock`, `Time::to_ros_msg` — three ledger
     rows, closed by phase-417 stage 4.
9. **Semantics the correlator cannot see and that phase-428 has not ledgered:**
   issue **1041**, a repeating timer catching up after a stall where rcl skips —
   home [phase-436](phase-436-poll-wake-revision-deadline-driven-executor.md),
   whose issue table records that the executor arena already defaults to `Skip`
   and names the unswept sibling.

## What this phase does NOT do

* Fix the user-API gaps listed above. Each group has a home phase; the list is here to
  keep them in one place, not to take them over.
* Change the contract. 0 gap stays 0; the 20 declined stay declined with their reasons.
* Verify produced slots against live peers in general — phases 433 and 441.
* Platform build issues (#1039 NuttX, #0852 Zephyr priorities).

## Issues homed here (survey 2026-09-11) — WITHDRAWN

This section listed #1268 and #1269 as homeless. **Both claims were false when
they were written and the survey could not see it.** phase-444 gained
`### W6 — #1268` and `### W7 — #1269` in `84926da56`, which landed on `main`
while the survey branch was open; the branch then rebased over it, so the table
arrived ~200 lines below the very work items it said did not exist, and #1269
had been resolved and archived by W7.

The rows are gone rather than corrected, because there is nothing to correct:
W6 and W7 are the homes, and they are above.

Kept from that survey: `related: [… phase-444]` in issue 1268's frontmatter,
which was right independently of the table.

**What this cost, recorded because it is the survey's own failure mode.** A
homing survey reads the tree once and writes its conclusions as prose. Prose
does not re-derive itself on rebase, so a conclusion about what a document
LACKS is stale the moment someone adds it — and it lands looking authoritative.
The three gates that guard this family all passed: `check-roadmap-claims` reads
a phase's header against its own ticked boxes, not against a table's claim about
another phase's headings; `check-markdown-links` only wanted the #1269 link
repointed to `archived/`, which it got. A cross-document claim of absence has no
gate, and on this evidence wants one — that is the shape
[phase-450](phase-450-gate-reach-narrower-than-its-rule.md) collects.
