# Phase 509 -- Executor preemption levels (RFC-0108, amended RFC-0107 steps 4-6)

**Status (2026-10-21). Planned; nothing started.** The design is
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

- [ ] Goal: rule 2 of tier derivation emits one `tier` degradation per member
      whose rank differs from the tier's, not only the first
      (`derive.rs:631-649`, `members.iter().find(...)`).

**Touches:** `derive.rs` (rule 2, around `:611-680`).

**Acceptance:** unit test: an authored tier with members at ranks 5, 7 and 8
yields two degradations, naming both the 7 and the 8 member. The existing
derive tests stay green. On the island's priority plan this adds a line for
`mrm_comfortable_stop_operator`.

**Depends on:** nothing.

### P2 -- Rank-ordered ready set on one thread

- [ ] Goal: inside one executor, ready callbacks dispatch in rank order
      (0 = most urgent), with equal ranks in registration order.

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

- [ ] Goal: after every callback the dispatcher picks again from the ready set
      as it is now. Pushed readiness (RFC-0084 attributed slots) and due timers
      join the ready set mid-pass.
- [ ] Measure the cost of re-probing unattributed slots (XRCE, uORB) after
      every callback, against probing them once per pass. Record the
      number in RFC-0108 and settle its deferred re-probe question.

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

- [ ] rlm (`ros-launch-manifest`): add `min_interarrival`, and `budget` +
      `budget_period`, on inputs.
- [ ] nano-ros: `nros-orchestration-ir` carries a task record per callback:
      release kind (periodic / sporadic / served / unanalysable), T or T_min,
      D, C, and the server's Q/P. The `unanalysable` degradation; refusal on a
      safety image.
- [ ] Pin bump to the rlm tag.

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

- [ ] Goal: tier-derivation rule 4 becomes RFC-0108 D4:
  - one level per distinct rank;
  - the merge with execution times (C);
  - the capped merge without C, which skips protected bands;
  - one `level-budget` degradation per member that loses its rank;
  - equal kernel priorities merged on time-slicing platforms;
  - the `max_levels` knob (`CONFIG_NROS_EXECUTOR_MAX_LEVELS`, or the
    platform's equivalent) with per-platform defaults;
  - a `levels` field in the priority record.
- [ ] Decide where the response-time analysis lives (`nros-orchestration-ir`
      or rlm), which is deferred to this item by RFC-0108.

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

- [ ] Goal: RFC-0107's per-tier `Dispatcher` splits into a per-core
      `Scheduler` and a per-level `Worker`. The Scheduler holds the ready
      set, callback slots, and release and budget accounting. The Worker
      holds the thread, stack, current dispatch and park state. `Executor`
      stays the one-level facade.

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

- [ ] Goal: the island's supervisor can own a route per level and feed the
      watchdog only when every level is alive (RFC-0108 D8). Adds:
  - a generated `NROS_ENTRY_LEVEL_OF_<node>` next to the priority plan header;
  - a runtime level query on an executor handle;
  - the level index passed to spin hooks;
  - the "every level alive" aggregate from the per-sched-context alive slots.

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

- [ ] Goal: the build refuses a synchronous service call whose server is at a
      level equal to or less urgent than the client's, when the contract
      names both. Asynchronous calls pass.

**Touches:** `derive.rs` (or the check module that reads the contract's
client/server pairs); the degradation and refusal text.

**Acceptance:**

- Unit: a sync client to a less urgent server is refused by name.
- Unit: a sync client to a server at its own level is refused (deadlock).
- Unit: an async client to the same server passes.
- The island's contract passes (its calls are asynchronous).

**Depends on:** P5.

### P9 -- Zephyr runner on levels, per-level stacks, cost report (amended RFC-0107 step 5)

- [ ] Goal: the Zephyr runner spawns levels instead of tiers. The FFI becomes
      `nros_cpp_scheduler_open(core)` + `nros_cpp_level_open(scheduler, level)`,
      replacing `nros_cpp_dispatcher_open(core, tier)`. Generated sizes become
      `NROS_CPP_CORE_BACKING_SIZE`, `NROS_CPP_SCHEDULER_BACKING_SIZE_<core>`
      and a per-level stack.
- [ ] Per-level stacks derived from the members (largest member plus the
      worker frame), never the platform default.
- [ ] The cost report after the target compile: per region and per level its
      stack and worker record; per core its scheduler and backing; text and
      JSON. A check, never an input.

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

- [ ] Goal: remove the 110.F opt-in pool now that levels cover it (RFC-0108
      alternative 4, decision 3).

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
