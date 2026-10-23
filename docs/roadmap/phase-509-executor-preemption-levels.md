# Phase 509 -- Executor preemption levels (RFC-0108, amended RFC-0107 steps 4-6)

**Status (2026-10-21). In progress: P1-P8 and P11 done.** The design is
[RFC-0108](../design/0108-executor-preemption-levels.md) (Draft, its open
questions decided 2026-10-21). This phase also carries RFC-0107's steps 4-6
as RFC-0108 amends them (RFC-0107 is on branch
`docs/rfc-0107-shared-executor-core`; steps 1-3 are merged).

## Why

Today a tier is a whole executor on its own thread, so the tier is both the
unit that orders ready work and the unit that buys preemption. Inside one
executor, ready callbacks run in registration order, not in the contract's
rank order. Each extra tier costs a whole executor: +7,096 B of DTCM on the
safety island, plus a 16 KiB stack (RFC-0108 section 4). RFC-0108 separates
the two concerns:

- **Ordering:** one ready set per core, keyed by rank.
- **Preemption:** N workers over that ready set, one per contiguous band of
  ranks. N is derived from the contract, capped by a level count, and every
  member that loses its rank is reported.

## Conventions

- Each item below is one small mergeable nano-ros PR with its own tests,
  unless it says otherwise.
- File:line citations are at `origin/main` 6c63994586, the commit RFC-0108
  was read against. Re-check them when an item starts.
- `nros-node` paths are relative to `packages/core/nros-node/src/`. Derivation
  paths are relative to `packages/core/nros-orchestration-ir/src/`.
- "Golden" means the entry-codegen golden files, re-blessed in the same PR.
- **Touches** lists files that already exist. A file the item creates is
  marked (new).

## Work items

### P1 -- Report every member that loses its rank

- [x] Goal: rule 2 of tier derivation emits one `tier` degradation per member
      whose rank differs from the tier's, not only the first
      (`derive.rs:631-649`, `members.iter().find(...)`).
      Done: one degradation per member that loses its rank, each member once
      however many of its groups the tier holds
      (`every_member_that_loses_its_rank_is_reported`).

**Touches:** `derive.rs` (rule 2, around `:611-680`).

**Acceptance:** unit test: an authored tier with members at ranks 5, 7 and 8
yields two degradations, naming both the 7 and the 8 member. The existing
derive tests stay green. On the island's priority plan this adds a line for
`mrm_comfortable_stop_operator`.

**Depends on:** nothing.

### P2 -- Rank-ordered ready set on one thread

- [x] Goal: inside one executor, ready callbacks dispatch in rank order
      (0 = most urgent), with equal ranks in registration order.

**Done (as built).** Two points differ from the plan below; RFC-0108 D2 is
amended to match.

- **The rank is the node's, seeded by name, not a field next to the tier's
  sched context.** The derivation ranks NODES (`RealizedNode`), and the
  island's single tier has one sched context for three ranks, so a rank
  beside the context could not tell them apart. Codegen normalises the
  derivation's priorities (`EntryDerivation::node_ranks`, 0 = most urgent,
  either kernel direction) and the C and C++ packs call
  `nros_cpp_bind_node_rank(exec, name, ns, rank)` before each ranked node is
  created (`declare_calls.jinja`). Every group of a node shares its rank. The
  rank rides in the node-name table entry's and the `NodeRecord`'s padding,
  and per callback slot in the `SlotTag` (4 -> 6 B per slot).
- **The level is `Priority::dispatch_level(rank)`:** `Critical` 0, `Normal`
  1 + rank (clamped to 29), `BestEffort` 31, so the three C/C++ buckets are
  three reserved ranks and their order is unchanged. The ready set's FIFO half
  is `RankedFifoSet` (one bitmap per level plus a 32-bit presence word); the
  EDF half keeps its three buckets.
- Not seeded yet: the Rust `nros::main!` road (it lowers no ranks; its nodes
  hold rank 0, today's order). It joins with P5, which moves rule 4 onto
  levels.

Tests: `ranked_callbacks_dispatch_in_rank_order_not_registration_order`,
`equal_ranks_keep_registration_order`,
`one_rank_dispatches_in_registration_order`,
`priority_buckets_are_reserved_ranks_around_the_contracts`,
`dispatch_levels_reserve_the_ends_for_the_buckets`, the `ranked_*` ready-set
tests, `node_ranks_normalise_to_zero_most_urgent`, and
`a_one_tier_entry_seeds_each_nodes_rank` (the island's shape: the 30 Hz pair
seeded 0 and the 10 Hz pair 1, each before the node exists). Goldens: two new
rows, `c_native_ranked` and `cpp_native_ranked`; no existing golden moves,
since none of them derives a rank. `executor_stays_small_enough_to_construct_on_a_stack`
holds (the rank adds no inline table).

**Touches:**

- Codegen: emit a normalised rank per callback group next to the tier's
  sched context (`packages/cli/nros-cli-core/src/codegen/entry/lower.rs:560-575`,
  `lower_sched`). Today only `os_pri` is set.
- `nros-node`: `executor/sched_context.rs:85-101` (the three-value
  `Priority`, `COUNT = 3`), `:549` (default `Normal`), `:584-618`
  (`from_tier_policy`).
- `nros-node`: replace the three-bucket FIFO with a multi-level FIFO (one FIFO
  per rank plus a 32-bit presence bitmap) in `executor/ready_set/mod.rs`. The
  dispatch path in `executor/spin.rs` changes at the readiness scan
  (`:11112-11121`), the bucketing (`:11387-11455`), the sort key
  (`:11686-11695`) and the drain (`:11892-11898`).
- The C and C++ `Priority` API (`packages/api/nros-c/src/executor.rs:4582-4584`,
  `packages/api/nros-cpp/src/lib.rs:4467-4469`) maps Critical / Normal /
  BestEffort to three reserved ranks.

**Acceptance:**

- Unit: three callbacks registered in reverse rank order, all ready, dispatch
  in rank order.
- Unit: equal ranks keep registration order.
- Unit: an image with one rank dispatches in exactly today's order.
- The C and C++ API tests pass unchanged.
- Golden: entry goldens re-blessed. The only diff is the added rank field.
- The `nros-node` executor size budget does not grow past its ratchet.

**Depends on:** nothing. It can land in parallel with P1.

### P3 -- Re-pick after every callback, and measure the re-probe cost

- [x] Goal: after every callback the dispatcher picks again from the ready set
      as it is now. Pushed readiness (RFC-0084 attributed slots) and due timers
      join the ready set mid-pass.
- [x] Measure the cost of re-probing unattributed slots (XRCE, uORB) after
      every callback, against probing them once per pass. Record the
      number in RFC-0108 and settle its deferred re-probe question.

**Done (as built).** The drain is one pick at a time: the most urgent queued
job (bucket by bucket, the EDF heap then the ranked FIFO), then
`merge_mid_pass` folds in what became ready while the callback ran. Rules:

- Pushed readiness (an attributed slot's arrival bit) joins mid-pass; an
  arrival for a slot the pass already settled is kept for the next scan.
- Each entry runs at most once per pass. A callback that readies itself
  waits for the next pass.
- A slot whose context gates dispatch (a sporadic budget, a time-triggered
  window, an OS-priority worker) waits for the next pass, whose scan applies
  the gate. So do the LET semantics (they dispatch the pre-sampled snapshot)
  and a trigger narrower than "any" (it chose the pass's work).
- **Timers:** not merged mid-pass, and that is a correction to this item's
  goal. A timer is always ready and is in every pass already; its clock
  advances by the pass's `delta_us`, so it becomes due only at a pass
  boundary. RFC-0108 D2 says so.

**The re-probe measurement** (`reprobe_cost_on_an_unattributed_executor`,
`cargo test -p nros-node --lib --release --features
alloc,log,sim-time,param-services` with `NROS_EXECUTOR_MAX_CBS=32`, x86-64
host, three runs): 32 unattributed subscriptions, 4 ready per pass, empty
callbacks, 20,000 passes.

| probe | ns per pass |
| --- | ---: |
| once per pass (the scan) | 380-385 |
| after every callback | 565-577 |
| difference | +185-191 (+46-48 per callback, 1.65-1.71 per extra probe) |

**Decided: once per pass by default** (`REPROBE_EACH_CALLBACK_DEFAULT =
false`). The per-callback probe costs one `has_data` per unsettled slot per
callback, so it grows with slots x callbacks; the mock's probe crosses no
FFI, while a real XRCE or uORB probe crosses the rmw C ABI on an MCU several
times slower than this host. An image that wants it opts in with
`Executor::set_reprobe_each_callback(true)`. Zenoh attributes every arrival,
so the island's subscriptions join mid-pass either way, for one atomic swap
per callback.

Tests: `a_callback_made_ready_mid_pass_runs_in_rank_order` (A readies the
more urgent B; B runs before the pending C), `without_the_reprobe_an_unattributed_slot_waits_for_the_next_pass`,
`an_entry_runs_once_per_pass_even_when_it_readies_itself`,
`the_let_semantics_take_nothing_mid_pass`.

**Touches:** `executor/spin.rs` (the pass structure around `:11112-11121` and
`:11892-11898`); `executor/ready_set/mod.rs`.

**Acceptance:**

- Unit: callback A makes a more urgent B ready; B runs before a less urgent
  C that was already pending in the same pass.
- Host benchmark: `spin_once` with re-probe after every callback and with
  re-probe once per pass, on an XRCE-shaped mock with unattributed slots. The
  PR states both numbers and the choice.
- Existing executor and E2E lanes stay green.

**Depends on:** P2.

### P4 -- The task record and the rlm schema change

- [x] rlm (`ros-launch-manifest`): add `min_interarrival`, and `budget` +
      `budget_period`, on inputs. Done: rlm v0.1.68 (61a134f), on `sub.<e>`
      and `srv.<e>`; the model carries `min_interarrival_ms`, `budget_ms`,
      `budget_period_ms` on `SubContract` and `SrvContract`. play_launch
      64b55e32 pins it and carries the keys into the model.
- [x] nano-ros: `nros-orchestration-ir` carries a task record per callback:
      release kind (periodic / sporadic / served / unanalysable), T or T_min,
      D, C, and the server's Q/P. The `unanalysable` degradation; refusal on a
      safety image.
- [x] Pin bump to the rlm tag (v0.1.68, `just bump-manifest`; play_launch
      d109f2ec -> 64b55e32).

**Done (as built).** `task_record.rs` (new): `task_records(model, wcet)`
gives one `TaskRecord` per callback the model names (timer paths,
subscriptions, service servers and clients, action endpoints), with its
release kind, the source of its release, T or T_min, D (stated or implicit),
C and Q/P. `check_model` refuses the unanalysable ones on a safety image; it
runs on the three bake roads beside the FTTI check. `derive_tiers_with_named`
carries the records (`DerivedSchedule::tasks`) and, on an image that
supervises nothing, one `unanalysable` degradation per unanalysable callback,
which the priority record lists. Points the plan left open, now in RFC-0108
D1 ("As built in P4"):

- A rate bound is the subscriber's `max_rate_hz` or the topic's `rate_hz`; a
  `min_rate_hz` is a floor and releases nothing.
- A service client's response inherits the release of the paths that issue
  its request (`output: [<client>]`); an action endpoint is unanalysable
  (the grammar has no key for it yet).
- D is implicit (T, T_min or P) when no path or `max_response` states one.
- No golden moves: the task record is not in the priority-plan JSON, and no
  golden fixture has an unbounded input on a derived schedule.

Tests: `each_release_kind_is_derived_from_the_contract`,
`exec_time_comes_from_the_wcet_profile_and_is_never_invented`,
`an_unbounded_input_is_reported_by_name_without_supervise`,
`an_unbounded_input_is_refused_by_name_on_a_safety_image`,
`a_client_inherits_the_release_of_the_paths_that_issue_its_requests`; rlm
`an_inputs_release_bound_is_a_minimum_interarrival_or_a_budget` (accepts the
keys, refuses a budget without a period) and the model round trip;
play_launch `an_inputs_release_bound_reaches_the_model`.

**Cross-repo step.** The rlm change goes to rlm `main` by fast-forward push
and needs a release tag before nano-ros can pin it. The rlm push and the
release are a **separate approval** from this phase's nano-ros PRs. Until the
tag exists, the nano-ros PR may pin the rlm rev, as phase-503 W5 did.

**Touches:** `derive.rs` (the task record next to the derivation;
`supervised_nodes` at `:72-80` defines a safety image); `wcet.rs` (C from the
contract's execution time); the rlm schema and its model types.

**Acceptance:**

- Unit: each release kind is derived from a fixture contract.
- Unit: an event input with no rate, minimum inter-arrival or budget is
  reported by name on an image without `supervise:`.
- Unit: the same input is refused by name, naming the two keys that would
  fix it, on an image whose contract supervises a deadline.
- rlm: its schema tests accept the new keys and reject a budget without a
  period.

**Depends on:** nothing in nano-ros. The rlm release gates the pin bump.

### P5 -- Level derivation and the level budget

- [x] Goal: tier-derivation rule 4 becomes RFC-0108 D4:
  - one level per distinct rank;
  - the merge with execution times (C);
  - the capped merge without C, which skips protected bands;
  - one `level-budget` degradation per member that loses its rank;
  - equal kernel priorities merged on time-slicing platforms;
  - the `max_levels` knob (`CONFIG_NROS_EXECUTOR_MAX_LEVELS`, or the
    platform's equivalent) with per-platform defaults;
  - a `levels` field in the priority record.
- [x] Decide where the response-time analysis lives (`nros-orchestration-ir`
      or rlm), which is deferred to this item by RFC-0108.
      Decided: `nros-orchestration-ir` (`levels.rs`).

**Done (as built).** `levels.rs` (new) derives one core's levels;
`derive_tiers_with_named` calls it in place of rule 4's tier per node.

- **A derived level runs as one derived tier** until the level runners land
  (P9): one thread at the band's most urgent priority, its members in rank
  order inside it (P2). A level of one node keeps the name and the fields
  rule 4 always gave it (`derived-<node>`); a level of several is
  `derived-level-<k>`, with the tightest period and deadline of its members.
  A per-node sporadic budget cannot be enforced on a shared thread, so a
  merged level drops it with a `budget` degradation (the bake roads derive
  no budget today: it needs a WCET profile).
- **Pins.** An authored tier (rule 1 or rule 2) is one band at its own
  priority. The derivation bands only the unbound nodes, and a free band
  never spans a pin's priority. Pins count toward the cap; when the pins
  alone leave no room, one `level-budget` line says the cap cannot be met.
- **The analysed merge (D4 rule 2)** is a dynamic program over the band
  boundaries: a callback's response time depends only on its own band
  (blocking from below it in the band, preemption from the bands above), so
  each band's feasibility is computed once, O(r^2) analyses. Fewest bands,
  ties to the largest least slack `(D - R) / D`. The response time is the
  preemption-threshold analysis over every job of the level-i busy period
  (`levels::response_time`); a served callback is analysed as (Q, P). It
  engages when no tier is authored and every derived callback is analysable
  with C; otherwise the capped merge applies. If the fewest feasible bands
  exceed the cap, the capped merge runs on top (`response-time-capped`). If
  no banding keeps every deadline, the levels stay one per rank and each
  callback that misses is a `feasibility` degradation.
- **C reaches the analysis only through `derive_tiers_with_wcet`.** The
  three bake roads select no RFC-0078 profile yet, so on them the task
  records carry no C and the levels come from rules 1 and 3. Wiring a
  selected profile into the roads is left to the phase that selects one per
  image.
- **The capped merge (D4 rule 3)** merges the least urgent adjacent pair
  whose more urgent band holds no deadline (the realizer's) and no
  supervised route; when every pair touches one, the least urgent pair is
  merged anyway and a `level-budget` line names the node whose deadline or
  route gains the blocking, and the nodes that add it.
- **The cap (D4 rule 4):** `CONFIG_NROS_EXECUTOR_MAX_LEVELS` (Zephyr
  Kconfig, default 8, range 1-32), read from the image's `.config` into
  `PriorityPlan::max_levels` on the roads that resolve the image's plan;
  otherwise `levels::default_max_levels` (D6: Zephyr, POSIX, NuttX 8;
  FreeRTOS 4; ThreadX the pool width). No `system.toml` knob: that is an rlm
  schema change, not needed for the island's Kconfig road.
- **Rule 8:** `levels::time_slices_equal_priorities` is true on FreeRTOS
  only. There a derived band at an authored tier's kernel priority joins
  that tier's level and its groups are bound to the authored tier; two
  authored tiers at one priority are one level in the record (the runtime
  keeps both threads until P9). Derived levels never collide: a level is a
  distinct allocated priority.
- **Unanalysable callbacks** are recorded at the least urgent rank of their
  level (`Level::best_effort`). The runtime ranks per node (P2), so the
  per-callback demotion is a record only until the scheduler/worker split
  (P6) can dispatch a callback at a rank other than its node's.
- **The record:** `nros_priority_plan.json` gains `levels` (the cap, its
  source, the method, and per level its priority, threshold rank, members
  with their ranks, authored tiers and best-effort callbacks), and the
  summary prints one line per level. The `level-budget` lines are in
  `degradations`.
- **The Rust road (`nros::main!`) now seeds ranks.** Each launch-named node
  is seeded with `nros::__macro::seed_node_rank` before its `register`, from
  the same normalisation the C/C++ packs use (`levels::node_ranks`); the
  CLI's Rust renderer emits the same statement (parity case `ranked`).
- **Not touched:** `executor_sizing.rs` (its sched-context count is the
  larger of the authored tiers and the node count, still an upper bound
  with fewer levels).

Tests: `levels::tests` (`two_equal_rank_nodes_and_one_other_give_two_levels`,
`cap_two_over_three_ranks_merges_the_least_urgent_pair`,
`a_protected_band_is_skipped_by_the_capped_merge`,
`a_forced_merge_names_the_blocking_it_adds`, `cap_one_reports_every_member`,
`with_c_stated_the_merge_keeps_every_deadline_where_the_naive_merge_misses`,
`the_analysed_merge_finds_the_fewest_levels`, `an_authored_tier_is_a_pin`,
`two_levels_at_one_kernel_priority_merge_on_a_time_slicing_kernel`,
`an_unanalysable_callback_dispatches_at_its_levels_least_urgent_rank`,
`the_platform_defaults_follow_d6`);
`derive::tests::unbound_nodes_are_banded_into_levels_under_the_images_cap`;
`priority_record::tests::the_record_carries_the_levels`. The
`derived-tiers-cpp` fixture goes from four tiers to two levels
(`derived_tiers_bake`, `derived_tiers_entry`). No entry golden moves: no
golden fixture derives a tier. Parity: `parity_ranked.rs.golden`.

**Touches:** `derive.rs:530-608` (rule 4, one tier per node; the name at
`:534`); `derive.rs:611-680` (rule 2); `priority_plan.rs` (the `levels`
field); `executor_sizing.rs`; the Zephyr Kconfig next to
`NROS_ZEPHYR_TIER_STACK_SIZE` (`zephyr/Kconfig:435`); the board capability
tables.

**Acceptance:**

- Unit: two equal-rank nodes plus one other give 2 levels (today rule 4 gives
  3 tiers).
- Unit: cap 2 over 3 ranks merges the least urgent pair.
- Unit: a band holding a deadline or a supervised route is skipped by the
  capped merge.
- Unit: cap 1 reports every member.
- Unit: with C stated, the merge keeps every deadline in a fixture where the
  naive merge misses one.
- Unit: two levels at one kernel priority on a time-slicing platform merge.
- Golden: the `derived-tiers-cpp` fixture goes from four tiers to two levels;
  the priority-plan JSON golden gains `levels`.

**Depends on:** P1 (shared rule-2 code), P2 (the rank in codegen), P4 (C and
the release kinds for the merge).

### P6 -- Scheduler / worker split in nros-node (amended RFC-0107 step 4)

- [x] Goal: RFC-0107's per-tier `Dispatcher` splits into a per-core
      `Scheduler` and a per-level `Worker`. The Scheduler holds the ready
      set, callback slots, and release and budget accounting. The Worker
      holds the thread, stack, current dispatch and park state. `Executor`
      stays the one-level facade.

**Done (as built).** `executor/scheduler.rs` (new):

- **`Scheduler`** (per core): each callback slot's dispatch level, one
  atomic word of ready bits, and the band table: contiguous `Band`s of
  dispatch levels that partition 0-31, most urgent first, one per worker.
  Its run-time half takes `&self` and is lock-free (the workers and the
  transport's arrival path reach it at once); its set-up half
  (`add_worker`, `bind_slot`) takes `&mut self`. `pick(worker)` takes the
  most urgent queued slot of the worker's own band (lowest level, slot
  order within a level); `run(worker, dispatch)` re-picks after every
  callback (D2).
- **D7:** `arrive(slot)` queues an attributed arrival and wakes only its
  band's worker; `arrive_unattributed()` wakes the most urgent parked
  worker, which re-picks.
- **`Worker`**: its band, its park (RFC-0106: the deadline sources, the
  park primitive and its granularity, the last park's bound, source and
  achieved value) and its stack record (the headroom rule's bound and
  state). These fields moved from `Executor` unchanged; `Executor` holds
  one `Worker` whose band is every level (`Band::ALL`), so it is the
  one-level facade and dispatches, parks and checks its stack exactly as
  before. No golden moves.

Deviations, recorded in RFC-0108 ("As built in P6"):

- **Release and budget accounting and the callback slots stay in
  `Executor`** (timers, sporadic refill, time-triggered windows, the
  callback table). A second worker over the same executor needs
  concurrent access that `&mut Executor` does not give; they move behind
  the scheduler when the runners spawn levels over one scheduler (P9),
  which is also where the FFI opens a scheduler and its levels. Until
  then the `Scheduler` is crate-private and driven by its own tests.
- **A worker picks only its own band.** D3 said "at or above its floor".
  A worker's stack is sized for its band's members (D3, decision 7), so
  it must never run a more urgent band's callback; on one core the more
  urgent band's own worker takes that work first anyway.
- **Size:** the backing is unchanged (no carved table moved), and so is
  the `Executor` value (5,024 B on the x86-64 host): the worker's fields
  stay flat, and `wake_source_count` is a `u8` (it was a `usize`), so its
  small fields share one padding word as they did inline.

Tests: `scheduler::tests` (`bands_partition_the_levels_most_urgent_first`,
`one_band_picks_by_level_then_registration`,
`an_arrival_wakes_only_its_bands_worker`, and on a deterministic one-core
kernel mock `a_more_urgent_bands_arrival_preempts_a_running_callback`,
`members_of_one_band_never_preempt_each_other`,
`a_less_urgent_arrival_waits_and_the_band_keeps_its_order`);
`the_park_and_the_stack_record_are_the_one_workers`; every RFC-0106 park
test passes with the park on the worker.

**Touches:** `executor/spin.rs`, `executor/storage.rs`, `executor/backing.rs`,
`executor/sched_context.rs`, `executor/port_park.rs` (the park moves to the
worker; RFC-0106), `executor/wake_fanout.rs` (an attributed arrival wakes
only its band's worker; RFC-0108 D7).

**Acceptance:**

- The one-level backing size is unchanged (size test).
- Host test with N workers on a deterministic platform mock: a band-2 arrival
  preempts a running band-1 callback; band-1 members never preempt each
  other.
- The RFC-0106 park tests pass with the park on the worker.
- Existing executor and E2E lanes stay green.

**Depends on:** P2, P3, and RFC-0107 steps 1-3 (merged).

### P7 -- Level identity and liveness API

- [x] Goal: the island's supervisor can own a route per level and feed the
      watchdog only when every level is alive (RFC-0108 D8). Adds:
  - a generated `NROS_ENTRY_LEVEL_OF_<node>` next to the priority plan header;
  - a runtime level query on an executor handle;
  - the level index passed to spin hooks;
  - the "every level alive" aggregate from the per-sched-context alive slots.

**Done (as built).**

- **Identity.** An executor's worker carries its level (0 the most urgent;
  `Executor::level` / `set_level`). Until P9 a level is a tier: a tier's
  level is its urgency order among the image's tiers, equal priorities in
  table order (`nros_node::executor::tier_level_by`). The Zephyr runners (C
  `zephyr_run_tiers.c`, Rust `entry_tiers.rs`) place every tier's executor
  by that rule before it spins and state the level count. An executor no
  runner placed is level 0, which is every executor of a one-level image.
- **The header.** `nros_priority_plan.h` gains `NROS_ENTRY_LEVEL_COUNT` and
  one `NROS_ENTRY_LEVEL_OF_<node>` per node, by the same rule over the
  same table order (`PriorityRecord::node_levels`); an entry with no
  running tier is one level with every node at 0.
- **Liveness.** `executor/level_liveness.rs` (new): a per-image registry
  of 32-bit atomics. Every spin checks its level in with the low 32 bits
  of its microsecond clock and a count (`level_last_spin_us`,
  `level_spins`). The watchdog feed is `take_every_level_alive`: true only
  when every level `0..level_count()` has checked in since the last true,
  which it then clears; `every_level_alive` reads without clearing.
- **FFI.** C++ ABI: `nros_cpp_executor_level`, `nros_cpp_executor_set_level`,
  `nros_cpp_tier_level` (reads a runner's own tier array by stride),
  `nros_cpp_set_level_count`, `nros_cpp_level_count`,
  `nros_cpp_level_last_spin_us`, `nros_cpp_levels_alive`,
  `nros_cpp_levels_take_alive`. C API: `rclc_executor_get_level`,
  `rclc_executor_set_level`, `rclc_levels_max`, `rclc_levels_set_count`,
  `rclc_levels_count`, `rclc_level_last_spin_us`, `rclc_levels_alive`,
  `rclc_levels_take_alive`. Rust: the same through `nros::`.

Deviations, recorded in RFC-0108 ("As built in P7"):

- **No spin-hook ABI.** nano-ros has no spin-hook registration; a
  supervisor hooks the spin by wrapping `nros_cpp_spin_once` (the island
  does), and reads the level of the handle it was given with
  `nros_cpp_executor_level`. That is the "level index passed to spin
  hooks".
- **The aggregate is not built from the per-sched-context alive slots.**
  Those count dispatches per sched context inside one executor, and until
  P9 the levels are separate executors on separate threads, so the
  check-in is a per-image registry the spins write.
- **The other runners** (POSIX, NuttX, FreeRTOS, ThreadX) do not place
  levels yet: there every tier's executor reports level 0 until their P10
  runner. The island's Zephyr runners do.
- **C++ parity:** the C++ surface is the `nros_cpp_*` C ABI, which the
  parity report does not hold against rclcpp; the C and Rust ledgers carry
  the new rows.

Tests: `level_liveness::tests` (`every_level_must_check_in_before_the_feed`,
`a_tiers_level_is_its_urgency_order`),
`an_executor_reports_its_level_and_checks_it_in_on_every_spin`; nros-cpp
`level_tests` (`a_tiers_level_is_read_from_the_runners_array`,
`the_alive_aggregate_waits_for_every_level`); priority record
`the_header_names_each_nodes_level` and the island tier's
`NROS_ENTRY_LEVEL_OF_MRM_HANDLER (0)`.

**Touches:** the entry codegen (`lower.rs`, the priority plan header);
`executor/spin.rs` (the spin hook signature); the C and C++ FFI
(`packages/api/nros-c/src/executor.rs`, `packages/api/nros-cpp/src/lib.rs`)
and their headers.

**Acceptance:**

- Golden: the header carries one `NROS_ENTRY_LEVEL_OF_<node>` per node.
- FFI unit: the level query matches the derivation.
- FFI unit: the alive aggregate is false while one level has not checked in.
- C and C++ API parity ledgers updated.

**Depends on:** P5 (level assignment), P6 (workers to query).

### P8 -- Refuse blocking calls to equal or less urgent levels

- [x] Goal: the build refuses a synchronous service call whose server is at a
      level equal to or less urgent than the client's, when the contract
      names both. Asynchronous calls pass.

**Done (as built).** The contract had no key that says a client waits, so
P8 needed one: rlm v0.1.69 (1fcf438) adds `call: blocking | async` on a
`cli.<e>` endpoint and on an `actions.<a>` entry (for every client it
lists); play_launch 0b44a695 carries it into the model
(`Contracts::cli_endpoints`, `Contracts::action_cli_endpoints`).
`blocking_calls.rs` (new) checks each service and action client against
each of its servers:

- `call: blocking` to the client's own level is refused as a deadlock, to
  a level that cannot preempt it (less urgent, or another tier at the same
  kernel priority) as an inversion, each by name.
- `call: async` is never checked.
- **No `call:` is not `async`:** such a client to an equal or less urgent
  level is reported, never refused, so a contract written before the key
  (the island's async `mrm_handler`) builds unchanged and is told what to
  state.
- **A level is a tier until P9:** the resolved tier table's tiers, urgency
  in the kernel's own direction (`SchedCaps::low_number_is_high`). A
  tierless image, or a table of one tier, is one level. A node of a
  multi-tier image that no tier holds has an unknown level: its calls are
  reported. A server outside the image is a call across images and is not
  checked. Tier members are bare node names, so a contract endpoint is
  matched by the last segment of its node FQN.
- **Where:** the three bake roads, after each resolves its table:
  `codegen entry` (`resolve_plan_sched_recorded`, also on its tierless
  early return), `codegen-system` (after `resolve_system_tiers`), and
  `nros::main!` (after `resolve_tiers`, or with no table). Reports go to
  the build's stderr beside the spin-period notes.

Tests: `blocking_calls::tests` (`a_blocking_call_to_an_equal_or_less_urgent_level_is_refused`,
`an_async_call_passes`, `an_unstated_call_is_reported_not_refused`,
`a_tierless_image_is_one_level`, `urgency_follows_the_kernels_direction`,
`an_action_client_is_checked_and_an_unplaced_node_reported`); the entry
road on the `derived-tiers-cpp` fixture
(`a_blocking_call_to_the_clients_own_level_is_refused`,
`an_async_or_unstated_call_builds`); rlm `a_clients_call_is_blocking_or_async`
and the model round trip; play_launch `a_clients_call_mode_reaches_the_model`.
No golden moves.

**Touches:** `derive.rs` (or the check module that reads the contract's
client/server pairs); the degradation and refusal text.

**Acceptance:**

- Unit: a sync client to a less urgent server is refused by name.
- Unit: a sync client to a server at its own level is refused (deadlock).
- Unit: an async client to the same server passes.
- The island's contract passes (its calls are asynchronous).

**Depends on:** P5.

### P9 -- Zephyr runner on levels, per-level stacks, cost report (amended RFC-0107 step 5)

- [x] Goal: the Zephyr runner spawns levels instead of tiers. The FFI becomes
      `nros_cpp_scheduler_open(core)` + `nros_cpp_level_open(scheduler, level)`,
      replacing `nros_cpp_dispatcher_open(core, tier)`. Generated sizes become
      `NROS_CPP_CORE_BACKING_SIZE`, `NROS_CPP_SCHEDULER_BACKING_SIZE_<core>`
      and a per-level stack.
- [x] Per-level stacks derived from the members (largest member plus the
      worker frame), never the platform default.
- [x] The cost report after the target compile: per region and per level its
      stack and worker record; per core its scheduler and backing; text and
      JSON. A check, never an input.

**Done (as built).**

- **The runner.** Every generated Zephyr entry calls
  `nros_board_zephyr_run_levels_in` (C++ `ZephyrBoard::run_levels`): each
  row of its table is one level (a derived level, or an authored tier,
  which is one band), placed by the P7 rule. The boot level owns the
  session and runs on `main()`; every other level's worker thread opens its
  executor over the core's scheduler. The `_tasks_in` / `_backed_in` /
  `_in` / `_ns` runners stay for older entries and take the scheduler from
  the platform heap (a few hundred bytes); a generated image takes none.
- **The FFI** (`nros_cpp_*`): `nros_cpp_scheduler_backing_size(n)`,
  `nros_cpp_scheduler_open(core, n_levels, owner, backing, bytes)` (over
  the session the owner executor opened; it states the level count),
  `nros_cpp_level_adopt(scheduler, level, executor, stack_bytes, name)`
  (the boot level), `nros_cpp_level_open(scheduler, level, name, storage,
  backing, backing_bytes, stack_bytes)` (every other level), and the
  queries `nros_cpp_scheduler_level_count`,
  `nros_cpp_scheduler_level_executor` and
  `nros_cpp_scheduler_level_stack_bytes`. The scheduler is a record in the
  entry's own backing: a head (core, level count, domain, session) and one
  seat per level (executor, the stack its worker runs on, name).
- **Generated sizes.** The entry emits `__nros_scheduler_0[
  NROS_CPP_SCHEDULER_BACKING_SIZE_0 / 8]` with
  `NROS_CPP_SCHEDULER_BACKING_SIZE_0` = `NROS_CPP_SCHEDULER_BACKING_SIZE(n)`
  (`<rclc/detail/main.h>`: 8 + one pointer + three pointers per level,
  8-rounded; 88 B for three levels on a 64-bit target). The per-level
  backings stay `NROS_CPP_TIER_BACKING_SIZE_<i>` (issue 1970).
- **Derived stacks.** A spawned level that declares no `stack_bytes` is
  `NROS_TIER_TASK_MEMORY_DEFINE(i, NROS_LEVEL_STACK_BYTES(0u))`
  (`tier_task_memory_zephyr.h`): the largest member's stack, or the
  per-callback default for a member that states none, plus the worker
  frame. New Kconfig `NROS_ZEPHYR_LEVEL_WORKER_FRAME` (default 4,096, an
  estimate); the per-callback default is `NROS_ZEPHYR_TIER_STACK_SIZE`
  minus the frame, so a level whose members state nothing gets exactly the
  knob, as every tier did before (a `BUILD_ASSERT` keeps the frame below
  it). An authored `stack_bytes` stays the expert override, and the
  priority record carries it (`tiers[].stack_bytes`). FreeRTOS keeps its
  stated default until its P10 runner (`TierTaskMemory::derives_level_stacks`).
- **The cost report.** `scripts/nros-mem-report.py <elf> --levels`
  (`just mem-report`): per level its stack (derived and measured), its
  worker record (executor context + backing + thread object) and its
  members, naming each member that took the per-callback default; per core
  its scheduler and the level backings; per region (the link map's memory
  regions, else the output section). `--json` for machines; `--check`
  asserts the contexts are `n x NROS_CPP_EXECUTOR_STORAGE_SIZE`, each
  backing its `NROS_CPP_TIER_BACKING_SIZE_<i>`, the scheduler
  `NROS_CPP_SCHEDULER_BACKING_SIZE(n)`, and each spawned level's stack
  holds the derived bytes. It reads the image, the per-build sizes header,
  the image's `.config` and `nros_priority_plan.json`; nothing reads it
  back. On the fixture: three levels, stacks 16,384 B (= derived) for the
  two spawned levels, each worker record 14,608 B (context 3,144 + backing
  11,288 + thread 176), scheduler 88 B; `.bss` 43,736 B, `.noinit` 32,768 B.
- **Fixed on the way:** P7's level helper in `zephyr_run_tiers.c` used
  `nros_tier_spec_t` before its typedef, so no tiered Zephyr C/C++ image
  compiled on `main` since P7 (no gate lane compiles one); and mem-report's
  sizes-header search still looked under the pre-phase-501 `nros/` mirror.

Deviations, recorded in RFC-0108 ("As built in P9"):

- **A level is still an executor over the shared session**, not a worker
  over one ready set: dispatching one `Executor` from two threads needs the
  spin pass to drop its `&mut` borrow around each user callback, and
  RFC-0107's step 4 dispatcher is not on `main`. The schedule is D3's
  (rank order and non-preemptive inside a level, kernel preemption across
  levels); release and budget accounting stay in each level's executor,
  which owns its members' slots, so nothing is shared and nothing moved.
  There is therefore no `NROS_CPP_CORE_BACKING_SIZE`.
- **The FFI takes what it needs:** `scheduler_open` takes the owner
  executor and the entry's backing besides the core, and the boot level is
  adopted rather than opened.
- **No member states a stack yet** (no contract or `system.toml` key), so
  every member takes the per-callback default and the report names it.
- **The board fixture is Zephyr native_sim**, the lane's Zephyr target with
  a router (the real Zephyr scheduler), not QEMU.
- **The Rust Zephyr runner** (`entry_tiers.rs`) keeps P7's placement, and
  `zephyr_image.rs` needed nothing.

Tests: `scheduler_tests` in nros-cpp
(`the_backing_is_a_head_and_one_seat_per_level`,
`a_short_backing_or_a_foreign_handle_is_refused`,
`each_level_is_seated_once_with_its_stack`); nros-entry-lower
`an_undeclared_spawned_level_derives_its_stack`; the entry goldens
(`cpp_zephyr_tiers`, `c_zephyr_tiers`) and
`typed_emit_tiers_zephyr_embedded_uses_run_tiers_path`; mem-report's
selftest (`selftest_levels`); and the board fixture
`examples/workspaces/levels-cpp` (row `workspace-zephyr-cpp-levels`),
run by `realtime_tiers_e2e` cell `zephyr/cpp-levels`: three levels at 50,
20 and 5 Hz, the two less urgent callbacks running 15 and 60 ms; the cell
requires an urgent tick inside a middle callback, urgent and middle markers
inside a lazy callback, no lazy marker inside a middle callback, each
spawned level on its derived 16,384 B stack, and `mem-report --levels
--check` clean (`the_level_order_verdict_reads_preemption_off_the_console`
proves the verdict can fail). One-level images are unchanged: they spawn
nothing and emit none of this.

**Touches:** `packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c`,
`packages/boards/nros-board-zephyr/src/entry_tiers.rs`,
`packages/cli/nros-entry-lower/src/zephyr_image.rs`,
`packages/api/nros-c/include/rclc/detail/tier_task_memory_zephyr.h`, the C++
FFI, `zephyr/Kconfig`.

**Acceptance:**

- Board (Zephyr QEMU): a 3-level fixture checks the preemption order.
- The cost report's numbers equal the generated sizes.
- The per-level stacks equal the derived values.
- One-level images: generated sizes unchanged.

**Depends on:** P5, P6, P7.

### P9b -- Levels as workers over one executor (closes P9 deviation 1)

P9 left each level a whole executor (context + backing, ~14.4 KB on
`levels-cpp`). P9b makes the levels worker threads over one executor per
core; the design note is RFC-0108 "As built in P9b". Four PRs, in order:

- [x] 1. The lock seam: `nros_node::executor::exec_lock` (`ExecLock`,
      `ExecGuard`), `Executor::set_exec_lock`, `nros_cpp_executor_set_lock`;
      every nros-cpp / nros-c entry that touches an executor takes the lock
      before its reference exists (`CtxRef`, `CExecRef`, `Locked`,
      `CLocked`). No lock is installed yet, so nothing changes at runtime.
- [x] 2. The pass in steps: `Executor::spin_once_shared` and
      `Executor::spin_band_once` (`BandWorker`) run a pass over a raw
      pointer and release the lock around each user callback when it
      carries a thread identity (`ExecLock::with_thread_id`); the running
      mark keeps other passes off the entry; a timer cancel or reset from
      another thread is deferred to the callback's return, other entry
      operations from another thread answer as for a stale handle; the
      scan, sweep and drain take only the pass's band
      (`Executor::set_worker_band`); `set_reprobe_each_callback` stays once
      per pass by default. `nros_cpp_spin_once` holds no reference across
      its pass. Tests: `two_workers_never_run_one_callback_at_once`,
      `a_band_worker_runs_only_its_band`.
- [x] 3. Scheduler-owned accounting: each band's timers advance only on
      its worker's passes, by that worker's own elapsed time; sporadic
      budgets stay executor-owned under the lock and are charged per
      callback by the worker that ran it; only the executor's own worker
      drives the transport. Per-worker band park: a `BandWorker` with a
      `BandWaiter` (`attach_band_worker`) parks, the lock released, until
      its band's next timer or a signal; an attributed arrival signals only
      its band's waiter, an unattributed one every worker
      (`WakeCtx::mark_ready` / `signal`). The executor's own worker parks
      and waits with the lock released too, and its park bound counts only
      its band's timers. Tests: `a_band_worker_parks_until_its_timer_is_due`,
      `a_signal_ends_a_band_park`,
      `an_attributed_arrival_wakes_its_band_and_an_unattributed_one_wakes_all`.
- [x] 4. Runner, FFI, cost report: the Zephyr runner opens ONE executor
      per core (`nros_cpp_init_levels`, carved from the entry's
      `NROS_CPP_LEVELS_BACKING_SIZE` static), installs its lock and thread
      identity, and every spawned level is a worker over it:
      `nros_cpp_level_open(scheduler, level, name, stack_bytes)` seats its
      record and wake object, its setup registers on the owner's handle
      (`nros_cpp_scheduler_owner`; the slots are the level's,
      `Executor::set_registering_level`), and it loops on
      `nros_cpp_level_spin_once`. The entry emits one executor block and one
      levels backing; `mem-report --levels` reports a level as its seat, its
      thread object and its stack. `levels-cpp` (native_sim, `.bss`):
      43,736 B before (P9) -> 22,280 B after (context 3,184 + backing
      18,408 + scheduler 336 + 2 x 176 thread objects).

**Measured before P9b** (`levels-cpp`, native_sim): three levels as
executors 43,736 B `.bss` (each 3,144 context + 11,288 backing, plus 176 B
thread objects and an 88 B scheduler); the same nodes as one level 16,296 B
(3,144 + 13,112 + 40).

**Measured after P9b** (same fixture): 22,280 B `.bss`: one context
(3,184), one backing for every level's members (18,408; the tiers' own
sizings summed, so 5,296 B above the one-level derivation's 13,112), the
scheduler (336: 3 seats of 104) and 2 x 176 B thread objects. A spawned level
costs its seat, its thread object and its stack.

### P10 -- The other runners (amended RFC-0107 step 6)

- [ ] Linux / POSIX: one `SCHED_FIFO` pthread per level.
- [ ] NuttX: `nros-board-nuttx-qemu/c/nuttx_run_tiers.c`.
- [ ] FreeRTOS: `nros-board-freertos/c/freertos_run_tiers.c`, with derived
      stacks and `configUSE_TIME_SLICING = 0` when the image has levels.
- [ ] ThreadX: `nros-board-common/c/threadx_hooks.c`, the band's threshold
      set through `tx_thread_preemption_change`.
- [ ] Kernel budget enforcement where the kernel has it (Linux
      `SCHED_DEADLINE`, NuttX `SCHED_SPORADIC`), one PR per platform, after
      its runner.

**Acceptance:** per platform, the P9 3-level preemption fixture on that
platform's QEMU or host lane, and the derived stacks in the cost report.

**Depends on:** P9 (each runner PR independent of the others).

### P11 -- Retire the OS-priority worker pool

- [x] Goal: remove the 110.F opt-in pool now that levels cover it (RFC-0108
      alternative 4, decision 3).

**Done (as built).** Removed: `executor/os_priority.rs`,
`executor/ready_set/os_priority.rs`, `executor/worker_item.rs` (the pool's
hand-off protocol, issue 1705) and its tests, the `scheduler-os-priority`
feature, `Executor::register_os_priority_dispatcher`, the pool's fields
and dispatch site in `spin.rs`, the executor-value allowance for it, the
`SlotTag::WORKER_QUEUED` flag (`IN_FLIGHT` is `IN_DISPATCH` now), and the
lane's `--features alloc,rmw-cffi,scheduler-os-priority` clippy line. The
book page names tiers (and levels) for the bridge case instead.

- **Users migrated:** the pool had no caller outside `nros-node` (no C or
  C++ entry point, no example, no board). Its one test becomes
  `an_os_pri_bound_callback_runs_on_the_spin_thread`.
- **Kept:** `SchedContext::os_pri`, now documented as recorded and not
  acted on: generated entries write the tier's kernel priority there, and
  the C and C++ sched-context configs carry it (an ABI field). The
  deferred-release machinery stays (`RELEASE_PENDING`,
  `reap_deferred_releases` as the backstop): a release requested from
  inside a callback still defers to its return.
- **The api-parity ledger** had no row for the pool (its method was
  feature-gated out of every extracted surface), so none was removed.
- **References** to `scheduler-os-priority` remain only in records of the
  past: this phase's and RFC-0108's description of the retirement, and the
  completed phase-359 campaign's tables.

Unsafe census: the removal is locked in (`--write-baseline`).

**Touches:** `executor/os_priority.rs`, `executor/ready_set/os_priority.rs`,
the `scheduler-os-priority` feature in `packages/core/nros-node/Cargo.toml`,
its `cfg` sites in `executor/spin.rs` and `executor/ready_set/mod.rs`, the
clippy lane in `just/check/lanes.just`, and the book page
`book/src/user-guide/cross-backend-bridges.md`.

**Acceptance:** the workspace builds and tests with the feature gone; no
reference to `scheduler-os-priority` remains outside archived docs; the
executor size ratchet does not grow.

**Depends on:** P6.

## Island adoption (consumer validation)

These items are tracked here because they validate this phase on the board
that motivated it. They are implemented in the island repo
(`simple-autoware-safety-island`), not in nano-ros. Each is a pin bump plus
island changes, checked on the S32K344 board.

### I1 -- Supervisor safe for N levels, board-checked at N = 1

- [ ] Route 1 owned by the handler's level: P7's level query when it has
      landed, or until then a latch on the handle for which the route watch
      succeeded.
- [ ] A per-level SWT0 check-in mask.
- [ ] A spin-gap statistic per level.
- [ ] `prio_take` records every level.

**Touches (island):** `src/zephyr_entry/src/island_supervisor.c:616-638`
(`island_supervisor_spin_after`), `:654-688` (the SWT0 feed).

**Acceptance (board):** every planning act PASSes on the unchanged one-level
image.

**Depends on:** nothing in nano-ros (P7 replaces the latch later).

### I2 -- Pin bump with P1-P3, measure mrm_handler jitter

- [ ] Still one thread; dispatch is now in rank order.

**Acceptance (board):** acts PASS; the handler's tick jitter is measured
against today's 14.84 ms [M]; the priority record names both operators
(P1).

**Depends on:** P1, P2, P3, I1.

### I3 -- Three levels, with a 2-level fallback

- [ ] Remove the authored `[tiers.mrm]` and the `group_tiers`; expect three
      levels at ranks 5, 7 and 8, and no rule-2 degradation.
- [ ] Record DTCM and SRAM [M] in place of RFC-0108's estimates.
- [ ] If the margins are thin, use `max_levels = 2`: {`mrm_handler`} |
      {emergency operator, comfortable operator}.

**Acceptance:** the island's board build and priority check agree on the
level count; DTCM stays under the 122,880 B guard; SRAM is recorded.

**Depends on:** P5, P7, P9, I2.

### I4 -- Full board check

- [ ] The planning acts encore, a and b, and the 300 ms overrun encore: every
      one PASS.
- [ ] Route 1 judged only from the handler's level.
- [ ] SWT0 fires 0.
- [ ] Stack high-water per level from the thread analyzer, against the
      derived stacks.
- [ ] The handler's tick jitter compared with I2.

**Depends on:** I3.

## Dependency summary

| item | depends on |
| --- | --- |
| P1 | none |
| P2 | none |
| P3 | P2 |
| P4 | none (rlm release gates its pin bump; separate approval) |
| P5 | P1, P2, P4 |
| P6 | P2, P3; RFC-0107 steps 1-3 (merged) |
| P7 | P5, P6 |
| P8 | P5 |
| P9 | P5, P6, P7 |
| P10 | P9 |
| P11 | P6 |
| I1 | none |
| I2 | P1, P2, P3, I1 |
| I3 | P5, P7, P9, I2 |
| I4 | I3 |

## Out of scope

- **General per-callback preemption thresholds** (overlapping non-preemptive
  groups). A later extension, with the triggers RFC-0108 D3 names.
- **Core assignment / load partitioning**, including who splits an end-to-end
  deadline across images. A later phase; this one keeps the derivation keyed
  by (core, rank) with `core` unset.
- **EDF across levels.** EDF orders work within a level only, until an image
  needs more.

## Acceptance (phase)

- P1-P11 merged.
- The island runs on derived levels with I4 green.
- RFC-0108 moves to Accepted, with its measured numbers in place of the [E]
  estimates.
