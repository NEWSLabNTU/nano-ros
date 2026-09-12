# Phase 430 — ROS time: the delta after phase-425

**Status (2026-09-13). ARCHIVED, amended once.** [Issue
1334](../../issues/archived/1334-ros-time-fallback-reads-an-unadvanced-steady-counter.md)
is fixed, so delta row 15's decision is now met by IMPLEMENTATION and not only
by record, and row 1's re-verdict loses one of its three arguments while keeping
its verdict. Both are re-verdicted below; the tables above them stay as they
were written. See ["Row 15, re-verdicted
(2026-09-13)"](#row-15-re-verdicted-2026-09-13--met-by-implementation).

**Status (2026-09-12, second pass). ARCHIVED.** W1–W8 landed, row 1 is MET by
implementation, and issue 1321 — the one thing this phase did not close — is
resolved and archived. The single item still owed, W7's ledger rows for
`install_ros_time_source*`, is a property of the parity EXTRACTOR rather than of
this delta, and it is now [issue 1351](../../issues/1351-rust-extractor-misses-nodectx-and-sim-time-surface.md)
so it outlives this document. [Issue 1334](../../issues/archived/1334-ros-time-fallback-reads-an-unadvanced-steady-counter.md),
measured while deciding row 1, was open when this was written and is now fixed
and archived — see the 2026-09-13 amendment at the top.

**Status (2026-09-12). W1–W8 LANDED; row 1's regression is CLOSED — issue 1321
is fixed and archived, and property 1 is MET for the first time by
implementation rather than by absence.** See
["Row 1, re-verdicted (2026-09-12)"](#row-1-re-verdicted-2026-09-12--met) below.
Nothing else in this document changed; the 2026-09-07 and 2026-09-11 tables are
left as they were written.

**Status (2026-09-11). W1–W8 LANDED; ONE row of the 2026-09-07 delta REGRESSED
and is now issue 1321.** Phase-425 owns sim-time and shipped it; this phase was
exactly the delta below and that delta is closed — the clock axis reaches every
node-level surface (W4 Rust, W5 C, W6 C++), the loudness item and both
parameter-semantics corrections are in, and W7 recorded the rulings. What is
NOT closed is row 1: phase-436 W1 gave the executor a timer-derived park bound
nine hours after this table verdicted the property "NO LONGER APPLIES", so the
property applies again and is unmet. It is filed rather than fixed here
([issue 1321](../../issues/archived/1321-ros-time-timer-caps-the-park-on-wall-time.md)),
because it is an executor change and this phase is the reach delta. See
"Re-measured (2026-09-11)" below; the 2026-09-07 table is left as it was
written.

This document was first written as if ROS time were unstarted work. It was not:
phase-425 ([`phase-425-ros-time-clock-semantics.md`](../phase-425-ros-time-clock-semantics.md))
landed `rosgraph_msgs/msg/Clock`, the `/clock` time source, `use_sim_time`,
clock-driven timers and an end-to-end fixture while this was being drafted, and
the collision was found in a rebase conflict rather than by looking. The
2026-09-06 rescope kept the design argument and said the delta was NOT yet
measured. It is now, item by item, against the code on `origin/main` at
`1976727d8` and RFC-0089's ROS-time decisions.

The design reasoning this document used to carry — what ROS time is, the three
properties, the flat-`Timer`-plus-verb shape — is not repeated here. It lives in
RFC-0089 §"Timer, studied against RTOS semantics" (with the 2026-09-05
amendment) and in phase-425's "The situation". Where a property turned out to
be answered differently from what this document argued, the table says so and
the design section is not restated to argue back.

## Delta, measured (2026-09-07)

**Superseded row by row by "Re-measured (2026-09-11)" below. Kept verbatim** —
it is the record of what was true on the day, and six of its verdicts moved
because work landed, not because the measurement was wrong.

Verdicts: **DONE** (425 shipped it — commit, file:line), **PARTIAL** (what is
missing, one line), **NOT STARTED**, **NO LONGER APPLIES** (the design settled
it another way). Commits are on `origin/main`; line numbers are the tree this
was measured in, `phase-428-w10-qos-ssot` as it stood on 2026-09-07 (PR #629,
then 45 commits ahead of `main` and none behind).

| # | Item (source) | Verdict | Evidence |
| --- | --- | --- | --- |
| 1 | Property 1 — a ROS-time timer's wake source is a message; the wall timeout handed to the platform must come from wall timers only (430 W1) | NO LONGER APPLIES | The executor derives NO wait from timers at all: `spin_once(timeout)` is the caller's, capped only by the session's next internal deadline (`executor/spin.rs:6441`). Every timer, wall or ROS, is polled in `timer_try_process` (`executor/arena.rs:1959`) and fires on a spin boundary; a `/clock` arrival is data, so it wakes `drive_io` like any sample. There is no second deadline set to build. The latency bound is the spin cadence, which is why `bins/sim-clock-listener` spins at 5 ms. |
| 2 | A 100 ms ROS timer fires once per 100 ms of RECEIVED time at whatever wall rate the samples arrive (430 W1 acceptance) | DONE | `8cfd19315`; `ros_time_timer_follows_the_simulated_clock` step 2 (`executor/tests.rs`, "RATE": four 100 ms steps in ~40 ms wall = four activations); end-to-end `tests/sim_time_clock_e2e.rs` (`6ec3764f2`) measured ~98 ROS activations/s against 10 wall on a 10x replay. |
| 3 | `rosgraph_msgs/msg/Clock` + the `/clock` subscription (430 W2) | DONE | `caa546fb2` (`packages/interfaces/rosgraph-msgs`), `f176079a5` (`Executor::install_ros_time_source`, `executor/spin.rs:9291`; `time_source.rs`), `QoSProfile::clock_default()` = rclcpp's `ClockQoS`. |
| 4 | `use_sim_time` is a real parameter that attaches the source (430 W2) | DONE | `5200437c5`; hook at `Executor::declare_parameter` → `note_reserved_parameter` (`executor/spin.rs:7943`, `:7971`), reconciled at the head of every spin (`reconcile_ros_time_source`, `:9221`). Unit: `use_sim_time_attaches_and_detaches_the_clock_source`, `a_non_bool_use_sim_time_attaches_nothing`. |
| 5 | A runtime `ros2 param set … use_sim_time true` is honoured or refused loudly (430 W2 acceptance) | PARTIAL | Honoured when the app DECLARED `use_sim_time` (`refresh_use_sim_time_from_store`, `executor/spin.rs:9257`, after a parameter service handled a request). On a node that never declared it the store refuses the set (`allow_undeclared` is false, `nros-params/src/server.rs:216`) — refused, but not how ROS 2 behaves: rclcpp declares `use_sim_time=false` on EVERY node, so the `ros2 param set` works everywhere and `ros2 param list` shows it. Ours shows nothing until the app names it. → **W2**. |
| 6 | Backward jump resets outstanding deadlines instead of stalling or storming (430 W3, property 2) | DONE | `executor/arena.rs:1970` (`step_ns < 0` → `elapsed_us = 0`, no activation); test step 3 ("back 9.4 s … the NEXT period must fire on schedule"). |
| 7 | Forward jump fires at most once per timer (430 W3) | DONE | Under the default `TimerOverrunPolicy::Skip` a backlog coalesces into ONE activation (`executor/arena.rs:1981`); `CatchUp` replays, by explicit opt-in. Documented in `book/src/user-guide/simulated-time.md` "A jump forwards". |
| 8 | Pause stops ROS timers and leaves wall timers running (430 W3) | DONE | Test step 1 (360 ms wall, 0 ROS activations, ≥3 wall); e2e asserts the ROS timer stops DEAD when `/clock` stops while the wall timer keeps cadence. |
| 9 | C++ `create_timer(clock, …)` beside `create_wall_timer` (430 W4; RFC-0089's verb) | DONE | `2829ab92a` freed the name (no alias); `8cfd19315` added `Node::create_timer(Timer&, const Clock&, period_ms, cb, ctx)` (`nros-cpp/include/nros/node.hpp:794`) and `nros::create_timer(Node&, Timer&, const Clock&, ms, fn)` (`std_compat.hpp:111`). ONE dispatch: `nros_cpp_timer_create_on_clock` → `Executor::register_timer_on_clock` (`nros-cpp/src/timer.rs:145`). |
| 10 | The Rust verb (430 W4) | PARTIAL | Executor level DONE: `Executor::register_timer_on_clock(period, TimerClockSource, cb)` (`executor/spin.rs:5124`), `nros::TimerClockSource` re-exported (`api/nros/src/lib.rs:1028`). NOT on `NodeCtx` (its only timer verb is `create_timer_in`, `executor/node.rs:1567`) and NOT on the declarative `nros::Node::create_timer` (`api/nros/src/node.rs:1084` — `EntityMetadata` has no clock field, and `node_runtime.rs:1459` always calls `register_timer`). A `nros::main!` component cannot own a ROS-time timer today. → **W4**. |
| 11 | The C verb (430 W4; RFC-0089 "C takes rcl's spellings") | NOT STARTED | `nros_timer_init(timer, support, period_ns, cb, ctx)` (`nros-c/src/timer.rs:117`) takes no clock; `rcl_timer_init` takes an `rcl_clock_t*`. `rclc_executor_add_timer` registers via `register_timer` only (`nros-c/src/executor.rs:1931`). → **W5**. |
| 12 | `ComponentNode` (430 W4) | NOT STARTED | Only `create_wall_timer` (`component_node.hpp:425`, `:442`); no clock-taking overload, no `NROS_CREATE_TIMER` macro. → **W6**. |
| 13 | The hosted `rclcpp::` spelling a ported file writes (430 W4 acceptance: "the same file using `create_timer(clock, …)` follows the bag") | NOT STARTED | rclcpp humble has exactly one clock-taking form, the FREE `rclcpp::create_timer(node, clock, period, cb[, group])` (`docs/reference/api-surface/rclcpp.json:9545`). The hosted shim has `rclcpp::Node::create_wall_timer` → `TimerBase::SharedPtr` (`nros.hpp:812`) and nothing for the clock-taking verb. A ported file using ROS time fails to compile — honest under the compile-or-conform rule, but the verb 430 called portable is the one that is missing. → **W6**. |
| 14 | `Node::create_timer(period, cb)` with no clock follows the node's clock (430 W4) | NO LONGER APPLIES | Not in the captured upstream surface: rclcpp humble has no clock-less `Node::create_timer` member (`rclcpp.json` lists `Node::create_wall_timer` and the free `rclcpp::create_timer` only); it arrived in Iron. Nothing to port until the surface bumps; noted in W6 so it is not re-invented ahead of the surface. |
| 15 | Property 3 — a ROS-time timer with no `/clock` must NOT fall back to wall time (430 W5) | NO LONGER APPLIES | 425 chose the opposite, deliberately and with rclcpp: a `Ros` timer with no override reads system time (`executor/arena.rs:232`, `node.hpp:781`, `nros-core/src/clock.rs` `RosTime` arm). "A node written for simulation still runs standalone" is upstream's contract, and a timer that refuses to fire would be a compile-and-differ of its own. The behaviour was decided, not skipped. |
| 16 | …and the image SAYS so once, so "paused" is distinguishable from "misconfigured" (430 W5) | NOT STARTED | Nothing reports `use_sim_time` true with no sample ever received; `Clock::started()` (`clock.hpp`) is a poll the app has to think to make. → **W1**. |
| 17 | Feature-gated, off by default on freestanding targets (430 W6) | DONE | `sim-time` feature on `nros-node` and forwarded by the `nros` umbrella (`api/nros/Cargo.toml:199`), additionally gated on `has_rmw`. |
| 18 | Byte-identical timer scheduling without the feature (430 W6 acceptance) | PARTIAL | `TimerEntry` carries `clock_source` + `last_clock_ns` unconditionally (`executor/arena.rs:282`, ~16 B per timer entry) and `timer_try_process` matches on the source every poll; the `Steady` arm is the pre-425 code path. Measured cost is below the noise of one arena slot; recorded here as the decision NOT to fork the struct on a cfg (→ **W8**, closed by this note). The cost line lives in `time_source.rs`'s module doc and the book page, not beside `ZPICO_MAX_QUERYABLES`. |
| 19 | Runtime clock-source enum, not a type parameter (RFC-0089 amendment) | DONE | `TimerClockSource { Steady, Ros, System }` (`executor/arena.rs:227`), `#[repr(u8)]`. |
| 20 | A clock field on the flat `Timer` (RFC-0089 amendment) | DONE, better than asked | The field lives in the ARENA entry, not the C++ handle: `nros::Timer` is still `{executor_, handle_id_, initialized_}`, so `check-cpp-capability-layout` was never touched. |
| 21 | No `TimerBase`, no hierarchy (RFC-0089 §"Timer, studied…" decision) | DONE by 425; contradicted elsewhere | 425 added no type. But phase-417 W1.a had already added a hosted `rclcpp::TimerBase` with a virtual destructor and `detail::WallTimer : TimerBase` (`nros-cpp/include/nros/timer.hpp:232`) — see finding A. |
| 22 | `Clock::now()` under sim time, all three languages | DONE | Rust `Clock::ros_time().now()` reads the override (`nros-core/src/clock.rs`); C `nros_clock_get_now` → `get_ros_time_ns` (`nros-c/src/clock.rs:116`); C++ `node.get_clock()->now()` (a `Node`'s clock is `NROS_CLOCK_ROS_TIME`, `node.hpp:218`). |
| 23 | `sleep_for` / `sleep_until` / `wait_until_started` on a ROS clock | NO LONGER APPLIES | Declined by RFC-0021 (a blocking helper that does not drive the executor): ledger rows `cpp:Clock::sleep_for`, `cpp:Clock::sleep_until`, `cpp:Clock::wait_until_started`; book "Limits". `rclcpp::Rate` here is wall (`nros_cpp_time_ns()`, `nros.hpp:1228`) and so is humble's (`GenericRate<system_clock>`); a ROS-time `Rate` is a Jazzy+ surface. |
| 24 | rosbag behaviours: rate ≠ 1, loop (time backwards), pause | DONE | Rows 2, 6, 8. One interaction the fixture found and both sides now document: each `/clock` sample is a jump of `step × rate`, so under `Skip` the TICK rate is 1x while `now()` runs at `rate` unless `step × rate ≤ period` (`sim_time_clock_e2e.rs` `STEP_MS`). |
| 25 | The red this doc cited on 2026-09-06 (`use_sim_time_attaches_and_detaches_the_clock_source` failing at `is_active()` on pristine `main`) | DONE (fixed) | `7195c9e00` (issue 1104): the two tests that share the process-global `/clock` gate take a lock and restore what they found (`SimTimeGuard`). Green, see below. |

## Re-measured (2026-09-11)

Every one of the 25 rows above re-read against `origin/main` at `fa103ab94`.
Same vocabulary. **Six moved because work landed, one moved because a DIFFERENT
phase removed the premise its verdict rested on, and one lost its
"contradicted elsewhere" tail.** The other seventeen stand; where a file:line
in the 2026-09-07 column no longer resolves, the current one is given, because
a citation that does not resolve is not evidence.

The measurement above was taken on the `phase-428-w10-qos-ssot` branch; this
one is on `main`, so a row's line numbers can move without its verdict moving.

| # | 2026-09-07 | 2026-09-11 | What moved, and the evidence |
| --- | --- | --- | --- |
| 1 | NO LONGER APPLIES | **NOT STARTED** | **REGRESSED, and it is the one bad row in this table.** The 2026-09-07 verdict rested on "the executor derives NO wait from timers at all". `d1a1c7a32` (phase-436 W1, issue 1192) merged 2026-09-10T15:47Z — nine hours after this table's own `f447547d1` at 06:23Z — and gave it one: `next_timer_deadline_us` (`executor/spin.rs:2838`) offers `period_us - elapsed_us` as a park bound and never reads `clock_source`, which `TimerHeader` carries (`executor/arena.rs:265`). For a `Ros` timer that quantity is SIMULATED microseconds and the park primitive is handed it as WALL (`spin.rs:2711`, `:7093`–`7130`). Bounded — it can only shorten a park, so nothing fires early — but property 1 is exactly this and it is unmet. Filed as **issue 1321**, not fixed here: it is an executor change, and this phase is the reach delta. **Superseded 2026-09-12 — see "Row 1, re-verdicted" below; 1321 is fixed and archived.** |
| 2 | DONE | DONE | Stands. `8cfd19315`; `ros_time_timer_follows_the_simulated_clock` (`executor/tests.rs:2628`); `6ec3764f2` for `packages/testing/nros-tests/tests/sim_time_clock_e2e.rs`. |
| 3 | DONE | DONE | Stands. `caa546fb2` (`packages/interfaces/rosgraph-msgs`), `f176079a5`. Lines moved: `Executor::install_ros_time_source` is `executor/spin.rs:10855`, `QoSProfile::clock_default` is `nros-rmw/src/traits.rs:1178`. |
| 4 | DONE | DONE | Stands. `5200437c5`. Lines moved: `note_reserved_parameter` `executor/spin.rs:9171`, `reconcile_ros_time_source` `:10664`. |
| 5 | PARTIAL | **DONE** | W2 landed: `004a14309`. `seed_use_sim_time_default` (`executor/spin.rs:8984`) declares `use_sim_time = Bool(false)` from `ensure_parameter_store` (`:8936`, seeded at `:8944` and `:8512`), so a node that never named it is listed and settable, as rclcpp does. An app's own declaration still wins — `yield_seeded_use_sim_time` (`:9011`) steps the placeholder aside at `:9115`, and a refused app declaration puts the default back (`:9126`). Book: `book/src/user-guide/simulated-time.md:62`. |
| 6 | DONE | DONE | Stands. `executor/arena.rs:2215` (`step_ns < 0`) → `:2222` (`elapsed_us = 0`, no activation). |
| 7 | DONE | DONE | Stands. `executor/arena.rs:2257` — under `TimerOverrunPolicy::Skip` a backlog coalesces into ONE activation. |
| 8 | DONE | DONE | Stands. Test step 1 in `executor/tests.rs:2628`; `sim_time_clock_e2e.rs`. |
| 9 | DONE | DONE | Stands. `2829ab92a` freed the name, `8cfd19315` added the verb. Lines moved: the member is `nros-cpp/include/nros/node.hpp:1446`, the free form `std_compat.hpp:111`, the one dispatch `nros-cpp/src/timer.rs:97` → `:144`. |
| 10 | PARTIAL | **DONE** | W4 landed, both halves. Imperative: `NodeCtx::create_timer_on_clock` (`nros-node/src/executor/node.rs:1612`) and `create_timer_on_clock_in_group` (`:1636`), commit `10c8a3e52`. Declarative: `nros::Node::create_timer_on_clock` (`api/nros/src/node.rs:1176`), with `create_timer` now delegating to it under `TimerClockSource::Steady` (`:1138`), commit `6039c5c65`. A `nros::main!` component can own a ROS-time timer. Ledger rows exist: `rust:Node::create_timer_on_clock`, `rust:Node::create_timer_on_clock_in_group`, `rust:DeclaredNode::create_timer_for_callback_name_on_clock`. **Re-keyed 2026-09-12 (issue 1351)** — the first two were written as `rust:NodeCtx::*`, which is a spelling the report can never print: the correlator folds `NodeCtx` onto `Node` (`TYPE_SYNONYMS`), so both rows were inert from the day they were filed. They were guessed because the extractor could not reach `NodeCtx` to be asked. |
| 11 | NOT STARTED | **DONE** | W5 landed: `592452ffe`. `nros_timer_init_on_clock(timer, clock, support, period_ns, cb, ctx)` — clock in rcl's position — at `nros-c/src/timer.rs:195`, documented at `nros-c/include/nros/rcl_compat.h:389`; both init verbs share `timer_init_inner` (`timer.rs:213`). The clock→source mapping has ONE implementation, `nros_timer_clock_source` (`nros-c/src/clock.rs:81`), which `rclc_executor_add_timer` reads at `nros-c/src/executor.rs:1949` and the C++ verb calls at `nros-cpp/src/timer.rs:126` — so the drift the work item warned about did not happen. Spelled `nros_timer_clock_source`, not the `TimerClockSource::from_clock_type` W5 proposed. Ledger row `c:timer_init_on_clock` written. |
| 12 | NOT STARTED | **NO LONGER APPLIES** | `ComponentNode` is DELETED — `packages/api/nros-cpp/include/nros/component_node.hpp` is gone, removed by `1f3b88aec` (phase-427 W4, "it wrapped a node, and now there is one"). There is one node type, so the clock verb this row wanted IS row 9's `rclcpp::Node::create_timer`. Nothing to port; W6's `ComponentNode` half is void. |
| 13 | NOT STARTED | **DONE** | W6's hosted half landed with `f4c5ea765`: the free `rclcpp::create_timer(node, clock, period, callback)` at `nros-cpp/include/nros/nros.hpp:935`, plus the `std::chrono` overload at `:962`, returning the same `std::shared_ptr<::nros::Timer>` cell `create_wall_timer` returns. Humble's only clock-taking form, which is what this row asked for. Ledger: `cpp:create_timer`, `disposition: adopt-bounded`. |
| 14 | NO LONGER APPLIES | NO LONGER APPLIES | Stands, and the tree now says so at the site: `nros.hpp:908`, "NOT ADDED: a clock-less `Node::create_timer(period, callback)`" — held back until the captured surface is Iron or later, exactly as W6 required. |
| 15 | NO LONGER APPLIES | NO LONGER APPLIES | Stands. Citation moved with the type: `TimerClockSource::Ros` is documented as "simulated time when a `/clock` source is active, and system time when none is (the same fallback `rclcpp::Clock` has…)" at `nros-node/src/timer.rs:164`–`167`. |
| 16 | NOT STARTED | **DONE** | W1 landed: `9942827bc`. `SILENCE_WARN_US` (`nros-node/src/time_source.rs:119`) with the one-shot `SilenceWatch` (`:134`, armed at `:198`), five unit tests over the watch (`:251`–`:335`), and an executor accessor so the assertion is not a log grep. Book "Limits": `book/src/user-guide/simulated-time.md:142`, re-arm at `:154`. |
| 17 | DONE | DONE | Stands. `sim-time = ["nros-node/sim-time"]`, `packages/api/nros/Cargo.toml:206`. |
| 18 | PARTIAL | PARTIAL | Stands, and it is W8's recorded decision rather than owed work. `TimerEntry` still carries `clock_source` + `last_clock_ns` unconditionally (`executor/arena.rs:243`/`:246`), mirrored in `TimerHeader` (`:265`/`:266`). Not re-litigated. |
| 19 | DONE | DONE | Stands. The enum MOVED out of the arena: `pub enum TimerClockSource { Steady, Ros, System }`, `#[repr(u8)]`, is now `packages/core/nros-node/src/timer.rs:158`, re-exported into `arena` at `arena.rs:26`. |
| 20 | DONE, better than asked | DONE, better than asked | Stands. `nros::Timer` is still `{ void* executor_; size_t handle_id_; bool initialized_; }` — `nros-cpp/include/nros/timer.hpp:165`–`167`. The clock stayed in the arena entry. |
| 21 | DONE by 425; contradicted elsewhere | **DONE** | The contradiction is RESOLVED by W7, and not in one direction. The HIERARCHY is deleted (`class TimerBase` with its virtual destructor, and `detail::WallTimer : TimerBase`), argued from the executor's dispatch at `nros-cpp/include/nros/timer.hpp:180`–`210`. The NAME came back as a hosted ALIAS — `using TimerBase = ::nros::Timer;` (`timer.hpp:305`) — because deleting it turned `colcon-parity` red: upstream has `TimerBase` and no `Timer`, so with no alias no spelling of a timer member compiles both against real rclcpp and here. RFC-0089 records both halves (§"AMENDED 2026-09-09", §"AMENDED 2026-09-08" item 1) and the alias rule they produced. |
| 22 | DONE | DONE | Stands. C `nros_clock_get_now` (`nros-c/src/clock.rs:212`); C++ a `Node`'s clock is `NROS_CLOCK_ROS_TIME` (`nros-cpp/include/nros/node.hpp:539`). |
| 23 | NO LONGER APPLIES | NO LONGER APPLIES | Stands. Ledger rows still present and still declined: `cpp:Clock::sleep_for`, `cpp:Clock::sleep_until`, `cpp:Clock::wait_until_started` (`docs/reference/api-parity-ledger/timer.json:302`, `:307`, `:312`). |
| 24 | DONE | DONE | Stands. Rows 2, 6, 8; the `STEP_MS × RATE ≤ PERIOD_MS` interaction is documented at `packages/testing/nros-tests/tests/sim_time_clock_e2e.rs:36`. |
| 25 | DONE (fixed) | DONE (fixed) | Stands. `7195c9e00`; `SimTimeGuard` at `nros-node/src/executor/tests.rs:31`, restoring in `Drop` at `:52`. |

### What landed, per work item

* **W1 [loudness] — LANDED** `9942827bc`. Row 16.
* **W2 [params] — LANDED** `004a14309`. Row 5.
* **W3 [correctness] — LANDED**, issue 1202. The reserved-parameter hook now
  runs AFTER `params.server.declare(...)` and only on acceptance
  (`executor/spin.rs:9141`–`9144`); the comment at `:9134`–`9140` records the
  state the old order could produce — store `true`, source detached — and names
  it as neither of the two states the parameter can express. Finding E closed.
* **W4 [api, Rust] — LANDED** `10c8a3e52` + `6039c5c65`. Row 10.
* **W5 [api, C] — LANDED** `592452ffe`. Row 11.
* **W6 [api, C++] — LANDED** `f4c5ea765`, and SMALLER than written: its
  `ComponentNode` half was voided by that type's deletion (row 12), so what
  landed is the hosted free `rclcpp::create_timer` (row 13) beside the member
  `rclcpp::Node::create_timer(Timer&, const Clock&, …)` (row 9). The clock-less
  `Node::create_timer` stays refused (row 14).
* **W7 [ledger, RFC, docs] — LANDED. The one owed row is PAID, 2026-09-12.** The
  `TimerBase` ruling is in RFC-0089 (§"AMENDED 2026-09-08" item 1) and at the
  header (`timer.hpp:180`), and the rows W4–W6 name exist
  (`rust:Node::create_timer_on_clock`, `c:timer_init_on_clock`,
  `cpp:create_timer`).

  What this bullet used to say — "still owed, deliberately: rows for
  `install_ros_time_source*`, which wait on the Rust extractor building the
  surface with `sim-time`" (finding C) — was tracked as
  [issue 1351](../../issues/archived/1351-rust-extractor-misses-nodectx-and-sim-time-surface.md),
  because it was a property of the TOOL rather than of this phase's delta:
  `sim-time` was absent from `NROS_FEATURES` and `NodeCtx` was not re-exported
  from the umbrella, so the rows were unmatchable BY CONSTRUCTION rather than
  unwritten — two states a reader and every gate saw as one blank.

  **1351 is fixed and the debt is settled.** `sim-time` joined `NROS_FEATURES`,
  `NodeCtx` is `pub use`d from the umbrella (it was already public API by value —
  `Executor::node_mut` returns it and the `nros` crate's own first doc example
  calls its methods — and only the TYPE had no path), and the widening put 25
  rows on the measured surface that had never been there. All 25 are ledgered,
  `rust:Node::install_ros_time_source` and `_on` among them. Two corrections
  fell out that this document could not have made: W4's two rows were keyed
  `rust:NodeCtx::*`, a spelling the correlator can never emit, and two EXISTING
  rows (`rust:Node::create_service`, `rust:Node::create_subscription`) argued
  from an absence that the widening disproved with no signature moving at all.
  The class is now gated — `check-ledger-key-spelling` refuses a key the report
  cannot print, which is the half of "does this row have a subject?" that needs
  no extractor (the other half is issue 1323).

  The one thing that was simply stale is fixed by the same pass that wrote this:
  RFC-0089's 2026-09-05 amendment still read "phase-430 brings ROS time" four
  days after this document had measured that phase-425 brought it.
* **W8 [cost] — CLOSED by row 18**, unchanged.

**What this phase did not close: issue 1321** — written 2026-09-11. It is a
phase-436 regression on this phase's property 1 rather than an item of the
delta, and it was owned by the issue. **It is closed now**; the re-verdict is
the next section, and the issue is archived.

### Row 1, re-verdicted (2026-09-12) — MET

| # | 2026-09-07 | 2026-09-11 | 2026-09-12 | Evidence |
| --- | --- | --- | --- | --- |
| 1 | NO LONGER APPLIES | NOT STARTED | **MET** | A timer bounds the executor's wall park only if its OWN clock runs in wall microseconds — `TimerClockSource::remaining_is_wall_time` (`packages/core/nros-node/src/timer.rs`), read by `next_timer_deadline_us` and by `audit_spin_quantization` (`executor/spin.rs`). [issue 1321](../../issues/archived/1321-ros-time-timer-caps-the-park-on-wall-time.md), archived. |

**Property 1 now reads, post-fix:** *a ROS-time timer's wake source is a
`/clock` message, so it contributes NOTHING to the wall timeout handed to the
platform; the timeout comes from the timers whose clock is wall time —
`Steady` and `System` — plus the caller's budget and the session's next
event.*

Three things that matters for, and all three are decisions rather than
restatements of the 2026-09-07 verdict:

* **This is the first time the property is met by IMPLEMENTATION.** The
  2026-09-07 verdict "NO LONGER APPLIES" rested on the executor deriving no
  wait from timers at all — the property was satisfied because the mechanism
  did not exist, which is why one unrelated phase could remove it nine hours
  later without anyone noticing. It is now satisfied by a predicate that
  names the clock, with a test that fails if the predicate is removed.
* **The three clock sources are decided SEPARATELY, not collapsed into
  `!= Steady`.** `System` keeps its bound: its clock IS wall time, so there
  is no rate indirection and nothing can hold it still. It can STEP (NTP),
  which makes the bound an estimate rather than a fact — but a bounded one,
  because the bound only ever shortens a park and `timer_try_process` re-reads
  the clock before dispatching, so a step costs at most one extra wake and can
  never fire a callback early.
* **`Ros` contributes nothing UNCONDITIONALLY**, not "unless a `/clock` source
  happens to be attached". *(Amended 2026-09-13 — the verdict stands, one of
  its two reasons does not.)* As written, the first reason was that the
  conditional rule would be wrong TODAY: `ClockType::RosTime`'s documented wall
  fallback read an in-image steady counter nothing advances
  ([issue 1334](../../issues/archived/1334-ros-time-fallback-reads-an-unadvanced-steady-counter.md),
  measured while deciding this), so the remainder was a constant in that state
  too. 1334 is fixed — that arm now reads the wall clock — so the reason is
  gone and the rule rests on the second, which was always the stronger and is
  about the RULE rather than one image's state: a park bound that flipped with
  whether a publisher was running would be a behaviour no declaration named,
  and it would flip at the first `/clock` sample, which is the instant the
  remainder stops being wall microseconds. (Deciding it per-timer at
  registration is not available either: `use_sim_time` is settable at runtime,
  W2.) The latency cost is the pre-436 bound, the caller's budget, which is
  what the e2e fixture's 5 ms `sim-clock-listener` cadence already assumes; the
  park is never unbounded, because `next_wake_bound_attributed_us` seeds its
  `min` with that budget. What 1334 changes is that the cost is now
  OBSERVABLE — before it, a standalone `Ros` timer never fired at all, so
  contributing nothing to the park cost nothing.

The same sweep found a third site with the same units error and WITHOUT the
"bounded, over-waking only" mitigation: issue 0736's budget-skip path advanced
every timer's `elapsed_us` by the executor's wall delta, which un-pauses a ROS
timer on a paused `/clock` — row 8's property, broken on one path. Both sites
now go through `arena::timer_clock_step`. Negative control, pre-fix: 202 359 µs
accumulated over five spins with `/clock` held still, i.e. two activations a
100 ms ROS timer had not earned.

### Row 15, re-verdicted (2026-09-13) — MET BY IMPLEMENTATION

| # | 2026-09-07 | 2026-09-11 | 2026-09-13 | Evidence |
| --- | --- | --- | --- | --- |
| 15 | NO LONGER APPLIES | NO LONGER APPLIES | **NO LONGER APPLIES, and now IMPLEMENTED** | `Clock::wall_now()` (`packages/core/nros-core/src/clock.rs`) is the ONE expression both `ClockType::SystemTime` and `ClockType::RosTime`'s no-override arm evaluate. [issue 1334](../../issues/archived/1334-ros-time-fallback-reads-an-unadvanced-steady-counter.md), archived. |

Row 15 verdicted property 3 ("a ROS-time timer with no `/clock` must NOT fall
back to wall time") NO LONGER APPLIES, on the ground that 425 had chosen the
opposite deliberately and with rclcpp. That was right about the decision and
wrong about the code, and the row's own evidence line named the site where:
it cited `nros-core/src/clock.rs`'s `RosTime` arm as reading system time, and
that arm read an in-image steady counter nothing advances. So the decision was
made, written down in five places across three languages, cited here as
evidence — and not implemented, for four phases. A standalone `Ros` timer took
neither the documented arm (wall) nor the refused one (do not fire): it took a
third that reads as "do not fire" by accident.

This does not re-open the design question, and the alternative was weighed
again rather than assumed: making the fallback REFUSE or report would
contradict 425's recorded choice and RFC-0089's "a node built for simulation
still runs standalone", and `use_sim_time` false is not a misconfiguration —
it is every standalone node. The state that IS a misconfiguration, sim time on
with no `/clock` ever, already has W1's one-shot report (row 16), and that
report's own text — "ROS-time timers are running on SYSTEM time meanwhile" —
is what this fix makes true. No second spelling was added.

### The tests, run (2026-09-07)

`just check node-std-tests`' first line, on this branch:

```
cargo test -p nros-node --lib --features std,sim-time,param-services --quiet
test result: ok. 366 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 1.46s
```

and the five sim-time tests by name:

```
test time_source::tests::a_pre_epoch_sample_installs_nothing ... ok
test time_source::tests::a_clock_sample_becomes_nanoseconds ... ok
test executor::tests::a_non_bool_use_sim_time_attaches_nothing ... ok
test executor::tests::ros_time_timer_follows_the_simulated_clock ... ok
test executor::tests::use_sim_time_attaches_and_detaches_the_clock_source ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 363 filtered out
```

**The lane did not compile on this branch before this PR.** Phase-428 W6 made
`ParameterServer::declare` and `Executor::declare_parameter` `#[must_use]`
and swept no test caller: eleven `unused return value … that must be used`
errors (eight in `parameter_services.rs`'s tests, three in the sim-time tests)
took `check node-std-tests` red on `phase-428-w10-qos-ssot`, invisible to the
PR gate because that lane is not in it. Fixed in this PR's first commit, as
`assert!`s — a declaration a test depends on is a precondition. The fixture
test (`sim_time_clock_e2e.rs`, cyclone, two processes) was not run here.

### Reverse check — what the tree holds that RFC-0089's design did not ask for

* **A. `rclcpp::TimerBase` exists, with a vtable.** Not 425's: phase-417 W1.a
  added `class TimerBase { virtual ~TimerBase(); }` and
  `detail::WallTimer : TimerBase` under `NROS_CPP_HAS_SHARED_PTR`
  (`timer.hpp:232`), so that `rclcpp::TimerBase::SharedPtr timer_;` names
  something in a ported file. RFC-0089 §"Timer, studied against RTOS
  semantics" decided "No `TimerBase`, no `WallTimer`, no `GenericTimer`" and
  the amendment repeats "no `TimerBase`". The RFC and the header disagree, and
  the vtable is exactly the kind RFC-0089 clause 1 refuses: no dispatch uses
  it (`WallTimer::trampoline` is a static function; the only virtual is the
  destructor). It is hosted-only and costs a freestanding image nothing, which
  is the argument for keeping it. **Whichever way this settles, the RFC must
  say it** (→ W7), and W6 builds on the ruling: a hosted `create_timer(clock,
  …)` returns the SAME cell type as `create_wall_timer` with the clock in the
  arena entry, never a `GenericTimer` sibling.
* **B. A third clock source.** `TimerClockSource::System` is not in the
  two-clock design. It is the honest mapping of `NROS_CLOCK_SYSTEM_TIME`, which
  `create_timer(clock, …)` would otherwise have to reject at runtime, and it
  is what rclrs's `TimerClock::SystemTime` names. Accepted; ledger row
  `rust:TimerClock` records it.
* **C. A second spelling for attaching the source.**
  `NodeCtx::install_ros_time_source()` / `install_ros_time_source_on(topic)`
  exist beside `use_sim_time`. rclcpp has no such verb — a remapped `/clock` is
  `-r /clock:=…`. The explicit form is what a `MockSession` test and an image
  without `param-services` need, so it is a warranted second path under
  RFC-0089 §"When a second path is warranted", but it is ours-only and has no
  ledger row because the parity extractor builds the Rust surface without
  `sim-time` (425 W3 said so). → W7.
* **D. The switch is a process-global with a default of TRUE** plus three
  per-executor fields (`sim_time_source`, `sim_time_requested`,
  `sim_time_stated`, `executor/spin.rs:1401`). The default-true is what lets an
  explicit install need no second call to arm it; it is also what made two
  tests race (issue 1104). No hidden thread anywhere — the source is a
  subscription callback, nothing else (checked: `time_source.rs` and the
  `sim-time` paths of `spin.rs` spawn nothing).
* **E. The reserved-parameter hook fires before the store's verdict.**
  `note_reserved_parameter` (`executor/spin.rs:7971`) runs unconditionally
  ahead of `params.server.declare(...)`, so a re-declaration the store REFUSES
  (`declare` → `false`, which phase-428 W6 made a refusal) still flips the
  switch: the store keeps `use_sim_time=true` while the source detaches. Found
  while making the lane compile; `use_sim_time_attaches_and_detaches_the_clock_source`
  now pins the current behaviour with a comment naming this item. → W3.

**Conclusion.** The capability is 425's and it is complete. What remains is
REACH (three surfaces cannot ask for a ROS-time timer), one loudness item, and
two parameter-semantics corrections. That is a real delta, small, and not
worth closing 430 into 425's archive: the items below are it.

## Work items — the delta only

Every item that creates a timer ends at **one dispatch**:
`Executor::register_timer_on_clock(period, TimerClockSource, cb)`
(`executor/spin.rs:5124`). The C++ verb already does (`nros-cpp/src/timer.rs:145`);
the Rust, C and `ComponentNode` verbs below must reach the same function
through the same `TimerClockSource`, flat, with the clock in the arena entry —
**no `TimerBase` derivative, no `GenericTimer`, no second cell type.** A second
spelling of the enum or a per-language clock-to-source mapping is the drift
0135/0160 measured one layer down.

* **W1 [loudness] — `use_sim_time` true and no `/clock` ever: say so once.**
  After the source is installed and a wall interval (a named const, on the
  order of seconds) passes with `!Clock::is_ros_time_override_active()`, emit
  ONE `nros_log` warning naming the parameter, the topic and the fact that
  ROS-time timers are running on system time meanwhile. Not a behaviour
  change: row 15 stands. Reset the one-shot if the source is later detached
  and re-attached.
  *Acceptance:* a `MockSession` unit test declares `use_sim_time` true, spins
  past the interval, and observes exactly one diagnostic (through an executor
  accessor the test can read, so the assertion is not a log grep); a second
  spin past the interval adds none; a sample arriving before the interval
  produces none. The book's "Limits" section names the diagnostic.

* **W2 [params] — declare `use_sim_time = false` on every node, as rclcpp does.**
  When `sim-time` and `param-services` are both on, `ensure_parameter_store`
  (`executor/spin.rs:7875`) declares `use_sim_time` `Bool(false)` if the app has
  not, so `ros2 param list` shows it and `ros2 param set <node> use_sim_time
  true` reaches `refresh_use_sim_time_from_store` on a node with no sim-time
  code at all — the acceptance 430 W2 wrote and row 5 found half-met. An app's
  own declaration (launch bake, YAML, code) wins because it happens first or
  replaces the default through the store, never through a second hook.
  *Acceptance:* `parameters_roundtrip` (or a unit test on `MockSession`) lists
  `use_sim_time=false` for a node that never named it; setting it true through
  the store attaches the source on the next spin; an image without
  `param-services` is unchanged.

* **W3 [correctness] — the reserved-parameter hook follows the store's verdict.**
  Move `note_reserved_parameter` after `params.server.declare(...)` and call it
  only on `true`; a refused declaration leaves the switch alone (a runtime
  change still arrives through `refresh_use_sim_time_from_store`). Finding E.
  *Acceptance:* re-declaring `use_sim_time` false on a node that declared it
  true is refused AND the source stays active; the comment this PR left in
  `use_sim_time_attaches_and_detaches_the_clock_source` is replaced by the
  test flipping the switch through the store.

* **W4 [api, Rust] — the clock axis on the node-level surfaces.**
  `NodeCtx::create_timer_on_clock(period, TimerClockSource, cb)` (and the
  `_in` group form if `create_timer_in` keeps one), and on the declarative
  `nros::Node` a `create_timer_on_clock` whose `EntityMetadata` carries the
  source so `node_runtime.rs:1459` registers through `register_timer_on_clock`
  instead of `register_timer`. `TimerClockSource` stays the one enum; no
  `TimerOptions` builder (ledger `rust:IntoTimerOptions` is declined and stays
  so).
  *Acceptance:* a `nros::main!` component declares a ROS-time timer and
  `sim-clock-listener` can be rewritten against `NodeCtx` with no executor
  access; `check-feature-contract` green (the capability reaches the umbrella);
  `just check api-parity` rows `rust:NodeCtx::create_timer_on_clock` /
  `rust:Node::create_timer_on_clock` filed as extension, `rust:TimerClock`
  re-read.

* **W5 [api, C] — `nros_timer_init` takes rcl's clock.** rcl's shape is
  `rcl_timer_init(timer, clock, context, period, callback, allocator)`; ours
  gains the `nros_clock_t*` in rcl's position and `rclc_executor_add_timer`
  (`nros-c/src/executor.rs:1879`) maps the clock's type to `TimerClockSource`
  and calls `register_timer_on_clock` — the same mapping
  `nros_cpp_timer_create_on_clock` performs, so factor it into ONE
  `TimerClockSource::from_clock_type(u8)` both FFI crates call. `nros_timer_t`
  grows one field: re-run the opaque-size mirror (`opaque_sizes.rs`,
  `check-ffi-struct-mirrors`).
  *Acceptance:* a C timer on a `NROS_CLOCK_ROS_TIME` clock stops when `/clock`
  stops and a `NROS_CLOCK_STEADY_TIME` one does not (a C unit test over the
  mock, mirroring `ros_time_timer_follows_the_simulated_clock`); `check-c`
  green; ledger row `c:timer_init` written (there is none today).

* **W6 [api, C++] — `ComponentNode` and the hosted `rclcpp::` spelling.**
  `ComponentNode::create_timer(clock, period_ms, cb)` beside its
  `create_wall_timer` (both overload shapes, plus `NROS_CREATE_TIMER` if the
  macro family is kept), and under `NROS_CPP_STD` the free
  `rclcpp::create_timer(node, clock, period, cb[, group])` — humble's only form
  (row 13) — returning the same `std::shared_ptr<TimerBase>` cell
  `rclcpp::Node::create_wall_timer` returns (`nros.hpp:812`), with the clock in
  the arena entry. Do NOT add a clock-less `Node::create_timer(period, cb)`
  until the captured surface is Iron or later (row 14).
  *Acceptance:* a ported file `rclcpp::create_timer(node, node->get_clock(),
  100ms, cb)` compiles unchanged under `NROS_CPP_STD` and follows the bag
  (extend the sim-time fixture pair with a C++ listener, or a hosted unit
  test over the mock); `create_wall_timer` in the same file is unaffected by
  `/clock`; `check-cpp` and `check-api-parity` green; rows `cpp:create_timer`
  and `cpp:ComponentNode::create_timer` re-verdicted.

* **W7 [ledger, RFC, docs] — record what is true.** RFC-0089's amendment
  ("phase-430 brings ROS time") becomes "phase-425 brought it; phase-430 is the
  reach delta", and the RFC settles finding A in one sentence: either the
  hosted `TimerBase` is the sanctioned exception (a name for `SharedPtr` to
  hang on, vtable = one destructor, freestanding pays nothing) or it goes.
  Ledger: rows for `install_ros_time_source*` once the extractor builds the
  Rust surface with `sim-time` (finding C), plus the rows W4–W6 name. Book
  page: the W1 diagnostic and the W2 default.
  *Acceptance:* `just check doc-refs`, `just check api-parity` green; no
  document in the tree still says ROS time is unstarted or that the delta is
  unmeasured.

* **W8 [cost] — closed by row 18.** The per-timer `clock_source` +
  `last_clock_ns` stay unconditional; a cfg fork of `TimerEntry` for ~16 bytes
  and one predictable branch would cost more in mirror drift than it saves.

## Sequencing

W3 first (smallest, a correctness fix with a pinned test waiting for it), then
W2 (it makes W1 observable on a node with no sim-time code), then W1. W4, W5
and W6 are independent of each other and of the first three; each is one
surface and each ends at the one dispatch. W7 last, because it records what
landed.

## Amends

RFC-0089 §"Timer, studied against RTOS semantics", amendment of 2026-09-05:
the sentence "phase-430 brings ROS time" is superseded by this measurement —
phase-425 brought it; the conclusion ("still flat, a runtime field and a
second verb, no hierarchy") held in the code 425 landed and is what W4–W6 must
preserve. Finding A is the one place the tree and the RFC disagree, and W7
owns the ruling.

## Adjacent, and NOT this phase's: the seventh porting difference closes (2026-09-09)

Recorded here because this phase's W4–W6 are the surfaces a ported file's timer
verbs land on, and a reader arriving at "which spin verb does a ported `main`
write?" from that direction should not have to find the answer by accident.

PR #753 (phase-427 W10) measured the rclrs talker port at SIX edits and recorded
a **seventh** difference deliberately outside the count: the port writes
`spin_blocking`, because `Executor::spin` is taken here by `spin(Duration) -> !`
— the body of an RTOS task, RFC-0002's one-executor-per-task shape — so
upstream's `spin(SpinOptions)` has no free name. It cost no extra edit (it falls
on the same line as one of the two `?`s), which is exactly why it was recorded
rather than absorbed.

**That item is now decided and owned, and neither is here.** RFC-0089 §"The
`spin` family: upstream's names get upstream's contracts" settles the shape —
`spin(SpinOptions) -> Result<(), NodeError>`, `spin_once(Duration)` as the tick
primitive, `spin_some(Duration)` as upstream's drain verb, and `spin_forever`
for the diverging `-> !` form that a bare-metal entry needs and upstream has no
name for. **phase-427 W11** landed it on `main` (2026-09-09) — this branch had
queued it as W12 and `main` numbered it W11 first, so W11 is the record — and
with it the port writes `spin` and the seventh difference is CLOSED. The port's
EDIT COUNT does not drop with it: the ported line still differs, because ours is
`?` where upstream is `.first_error()?`, which is the no-allocator divergence
rather than a naming one.

Nothing in this phase moves. The clock axis reaches the node-level surfaces
through `register_timer_on_clock` (W4–W6) whatever the spin verbs are called,
and no acceptance here names a spin verb. This paragraph exists so that "it
closes in phase-427 W11" has an answer in both documents rather than only in the
one that raised it.

## Issues homed here (survey 2026-09-11) — re-homed 2026-09-12

Every open issue was checked for a home phase; these had none, or were mentioned
here only in passing. A mention is not an owner — an issue with no work item is
an issue nobody is accountable for. Each row is a work item: the issue holds the
evidence, the item is *close it*.

**This phase was ARCHIVED on 2026-09-12 (`f3a0b7168`), a day after the survey
ran, so an open row here would be homed in a document that is finished — which
is the same "nobody is accountable" state the survey set out to remove.** The
table is kept as the record of what the survey found; the two rows that are
still open moved to a live phase on the same day:
[#1041](../phase-436-poll-wake-revision-deadline-driven-executor.md) and
[#1203](../phase-426-parameters-rust-ssot.md). #1049 needed no move — it is
resolved and archived.

| issue | why it belongs here |
| --- | --- |
| [#1041](../../issues/1041-timer-missed-deadline-policy-differs-from-rcl.md) | a repeating timer CATCHES UP after a stall where rcl SKIPS. Partly fixed already — the executor arena defaults to `Skip`; `nros-node/src/timer.rs:354` is the unswept sibling, which is the fix-the-class shape, not a new defect |
| [#1049](../../issues/archived/1049-timer-time-until-next-call-always-zero.md) | `nros_timer_get_time_until_next_call` returns 0 for every registered timer — a broken accessor on exactly the C surface W5 reshapes |
| [#1203](../../issues/1203-parameter-builder-bypasses-reserved-parameter-hook.md) | `Executor::parameter`'s `ParameterBuilder` declares straight into the store, bypassing the reserved-parameter hook W3 fixed on the other two paths. Sequence it with #1049: both are a value channel that silently lies |
