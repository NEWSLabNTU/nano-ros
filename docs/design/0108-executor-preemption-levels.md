---
rfc: 0108
title: "One ordered ready set per core, N preemption levels derived from the contract -- ordering is separated from preemption"
status: Draft
since: 2026-10
last-reviewed: 2026-10
implements-tracked-by: [phase-509]
amends: [rfc-0002, rfc-0107]
supersedes: []
superseded-by: null
---

# RFC-0108 -- One ordered ready set per core, N preemption levels

**Status:** Draft (2026-10-21). P1-P8 and P11 are built; the rest is a proposal.
The open questions of the first draft are decided (see Decisions); the work is
tracked by [phase-509](../roadmap/phase-509-executor-preemption-levels.md).

**Amends:** [RFC-0002](0002-rt-execution-model.md) (dispatch order inside an
executor becomes the contract's rank, not registration order) and RFC-0107
(shared executor core; branch `docs/rfc-0107-shared-executor-core`, steps 1-3
merged, steps 4-6 open): its per-tier dispatcher is split into one scheduler
per core plus one worker per preemption level. **Builds on:**
[RFC-0079](0079-priority-is-allocated-not-authored.md) (the rank and the
priority address plan) and [RFC-0106](0106-executor-park.md) (how a worker
waits).

Legend for numbers: **[M]** measured (source named), **[E]** estimated (method
named), **[C]** read from code (file:line; nano-ros paths relative to the repo
root, at `origin/main` 6c63994586 unless stated).

## Summary

Today a nano-ros "tier" is a whole executor on its own thread. The tier is at
once the unit of ORDERING (which ready callback runs next) and the unit of
PREEMPTION (which running callback can be interrupted). Two consequences:

1. Inside one executor, ready callbacks run in **registration order**, not in
   rank order, so the ranks RFC-0079 derives are invisible to the dispatcher
   unless every rank gets its own executor.
2. Every rank that wants to preempt another pays for a whole executor:
   +7,096 B of DTCM per extra tier on the safety island [M], plus a 16 KiB
   stack.

This RFC separates the two:

- **Ordering:** per core, ONE logical ready set inside one executor, keyed by
  the contract's priority (a FIFO per priority level plus a bitmap) or by
  absolute deadline (EDF). This is fixed first, for one thread.
- **Preemption:** N worker threads over that ready set, one per **preemption
  level**. Each level has its own stack and worker record, and worker k
  dispatches only work at or above its floor. N = 1 is fixed-priority
  non-preemptive scheduling; N = number of ranks is fully preemptive; anything
  between is preemption-threshold scheduling (Wang and Saksena 1999; Saksena
  and Wang 2000) with contiguous non-preemptive groups (bands). General
  per-callback thresholds are a later, backward-compatible extension.
- **N is derived:** one level per distinct rank by default; merged by the
  preemption-threshold rule when per-callback execution times exist; capped
  by a per-image level count. Every member that loses its rank is reported.
- **Each callback carries a task record** (release kind, deadline, budget).
  Event callbacks with no rate must declare a budget or a minimum
  inter-arrival time. On a safety image (one whose contract supervises a
  deadline) the build refuses them; elsewhere it reports them as
  unanalysable.
- **Scope:** one ready set never spans more than one core's scheduling
  domain. SMP runs one executor core shared by per-core schedulers;
  heterogeneous processors run separate images joined by a transport, with
  the end-to-end deadline split into per-segment budgets. Assigning work to
  cores is a separate, later phase that this design stays compatible with.

## Motivation / problem

### 1. Dispatch order inside one executor is registration order

`Executor::spin_once` (`packages/core/nros-node/src/executor/spin.rs:10692`)
works in phases:

- **Readiness scan, once per call.** The ready bits are collected once
  (`spin.rs:11112-11121`: "`pop_next` yields registration order (lowest bit
  first)"). A callback that becomes ready while this pass dispatches waits for
  the next `spin_once`.
- **Bucketing by the sched context's `Priority`** (`spin.rs:11387-11404`,
  `:11433-11455`). `Priority` has three values, Critical / Normal / BestEffort
  (`executor/sched_context.rs:85-101`, `COUNT = 3`). An EDF-class context is
  keyed by its deadline; every other class by its slot index
  (`spin.rs:11686-11695`: `sort_key: if is_edf { deadline_us } else { i as u32 }`).
- **Drain:** bucket by bucket, EDF heap then FIFO bitmap
  (`spin.rs:11892-11898`). The FIFO pops the lowest set bit
  (`executor/ready_set/mod.rs:56-58`, `:113`), which is registration order.

What generated code puts into those keys:

- Codegen makes **one sched context per tier**, not per node or group
  (`packages/cli/nros-cli-core/src/codegen/entry/lower.rs:560-575`). The
  tier's kernel priority goes into `os_pri`; nothing sets `Priority`.
- `SchedContext::from_tier_policy` leaves `priority` at its default
  (`sched_context.rs:584-618`; the default is `Priority::Normal`,
  `sched_context.rs:549`), and it never produces the `Edf` class.

**So inside one tier every member is in the Normal bucket, FIFO, in
registration order.** This differs from rclcpp's default executor, which orders
by entity type (timers, then subscriptions, services, clients, waitables),
and matches RFC-0002's description of the original model
(`0002-rt-execution-model.md:69-79`, "Dispatch order = registration order").
Neither honours the rank. Only the C (`nros-c/src/executor.rs:4582-4584`) and
C++ (`nros-cpp/src/lib.rs:4467-4469`) hand-written APIs can set a bucket, and
then only one of three.

### 2. The island: three ranks on one thread

The Autoware safety island (S32K344, Zephyr) runs three components with three
distinct ranks [M] (`nros_priority_plan.json` of the island board build,
2026-10-16): `mrm_handler` 5, `mrm_emergency_stop_operator` 7,
`mrm_comfortable_stop_operator` 8 (pool `app [5,14]`, transport `[4,4]`,
smaller is more urgent). The island authors one tier, `[tiers.mrm]`, and binds
all three components to it. That is rule 2 of tier derivation
(`packages/core/nros-orchestration-ir/src/derive.rs:611-680`): one tier, one
thread, at the most urgent member's priority.

- The handler is supervised (`call_mrm`, 149 ms budget) and carries a 100 ms
  deadline; the 30 Hz emergency operator has none.
- Under section 1, the handler's tick can wait behind any operator callback
  registered before it, and behind a whole operator callback that started
  first. The handler's tick jitter is 14.84 ms [M] (island contract), and how
  much of it is operator blocking is not measured.
- **Reporting defect:** rule 2's degradation names only the FIRST member whose
  rank differs (`derive.rs:631-649`, `members.iter().find(...)`). The island's
  record names `mrm_emergency_stop_operator` (7 vs 5) and not
  `mrm_comfortable_stop_operator` (8), which also lost its rank.

### 3. Rule 4 over-splits

When a group names no tier, rule 4 gives each NODE its own tier,
`derived-<node>` (`derive.rs:530-608`; the name at `:534`). Equal ranks pay a
thread each and gain nothing the fixed-priority analysis can see: two threads
at one priority on a FIFO kernel behave like one thread with both groups. The
`derived-tiers-cpp` fixture gets four tiers for two ranks.

### 4. What a tier costs

| item | value | kind |
| --- | ---: | --- |
| island DTCM, one tier (nano-ros 77e4175e77) | 103,224 B | [M] island board build |
| second tier (`estop` = emergency operator alone) | +7,096 B | [M] island phase-13 measurement |
| of which: backings / `CppContext` / thread object / monitor rows | +3,904 / +2,680 / +272 / +240 B | [M] ELF symbol diff |
| DTCM ceiling (131,072 B minus the 8 KiB guard) | 122,880 B | [C] island `linker/dtcm-headroom.ld:8` |
| spawned-tier stack on Zephyr | 16,384 B (SRAM) | [C] `zephyr/Kconfig:435` `NROS_ZEPHYR_TIER_STACK_SIZE` |
| island SRAM in use | 280,680 of 327,680 B | [M] island board build |
| spawned-tier stack on FreeRTOS (default) | 262,144 B | [C] `packages/cli/nros-entry-lower/src/lib.rs:201` |
| RFC-0107's baseline (d5fe58a518): one / two tiers | 112,464 / 127,400 B | [M] RFC-0107 |

RFC-0107 steps 1-3 removed most of the duplicated image-wide state. What is
left per tier is still a whole executor value, a whole backing, a thread, and
a stack. Three ranks on the island as three tiers would be about 117.4 KB of
DTCM [E: 103,224 + 2 x 7,096] against 122,880 B, and 313.4 KB of SRAM
[E: 280,680 + 2 x 16,384] against 327,680 B.

**The problem is structural:** the unit that buys preemption is a whole
executor, and the unit that orders work is a whole executor too. A level only
needs a stack and a small worker record.

## Design

### D1 -- The task model

Every callback (one dispatchable slot) gets a task record. It is derived from
the contract (rlm `ros-launch-manifest`), never written in `system.toml`.

| release kind | parameters | contract source | analysed as |
| --- | --- | --- | --- |
| **periodic** (timer) | period T; D = T unless stated | timer `rate_hz` / period; `max_latency` or deadline | periodic task (C, T, D) |
| **sporadic** | minimum inter-arrival T_min; D | an input with a declared minimum inter-arrival (new key) or a rate bound | sporadic task (C, T_min, D) |
| **served** (event-driven with a budget) | budget Q per server period P; server kind | an input with a declared budget (new key) | a periodic server (Q, P) |
| **unanalysable** | none | an event input with neither key | reported; best-effort placement (below) |

- **C** (execution time) is optional per callback. It comes from the
  contract's per-callback execution time (RFC-0078 WCET; `exec_ms`) when
  stated. Without C, the derivation still orders and groups, but it cannot
  check deadlines; it says so.
- **Servers.** A served callback is put behind a sporadic server (Sprunt, Sha
  and Lehoczky 1989), a deferrable server, or a constant bandwidth server
  (Abeni and Buttazzo 1998). To the analysis it is a periodic task (Q, P).
- **One ordering key per core:**
  - **Fixed priority:** deadline-monotonic, a server ranked by its P. This is
    the order rlm's chain-aware ranker already produces (it ranks by
    criticality then budget, with timers and input paths in one unit; RFC-0079
    section 3), so the key IS the rank.
  - **EDF:** the absolute deadline, release time plus D. A CBS assigns its
    own deadlines. The existing `SchedClass::Edf` and `BucketedEdfSet`
    (`spin.rs:11403`) are the mechanism.
- **Budget enforcement, two grades:**
  1. **At callback boundaries** (run to completion). The executor charges the
     measured runtime after each dispatch and refuses the next dispatch of an
     exhausted server until its refill. This exists today for
     `SchedClass::Sporadic` (refill `spin.rs:11406-11431`; charge
     `consume_dispatch_runtime_us`, `spin.rs:11781`). The overrun is bounded by one callback.
  2. **By the kernel**, when the server has a level of its own and the kernel
     enforces budgets (Linux `SCHED_DEADLINE`, NuttX `SCHED_SPORADIC`; D6).
- **Event callbacks with no rate** are the one place the contract must grow:
  an input needs `min_interarrival` or `budget` + `budget_period`
  (an rlm schema change, a tag, a pin move). An input with neither key is
  handled by the kind of image:
  - **Safety image: refused.** The build fails and names each such callback
    and the two keys that would fix it.
  - **Any other image: reported.** The build emits an `unanalysable`
    degradation naming each such callback. It is placed at the least urgent
    level of its node's band, and it is never allowed to raise a level's
    blocking term above a declared deadline without a report.
- **A safety image** is an image whose contract supervises at least one
  deadline: a node path carrying `supervise:` (`contracts.node_paths`). The
  derivation already computes this set (`supervised_nodes`,
  `derive.rs:72-80`) to place the supervisor, so the rule needs no new input.
  An explicit knob was rejected: it is one more setting to forget, and an
  image that supervises a deadline has already claimed a timing reaction that
  an input with unbounded arrivals can break (its callbacks add blocking and
  interference no analysis can bound).
- **As built in P4** (`nros-orchestration-ir::task_record`; rlm v0.1.68).
  Five points the table above left open:
  - **Inputs are subscriptions and service servers.** Both take the keys
    (`sub.<e>` and `srv.<e>`: `min_interarrival`, `budget` +
    `budget_period`). A budget decides the kind (served) even beside a
    `min_interarrival`. rlm refuses a budget without its period, a period
    without its budget, a budget longer than its period, the keys under
    `pub:`/`cli:`, and a `min_interarrival` above `1 / min_rate_hz`.
  - **A rate bound** is the subscriber's `max_rate_hz` or the topic's
    `rate_hz` (T_min = 1 / rate). A `min_rate_hz` is a floor the subscriber
    needs and bounds nothing, so it releases nothing.
  - **A service client's response** follows the node's own request, so it
    inherits the release of the paths that issue it (`output: [<client>]`):
    sporadic at the least T among them, unanalysable when one of them reads
    an unbounded input or none is known. An action endpoint has no release
    key in the grammar yet, so its callbacks are unanalysable.
  - **D** is the least `max_latency` of the paths the callback runs (and a
    server's `max_response`) when stated, else implicit: T, T_min or P. C is
    the summed RFC-0078 bounds of those paths when every one declares one.
  - **Where:** the refusal is `task_record::check_model`, on all three bake
    roads (`codegen entry`, `codegen-system`, `nros::main!`), beside the FTTI
    check. The report is an `unanalysable` degradation in the derived
    schedule's record (`DerivedSchedule::tasks` carries the records). The
    placement at the least urgent level of the node's band is P5's, with
    levels.

### D2 -- Ordering: one ready set per core, keyed by priority

- **Fixed priority:** a multi-level queue, one FIFO per priority level plus a
  presence bitmap, so the pick is a count-leading-zeros. The number of levels
  is the number of distinct ranks on the core (at most the pool width;
  RFC-0079's `pool.app`), not three.
- **EDF:** a heap or ordered list keyed by absolute deadline.
- **The key comes from codegen,** per node, as the rank, normalised so that 0
  is the most urgent whatever the kernel's direction; every callback group of
  the node shares it. (Amended in P2: the first draft said per callback group,
  but the derivation ranks nodes, and one tier's single sched context cannot
  carry several ranks. Codegen seeds each ranked node by name before it is
  created, `nros_cpp_bind_node_rank`.) The C and C++ `Priority` API
  (Critical / Normal / BestEffort) keeps working as three reserved ranks:
  `Critical` dispatches at level 0, `Normal` at 1 + rank, `BestEffort` at the
  last level.
- **Ties** (equal rank) stay FIFO in registration order, which keeps today's
  behaviour bit for bit for an image with one rank.
- **No stale snapshot.** After every callback the worker picks again, from the
  ready set as it is now. Readiness that is pushed (RFC-0084 attributed slots)
  is merged into the ready set after each callback; a slot that must still be
  probed is probed at the top of the pass as today. So an urgent callback made
  ready by a less urgent one runs before the rest of the pass. Each entry runs
  at most once per pass, so a callback that readies itself cannot hold the
  pass. Timers are in every pass already (they are always ready, and their
  clock advances by the pass's elapsed time), so a timer becomes due at a pass
  boundary, not mid-pass. **Decided in P3:** an unattributed slot is probed
  once per pass by default; probing it after every callback is an opt-in
  (`Executor::set_reprobe_each_callback`). Measured on the host (release,
  x86-64): 32 unattributed subscriptions, 4 ready per pass, empty callbacks,
  380-385 ns per pass once per pass against 565-577 ns after every callback,
  +46-48 ns per callback (1.65-1.71 ns per extra probe of a mock with no
  FFI) [M]. The cost is one probe per unsettled slot per callback, and a
  real XRCE or uORB probe crosses the rmw C ABI on a slower MCU.
- **Non-preemptive within a level** (run to completion), as today.

This is step 1 of the plan, and it is useful with one thread: it makes the
island's single tier dispatch in rank order without any second thread.

### D3 -- Preemption: N levels over one ready set

- **A level** is a worker thread with its own stack, a worker record (current
  dispatch, park state, alive slot) and a **floor**: the least urgent rank it
  may dispatch. Levels partition the core's ranks into **contiguous bands**,
  most urgent band on the highest kernel priority.
- **Worker k** picks the most urgent ready item whose rank is at or above its
  floor. An arrival wakes the worker of the arrival's own band (RFC-0106 wake,
  D7 below). On one core a lower worker runs only when every higher worker is
  parked, so the kernel's preemption between workers is exactly preemption
  between bands, and inside a band dispatch is non-preemptive.
- **Equivalence.** This is fixed-priority preemption-threshold scheduling in
  which every callback's threshold is the most urgent rank of its band:
  - N = 1: fixed-priority non-preemptive (the island today, once D2 lands);
  - N = number of distinct ranks: fixed-priority fully preemptive (the
    "one tier per distinct priority" rule);
  - 1 < N < ranks: non-preemptive groups (Saksena and Wang 2000).
- **Decision: contiguous bands are the level model.** Each worker owns a
  consecutive range of ranks; its kernel priority is fixed; every member's
  threshold is the most urgent rank of its band; its stack is sized for its
  largest member. A non-contiguous group would put a band between two members
  of one level, and its worker would then either run below that band (a member
  waits behind less urgent work) or above it (a less urgent member starts
  before the band).
- **General per-callback thresholds are a later extension.** They allow
  overlapping non-preemptive groups, at the cost of a priority raise and
  restore on every dispatch and a worker pool sized by the deepest preemption
  chain (the longest sequence of callbacks each of which may preempt the one
  before) instead of by the band count. Triggers for taking it:
  - a contract that can state non-preemption or shared-state relations between
    callbacks (there is nowhere to state them today, so nothing could derive
    an overlapping group); or
  - a kernel with native thresholds (ThreadX `tx_thread_preemption_change`),
    where the per-dispatch raise is the kernel's own operation.

  Bands are the special case of thresholds in which every member's threshold
  gamma is the most urgent rank of its band, so the extension is backward
  compatible: an image derived with bands keeps its schedule under it.
- **Worked example.** Four callbacks, lower number more urgent: H = 1, E = 2,
  C = 3, L = 4. A running callback i is preempted by a ready j only when
  rank(j) < gamma_i.
  - **Bands {H, E} | {C, L}** (gamma_H = gamma_E = 1, gamma_C = gamma_L = 3):
    E blocks H (a started E finishes before H), L blocks C, and H and E
    preempt C and L.
  - **A case bands cannot express:** C and L must not preempt each other
    (say they share state), C and E must not preempt each other, but E must
    preempt L. Thresholds gamma_H = 1, gamma_E = 2, gamma_C = 2, gamma_L = 3
    express it: with L running, E (2 < 3) preempts and C (3 < 3 fails) waits;
    with C running, E (2 < 2 fails) waits and H (1 < 2) preempts. The worst
    chain is L, then E, then H: three workers for four callbacks.
  - **Why every banding fails.** A banding is a choice of cuts at H|E, E|C and
    C|L. "C and L do not preempt each other" forbids the cut C|L; "C and E do
    not preempt each other" forbids the cut E|C; "E preempts L" needs a cut
    between E and L, which is one of those two. All eight bandings:

    | bands | fails because |
    | --- | --- |
    | {H, E, C, L} | E cannot preempt L |
    | {H} \| {E, C, L} | E cannot preempt L |
    | {H, E} \| {C, L} | E preempts C |
    | {H, E, C} \| {L} | C preempts L |
    | {H} \| {E} \| {C, L} | E preempts C |
    | {H} \| {E, C} \| {L} | C preempts L |
    | {H, E} \| {C} \| {L} | E preempts C, and C preempts L |
    | {H} \| {E} \| {C} \| {L} | E preempts C, and C preempts L |
- **Response time.** For a callback i in band b:

  `R_i = C_i + B_i + sum over j more urgent than i of ceil(R_i / T_j) * C_j`

  with `B_i = max C_k` over the LESS urgent members of i's own band (a started
  callback finishes), plus short kernel sections (the zenoh-pico tx mutex,
  which inherits priority). Lower bands contribute nothing. Because the tail
  of a band is non-preemptive, the analysis examines every job in the level-i
  busy period, not only the first (Bril et al. 2017 give the formulation for
  preemption thresholds).
- **Stack.** On one core, at most one callback per band is in progress, and
  a band can be interrupted only by more urgent bands. Total stack is the sum
  over levels of each level's largest member stack, which is the
  non-preemptive-group stack saving of Saksena and Wang 2000 and Gai et al.
  2001.
- **Decision: worker stacks are derived per level** from the members, never
  the platform's default thread stack (FreeRTOS's 262,144 B default, section
  4). A level's stack is its largest member's stated stack (a `stack_bytes`
  override, or a measured high-water recorded for it) plus the worker's own
  frame. A member with no stated figure takes the port's per-callback default,
  and the cost report (D4) names each member that did.
- **What a level costs** [E]: a stack plus a worker record. The worker record
  is the dispatch half of RFC-0107's dispatcher, which that RFC bounds above by
  the 2,672 B `CppContext` [M]; the kernel thread object is 272 B [M]. The
  backing, the callback slots, the arena and the monitor rows are per core,
  not per level. Method: RFC-0107's step-3 estimate minus the ready set and
  callback slots, which move to the per-core scheduler. To be measured at
  phase step P9.

### D4 -- Deriving N, and the budget

N is a pure function of the contract, the priority plan and the image's cap,
so it is reproducible and reviewable at codegen time.

1. **Default:** one level per distinct rank on the core. Equal ranks share a
   level. This replaces rule 4's one-tier-per-node.
2. **Merge with execution times** (C stated for every callback involved):
   find the fewest contiguous bands for which every callback with a deadline
   still meets it under D3's response time. With r ranks (r is at most the pool
   width) a dynamic program over band boundaries is O(r^2) analyses; ties are
   broken by the largest minimum normalised slack `(D_i - R_i) / D_i`.
3. **Merge without execution times,** only to meet the cap: merge the least
   urgent adjacent pair, skipping a pair whose more urgent member holds a
   deadline or a supervised route, until the count fits. Only when every
   remaining pair touches such a band is it merged, with a degradation naming
   the added blocking source.
4. **The cap is a count (decided):** `max_levels` per image, a Kconfig symbol
   (`CONFIG_NROS_EXECUTOR_MAX_LEVELS`) rendered from an image knob, or the
   platform's equivalent knob where there is no Kconfig, read directly on host
   and Rust roads. Defaults per platform in D6. The pool width is a second,
   independent cap (RFC-0079 already compresses past it). A byte budget per
   memory region was rejected as the input: bytes are known only after a
   target compile, so the derivation would depend on build order.
5. **Report:** one `level-budget` degradation per member that loses its rank,
   naming the cap, the rank it would have had, and the band it shares. This
   also replaces rule 2's first-member-only report (section 2).
6. **Authored still wins.** An authored tier (rules 1-3) is a pin: its members
   form one band at the authored priority, and derivation fills only what is
   unbound. `stack_bytes`, `core` and a priority pin stay expert overrides
   (RFC-0079 primitive reduction).
7. **Cost report** after the target compile: per region (DTCM, SRAM, ...)
   and per level its stack and worker record, per core its scheduler and
   backing, as text and JSON. It is a check, never an input. The image's own
   guard (the island's linker assert) stays the authority, so the derivation
   does not depend on build order.
8. **Equal kernel priorities are merged.** On a time-slicing kernel (FreeRTOS
   with `configUSE_TIME_SLICING`, Linux `SCHED_RR`) two levels or authored
   tiers that would run at one kernel priority become one level (decided).
   Separate threads at one priority buy no preemption between them, only
   memory, and slicing would preempt a callback mid-run. Real-time levels run
   with time slicing off (D6).

**As built in P5** (`nros-orchestration-ir::levels`). Points the rules above
left open:

- **Where the analysis lives (decided):** `nros-orchestration-ir`, beside
  the derivation that consumes it. It needs the board's plan, the authored
  pins, the task records and the cap, none of which rlm's platform-free
  ranker holds.
- **Until P9 a derived level runs as one derived tier** at the band's most
  urgent priority: `derived-<node>` for a level of one node (unchanged from
  rule 4), `derived-level-<k>` for several, with their tightest period and
  deadline. A per-node sporadic budget is dropped on a shared level, with a
  `budget` degradation.
- **Rule 2** engages only when no tier is authored and every derived
  callback is analysable with C. A callback's response time depends only on
  its own band, so the program is O(r^2) band analyses. The response time
  examines every job of the level-i busy period; a served callback is
  (Q, P); before it starts, equal ranks interfere (FIFO). If the fewest
  feasible bands exceed the cap, rule 3 runs on top; if no banding keeps
  every deadline, the levels stay one per rank with a `feasibility` line per
  miss. The bake roads select no RFC-0078 profile yet, so on them C is
  absent and rules 1 and 3 decide; `derive_tiers_with_wcet` takes one.
- **Rule 3's protected band** holds a node with a deadline (the realizer's
  `deadline_us`) or a supervised route.
- **Rule 4:** `CONFIG_NROS_EXECUTOR_MAX_LEVELS` (default 8) is read from the
  image's `.config`; without one, the D6 default. No `system.toml` knob yet.
- **Rule 6:** pins count toward the cap; a free band never spans a pin's
  priority, so the pins can force more levels than the cap, which one
  `level-budget` line reports.
- **Rule 8** applies on FreeRTOS only. A derived band at an authored tier's
  priority joins it (its groups bind to that tier); two authored tiers at
  one priority are one level in the record, two threads until P9.
- **D1's placement** of an unanalysable callback at the least urgent level
  of its band is recorded per level (`best_effort`, at the band's least
  urgent rank); the runtime ranks per node until P6.
- The priority record carries `levels`; the Rust `nros::main!` road seeds
  ranks (`nros::__macro::seed_node_rank`), which P2 had left at 0.

### D5 -- Multiple and heterogeneous processors

**Rule: a ready set never spans more than one core's scheduling domain.**

| approach | how | analysis | fit |
| --- | --- | --- | --- |
| global (one queue, m cores) | any core runs the most urgent ready item | weak: the Dhall effect (Dhall and Liu 1978) makes global RM/EDF fail at low utilisation; migration cost per job | rejected for analysed work |
| partitioned | one ready set per core, callbacks placed offline | uniprocessor analysis per core (D3); bin packing decides placement | **adopted** |
| semi-partitioned | a few callbacks split across cores at fixed points | per-core analysis with known migration points | allowed later, only with offline split points |
| work stealing | idle cores take queued work | none: placement depends on timing | best-effort class only (alternatives) |

- **SMP (one ISA, coherent memory):** one immutable executor core (RFC-0107's
  `ExecutorCore`, read-only after boot) shared by every core; per core a
  scheduler (ready set, callback slots, release and budget accounting, arena
  partition) and its levels. A tier becomes a **(core, level)** pair. The
  three locked items of RFC-0107's concurrency table (arena at runtime,
  parameter store, violation ring) take a real spinlock.
- **Uniform multiprocessors** (big.LITTLE): one ISA, different speeds. The
  task record's C is stated for a reference core and scaled per core type
  (`C / s_k`); the per-core analysis uses the scaled value.
- **Different ISAs or non-coherent memory** (Cortex-M7 + M4; Orin A78 + R52;
  host + MCU): separate images, separate executors, joined by a transport. An
  end-to-end chain deadline is split into a deadline per segment plus the
  transport's bound; each image derives its levels against its own segment
  deadlines.
- **Assigning callbacks to cores is out of scope.** It belongs to a later
  load-partitioning phase. This RFC only fixes the interface it will use:
  the derivation's key is (core, rank), `core` is `None` today
  (`derive.rs:542-546`), and the budget of D4 is per core.

### D6 -- RTOS cooperation

**Division of labour.**

- **The executor owns:** ordering within a level, level membership (the
  bands), release accounting, and budget accounting at callback boundaries.
- **The kernel owns:** preemption between levels, affinity, and budget
  enforcement where it has it.
- **Outside the executor:** the transport band and the supervisor stay plain
  kernel threads, placed by RFC-0079's address plan, never levels.

Each platform publishes a capability table that the derivation reads:

| platform | levels as | EDF in the kernel | kernel budget | affinity | runtime stats | default `max_levels` |
| --- | --- | --- | --- | --- | --- | --- |
| Zephyr | one `k_thread` per level, static priorities | `CONFIG_SCHED_DEADLINE`: `k_thread_deadline_set` orders threads of EQUAL static priority by deadline; no budget, no admission | none (boundary checks) | `CONFIG_SCHED_CPU_MASK` | `CONFIG_SCHED_THREAD_USAGE` | 8 |
| Linux / POSIX | one pthread per level, `SCHED_FIFO` | `SCHED_DEADLINE` (runtime, deadline, period; kernel CBS with admission) | `SCHED_DEADLINE` throttling | `sched_setaffinity` | `/proc`, `clock_gettime` per thread | 8 |
| NuttX | one pthread per level, `SCHED_FIFO` | none | `SCHED_SPORADIC` (`CONFIG_SCHED_SPORADIC`: high and low priority, initial budget, replenishment period) | SMP affinity | `CONFIG_SCHED_CPULOAD` | 8 |
| ThreadX | threads with a native preemption threshold (`tx_thread_preemption_change`), so a band can also be one thread per callback at the band's threshold | none | none (boundary checks) | SMP builds only | `TX_EXECUTION_PROFILE_ENABLE` | pool width |
| FreeRTOS | one task per level | none | none (boundary checks only) | SMP builds only | `configGENERATE_RUN_TIME_STATS` | 4; stacks derived per level (D3), never the 256 KiB task default |

- **Ties on time-slicing kernels (decided).** FreeRTOS time-slices equal
  priorities by default (`configUSE_TIME_SLICING`), Linux under `SCHED_RR`.
  Levels and authored tiers that share a kernel priority are merged into one
  level (D4 rule 8), and real-time levels run with time slicing off:
  `SCHED_FIFO`, never `SCHED_RR`, on Linux and NuttX, and
  `configUSE_TIME_SLICING = 0` on FreeRTOS images with levels.
- **Zephyr deadline mode** is useful for a different job: when EDF is the
  ordering key, the levels can sit at one static priority and be ordered by
  `k_thread_deadline_set` with the band's earliest deadline. Without budgets
  it is not a server.

### D7 -- Waking a level

RFC-0106 D6 wakes every executor over a session on every arrival: with N tiers
an arrival costs N-1 spurious wakeups (7-9 us each on the host [M, RFC-0106]).
With levels the wake goes through the per-core scheduler: an arrival whose
slot is attributed (RFC-0084) wakes only the worker of its band; an
unattributed arrival wakes the most urgent parked worker, which re-picks.
Each worker parks on its own wake object until the minimum of its band's
declared deadlines (RFC-0106 D1), rounded up to the port's granularity (D3).

**As built in P6** (`nros-node` `executor/scheduler.rs`). The
`Scheduler` holds each slot's dispatch level, one atomic word of ready
bits and the band table (contiguous bands partitioning the levels, most
urgent first); `arrive` wakes only the arrival's band worker and
`arrive_unattributed` the most urgent parked one. A `Worker` holds its
band, the RFC-0106 park and its stack record, moved from `Executor`
unchanged; `Executor` is the one-level facade (one worker, every level).
A worker picks only its own band, not "at or above its floor" as D3 says:
its stack is sized for its band's members. Release and budget accounting
and the callback table stay in `Executor` until the runners spawn levels
over one scheduler (P9).

### D8 -- Prerequisites for more than one level

These hold for every image, and they block the first board run with N > 1.

1. **Supervised routes are owned per level.** A route is armed, judged and
   cleared by the level that hosts the supervised node, never by another
   level's spin. nano-ros exposes which level hosts a node: a generated
   `NROS_ENTRY_LEVEL_OF_<node>` beside the priority plan header, and a runtime
   query on an executor handle. Spin hooks receive the level index.
   - Island instance: `island_supervisor_spin_after` (island
     `src/zephyr_entry/src/island_supervisor.c:616-638`) asks the executor
     that just spun for its earliest supervised due, and on `UINT64_MAX` calls
     `island_supervisor_done(ISLAND_SUPERVISOR_ROUTE_EXECUTOR)` (`:632-634`).
     A level with no supervised rows would disarm route 1, which the handler's
     level armed, and the timer backstop for a stuck `call_mrm` would be lost.
     The route watch is filled once from whichever executor spins first
     (`:625-630`).
2. **The watchdog is fed per level.** A feed from any one spin hides a
   blocked level. Each level checks in to a per-level bit; the hardware
   watchdog is fed only when every level has checked in since the last feed.
   nano-ros provides the aggregate ("every level alive") from the per-sched
   context alive slots it already keeps. Island instance: SWT0 is fed from
   every spin (`island_supervisor.c:654-688`), and `last_feed_cyc` and
   `spin_gap_max_us` are written by every spin thread without a lock.
3. **No blocking service call to a less urgent or equal level.** A blocking
   call to a less urgent level is a priority inversion bounded by all the work
   in between; to the same level it is a deadlock (the server cannot run while
   the client holds the worker). The build refuses it when the contract names
   the client and server; asynchronous calls (send now, drain the reply on a
   later release) are allowed. The island's calls are already asynchronous.

   **As built in P8** (`nros-orchestration-ir::blocking_calls`; rlm
   v0.1.69). The contract named clients and servers but not whether a
   client waits, so the grammar gained `call: blocking | async` on a
   `cli.<e>` endpoint and on `actions.<a>` (every client it lists). Only a
   stated `blocking` is refused: at the client's own level as a deadlock,
   at a level that cannot preempt it (less urgent, or another tier at the
   same kernel priority) as an inversion. An unstated call to such a server
   is reported, never refused, so existing contracts build unchanged. Until
   P9 a level is a tier of the resolved table and a tierless image is one
   level; a node no tier of a multi-tier image holds is reported, and a
   server outside the image is not checked. The check runs on the three
   bake roads after each resolves its tier table.
4. **Shared state between levels** follows RFC-0107's concurrency table; with
   levels on one core a critical section suffices, on SMP a spinlock.

**As built in P7** (`nros-node` `executor/level_liveness.rs`, the C/C++
FFI, `nros_priority_plan.h`). An executor's worker carries its level; until
P9 a tier's level is its urgency order among the image's tiers, equal
priorities in table order, and the Zephyr runners place each tier's
executor by that rule while the header writes `NROS_ENTRY_LEVEL_OF_<node>`
by it. The "every level alive" aggregate is a per-image registry that
every spin checks into (the levels are separate executors until P9, so the
per-sched-context alive slots, which live inside one executor, cannot hold
it); `take_every_level_alive` is the watchdog feed. nano-ros has no
spin-hook ABI: a supervisor wraps the spin and reads the handle's level.

**As built in P9** (`zephyr_run_tiers.c`, the `nros_cpp_*` scheduler FFI,
the entry packs, `tier_task_memory_zephyr.h`, `scripts/nros-mem-report.py
--levels`). The Zephyr runner spawns one worker thread per level (each row
of the entry's table is a derived level, or an authored tier, which is one
band) over a per-core scheduler record: `nros_cpp_scheduler_open(core,
n_levels, owner, backing, bytes)` over the session the boot level opened,
`nros_cpp_level_adopt` for the boot level and `nros_cpp_level_open` for
every other, each seated with the stack its worker runs on. Three points
differ from the step above:

- **A level is still an executor over the shared session**, not a worker
  over one ready set. A second worker dispatching from one `Executor` needs
  the spin pass to drop its `&mut` borrow around every user callback, and
  RFC-0107's step 4 dispatcher is not on `main`; that refactor is not part
  of P9. The schedule is the one D3 describes: inside a level, rank order
  (D2), non-preemptive; across levels, the kernel's preemption by the
  level's priority. Release and budget accounting stay in each level's
  executor, which owns its members' slots, so nothing is shared and
  nothing moves. The cost differs: a level carries an executor context and
  its own backing (issue 1970), not only a worker record.
- **Generated sizes.** `NROS_CPP_SCHEDULER_BACKING_SIZE_0` is
  `NROS_CPP_SCHEDULER_BACKING_SIZE(n)`, the entry's static; one scheduler
  for the runner's table, which spans the image. There is no
  `NROS_CPP_CORE_BACKING_SIZE`: the backings stay per level
  (`NROS_CPP_TIER_BACKING_SIZE_<i>`).
- **Member stacks.** A spawned level that declares no `stack_bytes` gets
  `NROS_LEVEL_STACK_BYTES(member)`: the largest member's stack plus the
  worker frame (`CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME`, 4,096 B, an
  estimate). No contract or `system.toml` key states a member's stack yet,
  so every member takes the per-callback default
  (`CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE` minus the frame) and the cost
  report names it; a level's stack is then exactly the knob, as before.

**As built in P9b** (design note; items 1-4 land it in order). P9 left each
level a whole executor over the shared session. P9b makes the levels worker
threads over ONE executor per core, so a level costs a worker record and its
stack instead of a context and a backing. Measured on `levels-cpp`
(native_sim, `mem-report --levels`): three levels as executors take
43,736 B of `.bss` (3 x (context 3,144 + backing 11,288) + 88 scheduler +
2 x 176 thread objects); the same three nodes as one level take 16,296 B
(context 3,144 + backing 13,112 + scheduler 40). Each per-level backing
carries a fixed floor of about 10.4 KB that one shared backing pays once.

- **Ownership and borrows.** One `Executor` per core. A `&mut Executor`
  exists only while the executor's lock is held, and never across a user
  callback. A worker's pass runs as steps: (1) lock; reap deferred
  releases, advance its band's timers, scan its band's readiness, pick from
  the band's per-level FIFO and bitmap at the band threshold, and mark the
  picked entry running (`begin_dispatch`'s slot tag); (2) copy out
  `(try_process, data_ptr, desc_idx, delta)` and unlock; (3) call
  `try_process(data_ptr, ...)` with no executor reference live (the data is
  in the pinned arena, which never moves for the executor's lifetime); (4)
  lock; finish the dispatch (runtime and budget, route judge, deadline),
  clear the running mark, merge mid-pass arrivals, re-pick; unlock. A
  worker's own record (band, park, stack record) is the P6 `Worker`, owned
  by its thread; the ready set, slots, timers, sporadic budgets and monitors
  stay in the executor under the lock.
- **Locking.** One lock per executor: the port's RECURSIVE,
  PRIORITY-INHERITING mutex (`nros_platform_mutex_rec_*`: Zephyr `k_mutex`,
  POSIX `PTHREAD_PRIO_INHERIT`, FreeRTOS recursive mutex, NuttX with
  priority inheritance, ThreadX `TX_INHERIT`), installed on the executor by
  the runner before any second thread can reach it
  (`nros_cpp_executor_set_lock`). Every C and C++ FFI entry that touches
  the executor takes it before its reference exists (`cpp_ctx_checked` /
  `cpp_ctx_locked` return a `CtxRef`, nros-c's `get_executor` a `CExecRef`)
  and gives it back when that reference drops; a lookup that hands back an
  object inside the executor (an action client core, a service client, a
  node's session) hands back the lock with it (`Locked` / `CLocked`). It is
  recursive because, until the pass releases it around callbacks (item 2),
  a callback that calls back into the executor re-enters it on the same
  thread. It is never held across a park or a callback once item 2-3 land.
  An executor with no lock installed (every one-level image, every Rust
  `&mut` user) pays one predictable branch per entry and nothing else.
  Interrupt handlers never take it: arrivals use the scheduler's lock-free
  ready word and the band's wake object, and `from_isr` is unchanged.
- **Lock cost** (host, Intel Core Ultra 7 265K, glibc, recursive +
  `PTHREAD_PRIO_INHERIT`): uncontended lock+unlock 13.6-15.3 ns, a nested
  (recursive) pair 16.0-16.5 ns; two threads hammering one lock 165-550 ns
  per pair (the PI futex hand-off). Zephyr's `k_mutex` on target is to be
  measured on the island (I4); native_sim does not advance time while code
  runs, so it cannot price it.
- **Blocking bound.** With one priority-inheriting lock, a level i's
  callback is blocked, besides the non-preemptive tail of its own band, by
  at most ONE critical section of a less urgent worker:
  `B_i = max C_k (k less urgent, same band) + L_max`, where `L_max` is the
  longest locked step of any less urgent worker (a pick or a finish, never a
  callback, never a park). The response-time analysis (`levels::response_time`)
  gains `L_max` in its blocking term once item 4 measures it on the levels
  fixture. Until item 3, the executor's own worker still holds the lock
  across its park and transport drive, so `L_max` is that park.
- **Cancellation and timers.** Cancel, reset or release of an entry that is
  running on another worker is deferred through the issue 1705 path (the
  running mark is the slot tag `begin_dispatch` sets) and reaped by the next
  locked step; on an entry that is not running it applies at once. Each
  timer belongs to one band, and only that band's worker advances it, with
  that worker's own elapsed time, so a timer is never credited twice.
  Each worker parks on its own wake object until the earliest deadline in
  its band (RFC-0106, the park already on the P6 `Worker`). One worker, the
  least urgent level (which owns the session), drives the transport.
- **Why it is sound.** All shared mutable executor state is behind one
  lock, and a `&mut` to it exists only inside a guard. A callback touches
  only its own entry's pinned arena data, through a raw pointer, and an
  entry marked running is neither freed nor moved. The running mark means
  two workers never run one callback at once; one worker per band makes a
  band non-preemptive, and preemption across bands is the kernel's. A
  worker picks only from its own band, so a less urgent worker never takes
  work that a more urgent band has pending.
- **As landed in item 2.** The pass is `Executor::pass` over a raw
  pointer; `spin_once(&mut self)` runs it owned (the lock, if any, held
  throughout), `spin_once_shared(*mut Executor, timeout)` runs the
  executor's own worker shared, and `spin_band_once(*mut Executor, &mut
  BandWorker)` runs one band. A shared or band pass opens a window around
  each user callback only when the lock carries the port's thread identity
  (`ExecLock::with_thread_id`, `k_current_get` / `pthread_self`); without
  one it holds the lock as in item 1. Across a window the pass keeps only a
  copy of the entry's meta and its arena address, and re-derives its
  `&mut` after. The running mark is the slot tag `begin_dispatch` sets,
  which every scan, sweep, drain, re-probe and timer-header walk now
  treats as held; a `Running` record in the pass's frame names the thread
  in the window. An operation on that entry from another thread: a timer
  cancel or reset is recorded in the tag (`CANCEL_PENDING`,
  `RESET_PENDING`; the later wins) and applied when the callback returns;
  a release is deferred as before (issue 1705); every other accessor
  answers as for a stale handle, and the caller may retry. The pass's band
  is the worker's (`Executor::set_worker_band`, every level by default),
  and its slots are the only ones it scans, takes from the ready word,
  re-checks, sweeps and drains; another band's arrival bits stay in the
  word. Item 2 leaves the park, the transport drive and the executor-wide
  chores (services, component ticks, the rules) to the executor's own
  worker, under the lock; a band pass does not park. A band worker
  credits its timers with the time since its own previous pass.
- **Order of landing.** (1) the lock seam: the lock, the guards on every
  FFI path, `nros_cpp_executor_set_lock`, one worker, no behaviour change;
  (2) the pass in steps: lock released around callbacks, running mark,
  deferred cancel, band-filtered pick, `set_reprobe_each_callback` (once per
  pass by default); (3) scheduler-owned accounting: per-band timers and
  budgets, one transport driver, per-worker band park; (4) the runner, FFI
  and cost report: `level_open` opens a worker over the core's executor.

## Relation to other RFCs

- **RFC-0002 (RT execution model).** Amended. Its single-thread, registration-
  order model becomes rank order (D2); its multi-executor answer to
  preemption becomes levels (D3). Its SchedClass family (Fifo, Sporadic, Edf,
  TimeTriggered) stays as a mechanism. The 110.F OS-priority worker pool is
  retired once the scheduler/worker split lands (alternative 4).
- **RFC-0079 (priority is allocated).** Unchanged as the source of the rank
  and the address plan. What changes is what a rank is realised as: today a
  rank becomes a thread only through a tier; here a rank is the ordering key
  and a band of ranks becomes a level. The pool width still bounds N. The
  derived-only fields it lists (`preempt_threshold`, `time_slice_us`) get
  their derivation here: the threshold is the most urgent rank of the band,
  and time slicing is off for every level.
- **RFC-0106 (executor park).** Unchanged mechanism; the park moves from the
  executor to the worker (D7), and the session fan-out narrows to one
  worker per arrival where readiness is attributed.
- **RFC-0107 (shared executor core).** **Amended, not superseded.**
  - Steps 1-3 (arena floor, launch remaps in flash, extra sessions; merged)
    are unaffected.
  - Its classification table and concurrency rules stand.
  - **Step 4 is amended:** the per-tier `Dispatcher` splits into a per-core
    `Scheduler` (ready set, callback slots, release and budget accounting) and
    a per-level `Worker` (thread, stack, current dispatch, park state). With
    one level the pair is RFC-0107's dispatcher.
  - **Steps 5 and 6 are amended:** the runners spawn levels instead of tiers,
    and the FFI is `nros_cpp_scheduler_open(core)` plus
    `nros_cpp_level_open(scheduler, level)` instead of
    `nros_cpp_dispatcher_open(core, tier)`. Generated sizes become
    `NROS_CPP_CORE_BACKING_SIZE`, `NROS_CPP_SCHEDULER_BACKING_SIZE_<core>` and
    a per-level stack.
  - When RFC-0107 lands on `main`, its steps 4-6 get a pointer to this RFC.

## Alternatives considered

1. **A whole executor per tier (today).** It works and is what the island
   measured: +7,096 B DTCM [M] per extra tier at 77e4175e77, +14,936 B [M] at
   RFC-0107's baseline, plus a stack. RFC-0107 reduces the DTCM part to a
   dispatcher, but each executor keeps its own ready set, so the order across
   tiers is the kernel's and the order inside is registration order (section
   1). Rejected as the unit of preemption; kept as the shape of an authored
   tier until it migrates.
2. **Work stealing** (per-worker deques, idle workers steal). It optimises
   throughput. It loses the global priority order: an urgent item can sit in a
   busy worker's deque while another worker runs less urgent stolen work, a
   cross-queue inversion no per-queue analysis sees. Placement depends on
   timing, so the per-core analysis breaks. On one core the levels are
   preemption levels, not parallel workers, so there is nothing to steal.
   Allowed later only for an explicitly best-effort class on spare cores.
3. **One global queue across cores.** Simple and work-conserving, but global
   fixed-priority and EDF schedulability is weak (Dhall and Liu 1978), every
   job may migrate, and the shared queue needs a cross-core lock on every
   pick. Rejected for analysed work (D5).
4. **One thread per callback.** Maximal preemption and the simplest mapping.
   It costs a stack per callback (stacks are SRAM, the island's binding
   constraint), more priorities than the pool has (Katcher, Sathaye and
   Strosnider 1995 analyse the loss when priorities must be shared), and a
   context switch per release. The 110.F OS-priority pool
   (`executor/os_priority.rs:1-40`, at most `MAX_PRIORITY_LEVELS` = 8 workers,
   `:83`) is this design as an opt-in feature (`scheduler-os-priority`); levels
   generalise it with a derived N. **Decided:** the pool
   (`executor/os_priority.rs`, `executor/ready_set/os_priority.rs`, the
   feature, and its clippy lane) is retired once the scheduler/worker split
   (P6) lands, so two worker mechanisms never coexist. **Done in P11:** the
   pool, its feature, its hand-off protocol and its tests are gone;
   `SchedContext::os_pri` stays as a recorded field (the C/C++ configs
   carry it) that nothing dispatches by.
5. **Grouping per rate-monotonic class or per chain.** Per rate class drops
   the chain-aware refinement the ranker added and can invert the order
   against the rank; per chain forces one priority on members the ranker
   deliberately spread, and a node in two chains has no home. Per distinct rank
   keeps exactly the ranker's order.
6. **General per-callback preemption thresholds now.** They express
   overlapping non-preemptive groups that bands cannot (D3's worked example),
   but they cost a priority raise and restore on every dispatch and a worker
   pool sized by the deepest preemption chain, and no contract can state the
   relations that would make an overlapping group necessary. Deferred as a
   backward-compatible extension with named triggers (D3).
7. **A byte budget per memory region as the level cap.** Rejected: bytes are
   known only after a target compile. The per-region cost report checks the
   result instead (D4).

## Phased plan

Each nano-ros step is its own PR with its own tests; the work items, files
and dependencies are in
[phase-509](../roadmap/phase-509-executor-preemption-levels.md). Steps P1-P5
do not depend on RFC-0107 steps 4-6; P6 onward do.

1. **P1. Report every member that loses its rank.** `derive.rs:631-649` emits
   one degradation per disagreeing member. Test: an authored tier with members
   at 5, 7 and 8 yields two lines.
2. **P2. Rank-ordered ready set, one thread.** Codegen seeds a rank per
   node (normalised, 0 most urgent) before the node is created; `nros-node` replaces the three-bucket FIFO with a multi-level
   FIFO (bitmap of up to 32 ranks) and maps the C/C++ `Priority` to three
   reserved ranks. Tests: three callbacks registered in reverse rank order and
   all ready dispatch in rank order; equal ranks keep registration order; an
   image with one rank is byte-identical in dispatch order; C/C++ API tests
   unchanged; entry goldens re-blessed.
3. **P3. Re-pick after every callback.** Pushed readiness and due timers join
   the ready set mid-pass. Test: callback A makes a more urgent B ready; B runs
   before a less urgent C already pending in the same pass. Measures the cost
   of re-probing unattributed slots (XRCE, uORB) after every callback, which
   settles the deferred re-probe question.
4. **P4. The task record.** `nros-orchestration-ir` carries (kind, T or
   T_min, D, C, server Q/P) per callback from the contract; the `unanalysable`
   degradation, and the refusal on a safety image. rlm PR for
   `min_interarrival` and `budget` on inputs, a tag, and a pin move (the rlm
   push and release are a separate approval). Tests: each release kind from a
   fixture contract; an undeclared event input is reported by name, and
   refused by name when the contract supervises a deadline.
5. **P5. Level derivation and budget.** Rule 4 becomes D4: per-rank default,
   the C-based merge, the capped merge, `level-budget` degradations, the
   Kconfig knob, and the priority record's new `levels` field. Tests: two
   equal-rank nodes and one other give 2 levels; cap 2 over 3 ranks merges
   the least urgent pair; a protected band is skipped; cap 1 reports every
   member; with stated C the merge keeps every deadline in a fixture where
   the naive merge misses one; two equal kernel priorities on a time-slicing
   platform merge into one level.
6. **P6. Scheduler and worker split in `nros-node`** (amended RFC-0107 step 4).
   `Executor` stays the one-level facade. Tests: one-level backing size
   unchanged; a host test with N workers on a deterministic platform mock
   checks that a band-2 arrival preempts a running band-1 callback and that
   band-1 members never preempt each other.
7. **P7. Level identity and liveness API.** `NROS_ENTRY_LEVEL_OF_<node>`, a
   runtime level query, the level index passed to spin hooks, and the
   "every level alive" aggregate. Tests: header golden, FFI unit test.
8. **P8. Refuse blocking calls to a less urgent or equal level** at build
   time. Tests: a sync client to a less urgent server is refused by name; an
   async one passes.
9. **P9. Zephyr runner on levels** (amended RFC-0107 step 5), the FFI, the
   entry packs and the cost report. Tests: a Zephyr QEMU fixture with 3
   levels checks the preemption order; the cost report's numbers equal the
   generated sizes, and the per-level stacks are derived from the members.
10. **P10. The other runners** (Linux, NuttX, FreeRTOS, ThreadX via its native
    threshold), one PR each (amended RFC-0107 step 6). Kernel budget
    enforcement (`SCHED_DEADLINE`, `SCHED_SPORADIC`) follows per platform.
11. **P11. Retire the OS-priority pool** (`scheduler-os-priority`) after P6.

### Island adoption and board check

1. **I1. Supervisor safe for N levels, at N = 1.** Route 1 owned by the
   handler's level (P7, or a latch on the handle for which the route watch
   succeeded until P7 lands); a per-level SWT0 check-in mask; a spin-gap
   statistic per level; `prio_take` records every level. Board: every
   planning act PASS on the unchanged image.
2. **I2. Pin bump to P1-P3.** Still one thread; dispatch is now in rank order.
   Board: acts PASS; measure the handler's tick jitter against 14.84 ms [M].
3. **I3. Pin bump to P5 and P9.** Remove the authored `[tiers.mrm]` and the
   `group_tiers`; expect three levels at 5, 7 and 8, no rule-2 degradation.
   Record DTCM and SRAM [M] in place of this RFC's estimates. **The 2-level
   fallback is decided:** if the margins are thin, `max_levels = 2` gives
   {`mrm_handler`} | {emergency operator, comfortable operator}. This keeps
   the supervised path protected from both operators; it is also what D4
   rule 3 produces, since it merges the least urgent pair and the handler's
   band holds a supervised route. The measured `estop` split (the emergency
   operator alone) was rejected: it protects the unsupervised 30 Hz operator
   and leaves the handler behind the comfortable operator.
4. **I4. Board check:** the island repo's `board-build` and `priority-check` recipes (N levels
   agree), the planning acts (encore, a, b, and the 300 ms overrun encore)
   every one PASS; route 1 judged only from the handler's level; SWT0 fires
   0; stack high-water per level from the thread analyzer; the handler's tick
   jitter compared with I2.

## Related work

- **ROS 2 executor analysis.** Casini, Blass, Luetkebohle and Brandenburg
  (ECRTS 2019) give the first response-time analysis of ROS 2 processing
  chains, under reservation-based scheduling. Tang, Feng, Guan, Jiang, Lv,
  Deng and Yi (RTSS 2020) refine it and show a chain's response time on an
  executor depends on its last callback, which gives a priority assignment.
  Blass, Casini, Bozhko and Brandenburg (RTSS 2021) exploit the default
  executor's starvation freedom and execution-time variance.
- **Priority-driven ROS 2 executors.** Choi, Xiang and Kim (RTAS 2021, PiCAS)
  replace the default executor's order with chain-aware priorities across
  callbacks, nodes and executors. Arafat, Vaidhun, Wilson, Sun and Guo (DAC
  2022) put EDF into the executor with an end-to-end analysis. Sobhani, Choi
  and Kim (RTAS 2023) analyse the multi-threaded executor and add
  priority-driven dispatch to it. Teper, Kuhse, Guenzel, von der Brueggen,
  Howar and Chen (IEEE TCAD 2024, EMSOFT 2024 best paper) show the
  multi-threaded executor can starve callbacks, which invalidates analyses
  that assume starvation freedom. This RFC's ordering (D2) is in the PiCAS
  line; its levels avoid a shared multi-threaded pick on one core.
- **Microcontroller executors.** Staschulat, Luetkebohle and Lange (EMSOFT
  2020, work in progress) introduce the rclc executor with a user-defined
  order and trigger conditions. Lange's callback-group-level executor
  (ROSCon 2018 lightning talk) maps callback groups to separate executors
  on separate threads, which is today's tier.
- **Limited priority levels.** Katcher, Sathaye and Strosnider (IEEE TC 1995)
  bound the schedulability loss when tasks must share fewer priority levels.
- **Preemption thresholds and limited preemption.** Wang and Saksena (RTCSA
  1999) introduce preemption-threshold scheduling and its analysis; Saksena
  and Wang (RTSS 2000) map tasks to fewer threads as non-preemptive groups and
  save stack. Gai et al. (RTSS 2001) minimise RAM with preemption thresholds
  and stack sharing on single and multiprocessor SoCs. Bril, Altmeyer, van den
  Heuvel, Davis and Behnam (Real-Time Systems 2017) give the analysis with
  preemption delays. Buttazzo, Bertogna and Yao (IEEE TII 2013) survey
  limited-preemptive scheduling.
- **Servers.** Sprunt, Sha and Lehoczky (Real-Time Systems 1989) define the
  sporadic server; Abeni and Buttazzo (RTSS 1998) the constant bandwidth
  server, which Linux `SCHED_DEADLINE` implements.
- **Foundations.** Liu and Layland (JACM 1973): RM and EDF on one processor.
  Leung and Whitehead (Performance Evaluation 1982): deadline-monotonic
  priority and the complexity of fixed-priority scheduling. Audsley (York
  YCS-164, 1991): optimal priority assignment. Dhall and Liu (Operations
  Research 1978): the multiprocessor problem behind D5's rejection of global
  scheduling.

### References

1. D. Casini, T. Blass, I. Luetkebohle, B. B. Brandenburg. Response-Time
   Analysis of ROS 2 Processing Chains Under Reservation-Based Scheduling.
   ECRTS 2019, LIPIcs 133, 6:1-6:23.
2. Y. Tang, Z. Feng, N. Guan, X. Jiang, M. Lv, Q. Deng, W. Yi. Response Time
   Analysis and Priority Assignment of Processing Chains on ROS2 Executors.
   RTSS 2020, 231-243.
3. T. Blass, D. Casini, S. Bozhko, B. B. Brandenburg. A ROS 2 Response-Time
   Analysis Exploiting Starvation Freedom and Execution-Time Variance. RTSS
   2021, 41-53.
4. H. Choi, Y. Xiang, H. Kim. PiCAS: New Design of Priority-Driven
   Chain-Aware Scheduling for ROS2. RTAS 2021, 251-263.
5. J. Staschulat, I. Luetkebohle, R. Lange. The rclc Executor:
   Domain-specific deterministic scheduling mechanisms for ROS applications
   on microcontrollers (work in progress). EMSOFT 2020.
6. R. Lange. Mixed Real-Time Criticality with ROS2 -- the Callback-group-level
   Executor. ROSCon 2018 (lightning talk).
7. A. A. Arafat, S. Vaidhun, K. M. Wilson, J. Sun, Z. Guo. Response Time
   Analysis for Dynamic Priority Scheduling in ROS2. DAC 2022.
8. H. Sobhani, H. Choi, H. Kim. Timing Analysis and Priority-driven
   Enhancements of ROS 2 Multi-threaded Executors. RTAS 2023.
9. H. Teper, D. Kuhse, M. Guenzel, G. von der Brueggen, F. Howar, J.-J. Chen.
   Thread Carefully: Preventing Starvation in the ROS 2 Multithreaded
   Executor. IEEE TCAD 2024 (EMSOFT 2024).
10. D. I. Katcher, S. S. Sathaye, J. K. Strosnider. Fixed Priority Scheduling
    with Limited Priority Levels. IEEE Trans. Computers 44(9), 1995,
    1140-1144.
11. Y. Wang, M. Saksena. Scheduling Fixed-Priority Tasks with Preemption
    Threshold. RTCSA 1999.
12. M. Saksena, Y. Wang. Scalable Real-Time System Design using Preemption
    Thresholds. RTSS 2000.
13. P. Gai, G. Lipari, M. Di Natale. Minimizing Memory Utilization of
    Real-Time Task Sets in Single and Multi-Processor Systems-on-a-Chip.
    RTSS 2001.
14. R. J. Bril, S. Altmeyer, M. M. H. P. van den Heuvel, R. I. Davis,
    M. Behnam. Fixed priority scheduling with pre-emption thresholds and
    cache-related pre-emption delays: integrated analysis and evaluation.
    Real-Time Systems 53, 2017.
15. G. C. Buttazzo, M. Bertogna, G. Yao. Limited Preemptive Scheduling for
    Real-Time Systems: A Survey. IEEE Trans. Industrial Informatics 9(1),
    2013, 3-15.
16. B. Sprunt, L. Sha, J. Lehoczky. Aperiodic Task Scheduling for
    Hard-Real-Time Systems. Real-Time Systems 1(1), 1989.
17. L. Abeni, G. Buttazzo. Integrating Multimedia Applications in Hard
    Real-Time Systems. RTSS 1998.
18. C. L. Liu, J. W. Layland. Scheduling Algorithms for Multiprogramming in a
    Hard-Real-Time Environment. JACM 20(1), 1973, 46-61.
19. J. Y.-T. Leung, J. Whitehead. On the Complexity of Fixed-Priority
    Scheduling of Periodic, Real-Time Tasks. Performance Evaluation 2(4),
    1982, 237-250.
20. N. C. Audsley. Optimal Priority Assignment and Feasibility of Static
    Priority Tasks With Arbitrary Start Times. Tech. report YCS-164, Univ. of
    York, 1991.
21. S. K. Dhall, C. L. Liu. On a Real-Time Scheduling Problem. Operations
    Research 26(1), 1978, 127-140.

## Decisions (2026-10-21)

The first draft's open questions, as decided:

1. **Level model: contiguous bands** (D3). General per-callback thresholds are
   a later, backward-compatible extension, taken when a contract can state
   non-preemption or shared-state relations between callbacks, or for a
   kernel with native thresholds (ThreadX `tx_thread_preemption_change`).
2. **The level cap is a count**, `CONFIG_NROS_EXECUTOR_MAX_LEVELS` or the
   platform's equivalent knob (D4 rule 4). The per-region byte-cost report is
   a check, never an input (D4 rule 7).
3. **The opt-in OS-priority worker pool** (`executor/os_priority.rs`, feature
   `scheduler-os-priority`) is retired once the scheduler/worker split (P6)
   lands (alternative 4, P11).
4. **Event inputs with no declared rate, minimum inter-arrival or budget** are
   refused on a safety image and reported elsewhere. A safety image is one
   whose contract supervises a deadline (`supervise:`) (D1).
5. **Island 2-level fallback:** {`mrm_handler`} | {emergency operator,
   comfortable operator}, which protects the supervised path from both
   operators (island adoption, I3).
6. **Equal kernel priorities are merged on time-slicing kernels** (FreeRTOS,
   Linux `SCHED_RR`): separate threads at one priority only cost memory. Time
   slicing is off for real-time levels (D4 rule 8, D6).
7. **Worker stacks are derived per level** from the members, never the
   platform's default thread stack (D3).

8. **The response-time analysis lives in `nros-orchestration-ir`**
   (decided in P5, which first needed it): the derivation that consumes it
   runs there and holds the plan, the pins, the task records and the cap.

### Deferred

| question | decided when |
| --- | --- |
| EDF across levels: Zephyr's deadline mode to order levels by deadline, or EDF per level only (D6) | When an image first needs EDF with more than one level; not before P9. Until then EDF orders within a level. |
| Who splits an end-to-end deadline across heterogeneous images: rlm, or each image's derivation from a stated segment budget (D5) | In the load-partitioning phase, which also assigns callbacks to cores. |

## Changelog

- 2026-10-21: Draft.
- 2026-10-21: Open questions decided (contiguous bands, a count cap, retire
  the OS-priority pool, refuse unanalysable inputs on safety images, the
  island fallback, merge equal priorities, derived stacks); the rest
  deferred with a decision point; worked threshold example; P11; tracked by
  phase-509.
- 2026-10-21: P2 built; D2 amended: the rank is seeded per node, not per
  callback group (the derivation ranks nodes).
- 2026-10-21: P4 built; D1 amended with the task record as built: inputs
  are subscriptions and servers, a rate bound is `max_rate_hz` or the topic's
  `rate_hz`, a client response inherits its requests' release, implicit
  deadlines, and where the refusal and the report live (rlm v0.1.68).
- 2026-10-21: P3 built; the deferred re-probe question settled (once per pass
  by default, D2, with the measurement); timers join at pass boundaries.
- 2026-10-21: P5 built; D4 amended with the level derivation as built; the
  deferred analysis-location question decided (`nros-orchestration-ir`).
- 2026-10-21: P8 built; D8.3 amended with the call mode (rlm v0.1.69) and
  what an unstated call does.
- 2026-10-21: P6 built; D7 amended with the scheduler / worker split as built
  (a worker picks only its own band).
- 2026-10-21: P11 built; the OS-priority worker pool is retired (alternative 4).
- 2026-10-21: P7 built; D8 amended with the level identity and liveness as
  built (a per-image check-in registry; no spin-hook ABI).
