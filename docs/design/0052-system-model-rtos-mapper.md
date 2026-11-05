---
rfc: 0052
title: "SystemModel → RTOS primitives — the execution/contract mapper for embedded consumption"
status: Draft
since: 2026-07
last-reviewed: 2026-11-05
implements-tracked-by: [phase-296]
supersedes: []
superseded-by: null
---

# RFC-0052 — SystemModel → RTOS primitives

## Summary

RFC-0050 defined the SystemModel; play_launch phase 43 made it real (the
Linux runtime consumes it end-to-end). This RFC defines the **nano-ros
half**: how the model's execution layer (sched tasks, bindings, deploy) and
contract layer (rates, ages, budgets, QoS) map onto the RTOS primitives a
baked image actually has — task priorities, stacks, core pinning,
preemption thresholds, executor scheduling contexts, and on-target
monitors. One mapping table, applied uniformly across POSIX, FreeRTOS,
Zephyr, ThreadX, and NuttX, landing on the seams that already exist
(RFC-0015 sched tasks, RFC-0016 priority normalization, RFC-0047 sched-context
binding, RFC-0045 boot config, RFC-0048 CMake verbs).

## Names (decided 2026-11-05, phase 536 W0)

rlm and play_launch call the scheduling unit a **task** (`[tasks.<name>]`,
`group_tasks`, model `execution.tasks`; play_launch phase 99). nano-ros
follows with one word for its own names: a scheduling tier is a **sched
task** (`sched_task` / `SCHED_TASK` / `SchedTask`). Not bare "task": every
RTOS port already has one (FreeRTOS/ThreadX's boot-time app task,
`app_task_entry`, `NROS_FREERTOS_APP_STACK_KB`, `create_app_task`), and "tier
task" collapsed onto it exactly (`tier_task_entry` would have become
`app_task_entry`'s neighbour `task_entry`). The app task names stay as they
are. Where nano-ros mirrors an rlm key or field it keeps rlm's spelling
(`tasks`, `group_tasks`).

### The rule

Every own name moves by one substitution, applied in this order:

| Was | Now |
|---|---|
| `tier_task` / `TierTask` / `TIER_TASK` ("tier task") | `sched_task` / `SchedTask` / `SCHED_TASK` ("sched task"), never "sched task task" |
| `tiers` / `Tiers` / `TIERS` | `sched_tasks` / `SchedTasks` / `SCHED_TASKS` |
| `tier` / `Tier` / `TIER` | `sched_task` / `SchedTask` / `SCHED_TASK` |
| `tiered` (in a name: `c_tiered_plan`) | `sched_task` (`c_sched_task_plan`) |
| `untiered`, `tierless` | `single_executor` (an image with no sched task table runs one executor) |
| hyphenated names (`derived-tiers-cpp`, `check-tier-spin-gap`) | `sched-task(s)` (`derived-sched-tasks-cpp`, `check-sched-task-spin-gap`) |
| prose "tier" / "tiers" / "tiered entry" | "sched task" / "sched tasks" / "sched-task entry" |

Two names do not follow the rule: `nros_board_{freertos,zephyr}_run_tiers_tasks_in`
(whose `tasks` meant the task memory) is `..._run_sched_tasks_memory_in`, and
the nros-cpp struct field `tier_class` is `sched_task_class`.

Before the rename every target name was checked: `sched_task`, `SCHED_TASK`
and `SchedTask` had no occurrence on `main` (2026-11-05), so no new name
collides with an old one.

### C ABI (public headers), one release of aliases

| Was | Now |
|---|---|
| `nros_native_tier_spec_t` | `nros_native_sched_task_spec_t` |
| `nros_tier_task_memory_t` | `nros_sched_task_memory_t` |
| `nros_tier_backing_t` | `nros_sched_task_backing_t` |
| `nros_board_{native,freertos,zephyr,nuttx}_run_tiers{,_ns,_in}` | `nros_board_{...}_run_sched_tasks{,_ns,_in}` |
| `nros_board_{freertos,zephyr}_run_tiers_tasks_in` | `nros_board_{freertos,zephyr}_run_sched_tasks_memory_in` |
| `nros_board_zephyr_run_tiers_backed_in` | `nros_board_zephyr_run_sched_tasks_backed_in` |
| `nros_native_tier_spec_t::tier_class` | `sched_task_class` (no alias: a C99 struct field cannot carry one) |
| `<rclc/detail/tier_task_memory_{zephyr,freertos,nuttx}.h>` | `<rclc/detail/sched_task_memory_{...}.h>` |
| `NROS_TIER_TASK_MEMORY_DEFINE`, `NROS_TIER_TASK_MEMORY`, `NROS_TIER_TASK_MEMORY_NONE` | `NROS_SCHED_TASK_MEMORY_DEFINE`, `NROS_SCHED_TASK_MEMORY`, `NROS_SCHED_TASK_MEMORY_NONE` |
| `NROS_TIER_TASK_STACK_{DEFAULT,BYTES,DEFINE,WORDS}_` (internal) | `NROS_SCHED_TASK_STACK_{...}_` |
| `NROS_CPP_TIER_BACKING_SIZE_<i>`, `NROS_CPP_TIER_COUNT` (per-build header) | `NROS_CPP_SCHED_TASK_BACKING_SIZE_<i>`, `NROS_CPP_SCHED_TASK_COUNT` |
| nros-cpp FFI `nros_cpp_init_tier`, `nros_cpp_tier_level`, `nros_cpp_tier_backing_size`, `nros_cpp_executor_open_over_session_tier` (internal to the C++ headers) | `nros_cpp_init_sched_task`, `nros_cpp_sched_task_level`, `nros_cpp_sched_task_backing_size`, `nros_cpp_executor_open_over_session_sched_task` |
| board-internal mirrors `nros_tier_spec_t`, `nros_tier_setup_fn_t`, `nros_{zephyr,freertos,nuttx}_tier_ctx_t`, `nros_zephyr_tier_task_create{,_static,_stack}`, `nros_zephyr_tier_thread_start`, `nros_tier_stacks`, `nros_tier_index` | `nros_sched_task_spec_t`, `nros_sched_task_setup_fn_t`, `nros_{...}_sched_task_ctx_t`, `nros_zephyr_sched_task_create{,_static,_stack}`, `nros_zephyr_sched_task_thread_start`, `nros_sched_task_stacks`, `nros_sched_task_index` |

The public old names (the types, the run functions, the three headers and
their three macros) stay one release: `typedef`s and macros that warn with
the new name (`NROS_NO_DEPRECATED_TIER_NAMES` drops them early), forwarding
headers that `#warning`. Struct layouts do not change.

### C++

| Was | Now |
|---|---|
| `rclcpp::board::NativeTierSpec` | `NativeSchedTaskSpec` (deprecated alias one release) |
| `<Board>::run_tiers(...)` (Linux, Zephyr, NuttX, FreeRTOS boards) | `run_sched_tasks(...)` (deprecated forwarder one release) |
| `nros_px4::TierWorkItem`, `<nros_px4/tier_work_item.hpp>` | `SchedTaskWorkItem`, `<nros_px4/sched_task_work_item.hpp>` (deprecated alias and forwarding header one release) |

### Kconfig, one release of aliases

| Was | Now |
|---|---|
| `NROS_ZEPHYR_MAX_TIERS` | `NROS_ZEPHYR_MAX_SCHED_TASKS` |
| `NROS_ZEPHYR_TIER_STACK_SIZE` | `NROS_ZEPHYR_SCHED_TASK_STACK_SIZE` |
| `NROS_ZEPHYR_TIER_STACKS_IN_DTCM` | `NROS_ZEPHYR_SCHED_TASK_STACKS_IN_DTCM` |

The old symbols stay one release as deprecated symbols whose value feeds the
new ones' defaults; the configure warns, naming the new symbol, when a
`.conf` file sets one.

### Rust (no aliases: no nano-ros crate is on crates.io)

`TierSpec` `SchedTaskSpec`, `NativeTierSpecC` `NativeSchedTaskSpecC`,
`NativeTierBackingC` `NativeSchedTaskBackingC`, `TierExecutorBacking{,Slot}`
`SchedTaskExecutorBacking{,Slot}`, `TierBackingShort`
`SchedTaskBackingShort`, `TierTaskMemory{,Raw,Set}` `SchedTaskMemory{,Raw,Set}`,
`TIER_TASK_TCB_U64S` `SCHED_TASK_TCB_U64S`, `TIER_BACKING_SIZE_<i>`
`SCHED_TASK_BACKING_SIZE_<i>`, `TierSetup{,Gate,Guard}`
`SchedTaskSetup{,Gate,Guard}`, `TierSpinGap` `SchedTaskSpinGap`,
`RealtimeTiers` `RealtimeSchedTasks`, `BootTier` `BootSchedTask`,
`tier_level{,_by}` `sched_task_level{,_by}`, `SchedContext::from_tier_policy`
`from_sched_task_policy`, `apply_tier_sched_policy`
`apply_sched_task_sched_policy`, `DeadlineAction::{from,as}_tier_str`
`{from,as}_sched_task_str`, module `nros_platform::board::tier`
`board::sched_task`; `nros-orchestration-ir`'s `TierDef` `SchedTaskDef`,
`TierRtosSpec` `SchedTaskRtosSpec`, `ResolvedTier{,Table}`
`ResolvedSchedTask{,Table}`, `DEFAULT_TIER` `DEFAULT_SCHED_TASK`,
`TierResolveError` `SchedTaskResolveError`, `resolve_tiers`
`resolve_sched_tasks`, `resolve_system_tiers` `resolve_system_sched_tasks`,
`derive_tiers_*` `derive_sched_tasks_*`, `rtos_plan_to_tier_table`
`rtos_plan_to_sched_task_table`, `is_{single,multi}_tier`
`is_{single,multi}_sched_task`, `ModeTier{,Deadlines}`
`ModeSchedTask{,Deadlines}`, `LoweredTiers` `LoweredSchedTasks`; the CLI's
`orchestration::tier_resolver` `sched_task_resolver`, `Px4TooManyTiers`
`Px4TooManySchedTasks`, `TierSizing`/`TIER_SIZINGS`
`SchedTaskSizing`/`SCHED_TASK_SIZINGS`; and every other item, local and test
name by the rule.

### Generated code and records

| Was | Now |
|---|---|
| `__nros_tiers`, `__nros_tier_executor_storage`, `__nros_tier_backings`, `__nros_tier_task_memory`, `__nros_tier_{stack,tcb,thread,backing}_<k>` | `__nros_sched_tasks`, `__nros_sched_task_executor_storage`, ... |
| `__nros_entry_setup_tier_<k>[_nodes]`, `__nros_tier_<k>_groups`, `__NROS_TIER_EXECUTOR_BACKING` | `__nros_entry_setup_sched_task_<k>[_nodes]`, `__nros_sched_task_<k>_groups`, `__NROS_SCHED_TASK_EXECUTOR_BACKING` |
| `NROS_ENTRY_TIERS_BACKED`, `NROS_ENTRY_TIER_COUNT`, `NROS_ENTRY_TIERS_MOST_URGENT_PRIORITY`, `NROS_ENTRY_TIER_<NAME>_PRIORITY` | `NROS_ENTRY_SCHED_TASKS_BACKED`, `NROS_ENTRY_SCHED_TASK_COUNT`, ... |
| sizing descriptor `--tier-record`, priority plan and record keys `tiers` / `tier` | `--sched-task-record`, `sched_tasks` / `sched_task` |

### Names on disk

| Was | Now |
|---|---|
| example workspace `derived-tiers-cpp` | `derived-sched-tasks-cpp` |
| fixtures `orchestration_tiers_{native,freertos}`, cases `orch_tiers_*` | `orchestration_sched_tasks_*`, `orch_sched_tasks_*` |
| tests `realtime_tiers_*`, `system_toml_tiers_parse`, `linux_orchestration_tiers`, `orchestration_tiers_freertos`, `tier_filter`, `tier_wake_fanout`, `derived_tiers_{bake,entry,precedence}` | `realtime_sched_tasks_*`, ... by the rule |
| goldens `*_tiers.*.golden`, `*_default_tier.*.golden` | `*_sched_tasks.*.golden`, `*_default_sched_task.*.golden` |
| `freertos_run_tiers.c`, `zephyr_run_tiers.c`, `nuttx_run_tiers.c`, `entry_tiers.rs`, `board/tier.rs`, `tier_resolver.rs` | `*_run_sched_tasks.c`, `entry_sched_tasks.rs`, `board/sched_task.rs`, `sched_task_resolver.rs` |
| bin `tier-stacks-dtcm-zephyr-mps2` | `sched-task-stacks-dtcm-zephyr-mps2` |
| gates `check-tier-priority-plan{,-image}`, `check-tier-spin-gap`, `scripts/dev/measure-tier-priority-cell.sh` | `check-sched-task-priority-plan{,-image}`, `check-sched-task-spin-gap`, `measure-sched-task-priority-cell.sh` |

### RFC-0108's per-callback record

With the unit called a task, RFC-0108's per-callback "task record" (a
callback's release kind, T, D, C) would read as the renamed unit. It is the
**callback record**: module `nros_orchestration_ir::callback_record`,
`CallbackRecord`, `callback_records(..)`; the derived schedule's and the
level input's `tasks` field is `callback_records`. That frees the bare word
"task" for the scheduling unit rlm names.

### Kept: the other senses of "tier"

| Sense | Names | Why it stays |
|---|---|---|
| CI and test tiers (RFC-0061) | `CiTier`, `Tier1`/`Tier2`/`Tier2N`/`Tier3`, `just ci tier1`, `lane=tier1\|tier2\|tier2-nightly`, `check-tier-has-ci-owner.py`, `check-tier-preconditions.sh`, `NROS_SKIP_TIER_PRECONDITIONS`, `scripts/ci/tier-health.py`, `BucketTier`/`BUCKET_TIERS`, the coverage `Tier` (`matrix.rs`), `isotp-zephyr-tier2.sh` | a rank of test cadence, which is what "tier" means in English; not the scheduling unit |
| Board and framework support tiers (RFC-0112, RFC-0114) | `check-board-tiers.py`, "Tier 1 framework", the board registry's support levels | a rank of support |
| Compile, build and SDK tiers | "compile tier", `sdk-tiers.md`, the build tier of a fixture | a rank of how far a target is built |
| CLI command tiers (RFC-0117) | `CommandTier`, `user`/`dev` tiers, `cli-dev-tier.md`, `tier = "dev"` in an extension descriptor | a rank of command audience |
| C API tiers | `polling_tier_*`, `callback_tier_*`, `action_callback_tier` | a rank of API shape (polling, callback) |
| API surface tiers | prelude tiers (`check-prelude-tiers.py`), entity-API tiers (RFC-0022), heap-free / unified tiers (`--tier heap-free`, `check-no-alloc-image.py`) | a rank of API or allocation discipline |
| Name resolution | `RemapTier` (launch rule before `--ros-args` rule) | the layer a remap rule belongs to |
| Rust target tiers | "Tier 3 target, needs nightly" | rustc's own term |
| TIER IV | `tier4_*` packages | a company's package names |
| The retired keys | `[tiers.*]`, `group_tiers`, `RETIRED_GROUP_TIERS`, `refuse_retired_tiers` | they name the spelling being refused |
| History | `archived/`, closed issues, `CHANGELOG.md`, existing `changelog.d` fragments | the words of their time |

## Baseline facts (2026-07 exploration)

- `system.toml` sched tasks are resolved by `nros-orchestration-ir::resolve_sched_tasks`
  (the CLI/`nros::main!` shared SSoT) and narrowed to the runtime
  `SchedTaskSpec { name, groups, priority, stack_bytes, spin_period_us }`
  (`packages/platform/nros-platform/src/board/sched_task.rs`).
- **Lossy narrowing**: `core`, `preempt_threshold`, `sched_class`, `class`,
  `period_us`, `budget_us`, `deadline_us`, `deadline_policy` die at the
  codegen boundary (`codegen_system.rs::PlanSchedTaskDoc`); FreeRTOS's
  `emit_cpp` additionally drops `stack_bytes` (documented in
  `freertos_run_sched_tasks.c`).
- The executor already HAS sporadic-budget ticking and time-triggered
  windows (`sched_context.rs::tick`, `tt_window_offset_us`) — but only via
  the programmatic API, never fed from sched task tables.
- There is **no message-age concept** in `nros-node`: the take path
  (`handles.rs::take`) never extracts `header.stamp`; no rate/jitter
  accounting exists.
- The vendored `ros-launch-manifest` `model`/`sched` crates define
  `SchedTaskDef`/`SchedTaskPlatformSpec` that duplicate `nros-orchestration-ir`'s
  `SchedTaskDef` — two hand-mirrored sched task schemas (the FFI-mirror-drift class,
  cross-repo).

## Model ingestion (build side)

`nros dev codegen system` gains a model mode: `--model system_model.yaml`
(mutually exclusive with the launch/system.toml pair). Per RFC-0050 the
model is checked-by-construction; ingestion is selection + mapping, not
re-validation:

1. **Slice selection**: `execution.deploy` picks this image's nodes —
   entries with `target: mcu:<board>` matching the build's board (from
   `nano_ros_use_board` / `package.xml <export><nano_ros board=…>`). A
   model with no `deploy` section and exactly one candidate node set is
   accepted with a note; ambiguity is a hard error.
2. **Sched tasks + bindings** → `ResolvedSchedTaskTable` (existing sched_task_resolver
   path) → `nros-plan.json` + the `run_sched_tasks` const table. Bindings keys
   are node FQNs or `FQN/callback_group` — exactly RFC-0047's binding
   granularity.
3. **Boot config** → RFC-0045 baked rung: domain/locator from
   `execution.deploy[node]` (falling back to `meta`-level system config),
   emitted into `system_config.h` like today's `codegen_system` output.
4. **Wiring** → topic names for the node's endpoints (layer 1), replacing
   what the launch XML carries today.
5. **Contracts** → a generated per-node `const` monitor table (see
   "On-target monitors"), plus QoS onto endpoint config.

The vendored `model` crate is the parser — no schema re-declaration in
nano-ros. **Schema unification**: `nros-orchestration-ir::SchedTaskDef` gains
`From<ros_launch_manifest_sched::TaskDef>` (explicit conversion, not type
replacement — orchestration-ir must stay `no_std`-friendly for the
proc-macro; drift is caught by a round-trip test over every field, the
FFI-mirror lesson applied).

## The mapper

### Execution layer → task/scheduling primitives

| Model field | POSIX | FreeRTOS | Zephyr | ThreadX | NuttX |
|---|---|---|---|---|---|
| `priority` (+ `sched_class`) | `pthread_setschedparam` SCHED_FIFO/RR (upgrade from today's advisory-only) | `xTaskCreate` priority (RFC-0016 normalize) | `k_thread` priority (negative coop admitted) | `tx_thread_create` priority | pthread SCHED_FIFO priority |
| `stack_bytes` | `pthread_attr_setstacksize` | task stack words (**fix the emit_cpp drop**) | `K_THREAD_STACK` size | thread stack size | `pthread_attr_setstacksize` |
| `core` | `pthread_setaffinity_np` | `vTaskCoreAffinitySet` (SMP builds; ignore+note on UP) | `k_thread_cpu_pin` | SMP core exclusion mask | `pthread_attr_setaffinity` |
| `preempt_threshold` | reject (validate-time error) | reject | reject | `tx_thread_preemption_change` | reject |
| `spin_period_us` | sched task spin loop (exists) | exists | exists | exists | exists |

`spin_period_us` is an UPPER bound on one spin's wait, never a rate: a spin
returns as soon as it has something to do. Issue 0636 option 3 adds the missing
lower half — **a sched task loop may not run for longer than
`max(spin_period_us, 10 ms)` without either blocking in its own spin or taking a
1 ms scheduled gap.** Both halves are derived from the declared period, so there
is no knob; the gap costs nothing on a loop whose spins block, and its 1 ms
against a 10 ms floor is what caps the worst case at 10 %.

Why it is a mapper concern rather than an executor one: whether a spin blocks is
a property of the TRANSPORT. `Executor::spin_once` drives I/O with a zero
timeout whenever a wake already fired, so under sustained arrival the loop never
blocks — precisely when the system is busiest — and on a uniprocessor under
fixed-priority preemption a thread that never blocks never lets a lower-priority
sched task run. Choosing the least urgent sched task as session owner (`boot_sched_task_index`)
removes the starvation; the gap is what makes the guarantee independent of that
ordering being right. One implementation,
`nros_platform::board::sched_task::SchedTaskSpinGap` / `nros_sched_task_spin_gap_step`, for the
Rust and C sched task runners alike; gate: `check-sched-task-spin-gap`.

Rejection semantics: a platform-inapplicable field in the SELECTED
target's sub-table is a **bake-time error** (same philosophy as
play_launch 43.3's missing-sub-table hard error): the integrator wrote a
knob the platform cannot honor; silently ignoring it is the domain-0
class of bug. Fields in OTHER platforms' sub-tables are ignored (one
model serves all targets).

### `class` → executor scheduling mode

| `class` | Mapping |
|---|---|
| `best_effort` (default) | plain sched task; no deadline machinery |
| `real_time` | fixed-priority preemptive sched task + deadline monitor when `deadline_us` set |
| `time_triggered` | executor TT window (`tt_window_offset_us` — exists): `period_us` = window period, sched task spin aligned; requires `period_us` |
| `interrupt` | out of scope v1 — bake-time reject (ISR-context executors are their own RFC) |

`budget_us` + `period_us` → the existing sporadic-budget `SchedContext`
(`tick`), fed from the sched task table instead of only the programmatic API.

`deadline_policy` → monitor action: `ignore` (no monitor), `warn`
(diagnostics entry), `skip` (executor skips the group's remaining
callbacks this cycle — `set_active_groups` seam), `fault` (platform fault
hook: `nros_fault()` → board-defined, defaults to panic).

### Contract layer → on-target monitors

Generated per-node `const` table (no heap, no YAML on target):

| Contract | Monitor | Seam |
|---|---|---|
| sub `max_age_ms` | `now - header.stamp` at take; requires the **new** stamp extraction: CDR peek of the leading `std_msgs/Header` when the type has one (codegen knows; non-stamped types get no age monitor) | take path (`handles.rs::take`) |
| pub `min_rate_hz` / `jitter_ms` | per-endpoint publish counter + EWMA period, checked per spin tick (~the play_launch 5 s cadence, scaled to `spin_period_us`) | publish path + spin loop |
| node path `max_latency_ms` | take-timestamp → publish-timestamp delta for the declared (input, output) pair | executor sched-context (already threads a monotonic clock) |
| topic QoS | baked endpoint QoS config (exists — RFC-0006 axes) | codegen |
| scope paths / drops | NOT monitored on-target v1 — cross-machine E2E is the Linux side's job (`max_age_ms` at the final subscriber catches the total, per RFC-0050) | — |
| `hazards` (guards, `ftti_ms`, reaction `safe_state`) | phase-462 W3b: an observer over the rows above, no detector of its own -- a guard topic has faulted when its age row's `silence-runtime` lease is reported or its rate row's window was short. Reports `hazard-detected` / `hazard-reaction` (the `safe_state.emits` publisher's counter advanced) / `hazard-recovered`, in play_launch's spelling. Lowered only onto an executor whose tables hold every monitor of the hazard's guards. Build time (W3a): detection + reaction + release over `ftti_ms` refuses. W3c: on detection, a reacting path that states a `safe_state` is started through the phase-503 reaction hook, by its model-wide reaction id; the hook's event carries `fired_at_us`, the detection tick. Issue 2173 (play_launch phase 96 W4.2): the model's `rungs` give each rung of the hazard's ladder the ROUTE play_launch's checker charged it (detection fire to the rung's first output, excluding the detection time, any window and the settle). The image bakes every rung (the mode id it is, the rate row observing its output, its route rounded up, its role, and the guard members that belong to its `starts_at` node) and judges the ACTIVE one: at detection, the rung whose mode the executor already judges by (the ladder fell before this fault), else the top rung; while the hazard holds, the executor entering a lower rung's mode is a fall, and that rung becomes active. A rung's route clock starts at its `starts_at` node's detection when that node is one of the guard's detectors on this executor, else when the rung becomes active (a rung below a window starts at the window owner, which is no guard detector, so at the fall). It reports `hazard-route-runtime` (measured: clock start to the first publish on the rung's output, or to the tick that saw the route pass with none; declared: the rung's `route_ms`) at most once per rung that becomes active, nothing when it answers in time; a rung with no route or no observed output is followed but not judged. The active mode is the one the executor applied (`request_mode`), so a mode change the application never requests is not seen | spin loop, after the rows |
| service client `call` (rlm v0.1.69) | phase-509 P8: a stated `blocking` call to a level that cannot preempt the client is refused at build; an unstated one is reported (`blocking_calls`). Read on all three bake roads | build time |
| service client `timeout` / `on_timeout` / `on_rejected` / `max_transport`; server `idempotent` / `stale_after` (rlm v0.2.5, play_launch phase 95) | NOTHING. No deadline, monitor, retry, refusal or QoS is derived from them on the image (play_launch phase 95 decision 4, agreed in its phase 96 W4.3). They are facts about the code. A client's timeout and fallback are what its own callback does, and a server that honours `idempotent` or `stale_after` does so in its own callback. play_launch charges them in its fault-reaction arithmetic and observes unanswered calls itself. A server can honour `stale_after` only if it sees when the request was sent: `ServiceInfo.source_timestamp` (issue 1942), which zenoh and Cyclone DDS fill and XRCE-DDS does not. On XRCE such a server cannot tell a stale request from a fresh one. Issue 2174: each backend states this in its `nros-rmw.toml` as `[rmw.facts] request_source_timestamp = true\|false` (XRCE-DDS `false`, zenoh and Cyclone DDS `true`; a backend that says nothing is unknown, never false), and the codegen-system and C/C++ entry roads WARN when a server this image runs declares `stale_after` on a backend that states `false` (`orchestration::service_facts`). The entry road knows the backend only from the model's `execution.deploy.<fqn>.rmw`; `nros::main!` does not check | build time (warning) |

Violations surface through the existing diagnostics path
(`nros-diagnostic-updater`), one entry per rule id, mirroring
play_launch's severity vocabulary — the two runtimes report the same
contract in the same words. The assumption/guarantee split (sub = assume,
pub/path = guarantee) rides into the diagnostic payload for the 4-quadrant
diagnosis RFC-0050 describes.

Cost discipline: every monitor is compile-time-gated by the presence of
its contract field in the model — an uncontracted image bakes zero
monitor code (`const` table empty → dead-code elimination).

**Where a verdict goes, and when monitors start judging (phase-474 I1/I2).**
For a consumer of the model: the monitor TABLES are derived from the
resolved model's contract layer, as above, and nothing else about the
monitors is. In particular WHEN they arm is not a contract fact and is read
from the build only -- Kconfig `CONFIG_NROS_MONITOR_ARM_ON_CALL` (default
off: armed at the executor's first spin, the behaviour before phase-474) and
`CONFIG_NROS_MONITOR_ARM_GRACE_MS` (default 0), or the
`NROS_MONITOR_ARM_ON_CALL` / `NROS_MONITOR_ARM_GRACE_MS` build variables on
the cargo and cmake roads. rlm has no key for it and the contract does not
carry one. With ON_CALL the application arms the monitors where it enters its
running state (`nros_monitors_arm()`, `nros::arm_monitors()`,
`nros::monitor::request_monitor_arming()`); a verdict before that is counted
(`suppressed_before_arm`) and not stored.

A stored verdict goes to: the log line at detection (issue 0514), or the
drain-and-report line at the end of its spin with
`CONFIG_NROS_VIOLATION_DRAIN_REPORT`; the image's `/diagnostics` sink where
one is installed (issue 1635); trace markers 21-24 (rule code, endpoint
FNV-1a, measured, declared) with `CONFIG_NROS_TRACE_CALLBACKS`; the SWD
record `NROS_VIOLATION_RECORD` with `CONFIG_NROS_BOOT_REPORT`
(`scripts/read-violation-record.py`); and the executor's ring, which keeps the
LATEST `CONFIG_NROS_EXECUTOR_MAX_VIOLATIONS` (default 8) for
`drain_violations` and counts what it evicted.

**What `max-latency-runtime` measures (phase-474 I6).** One dispatch's
elapsed time on the executor's monotonic clock, from callback entry to
callback exit, charged to every monitored publisher whose publish count
advanced during that dispatch. This holds for every dispatch kind: a
subscription or service callback in the drains, and a timer callback whether
it fires in the drains or in the sweep that runs when the executor trigger
does not pass (a node that also subscribes, with no sample this spin). Work a
callback does synchronously, including a service call made from a timer
callback, is inside the window. The figure is not the timer's release jitter
(how late the callback started against its period), which
`release-jitter-runtime` judges, and it is not a route: waiting for the next
tick or crossing a link before the callback starts is outside it (phase-474
D3).

**The rate rule's first window (phase-474 I8).** `rate-hierarchy-runtime`
counts publishes over windows of `RATE_CHECK_INTERVAL_US` (5 s) that roll at
monitor ticks. The first window, opened at the first spin or reopened by
arming, starts at the first tick that sees a new sample rather than where it
opened, so it has the phase the later windows have; a stream that stays
silent is judged at the end of the window as opened.

**When the `/diagnostics` report leaves (phase-474 I7).** Detection writes
the ring, the SWD record, the trace markers and the log at once; the
`/diagnostics` report is queued (four deep) and sent at the end of a spin,
after its dispatches and rules, when the next timer is due no sooner than the
last report took (or a verdict has waited 1 s). Pending verdicts are
coalesced into one `DiagnosticArray`, as many as fit the reporter's 512 B
buffer. The reporter's time is reported by `violation_report_cost_us` and is
not charged to the release-jitter sample that follows it (issue 1727).

## CMake surface

`nano_ros_add_executable(... MODEL path/to/system_model.yaml)` as the
alternative to `LAUNCH "pkg:file.xml"` — the seam ASI already sits on
(its `system.launch.xml` is exactly what the resolved model replaces).
`MODEL` implies model-mode `codegen-system`. The launch-file path stays
supported; deprecation is a later decision.

## Non-goals

- `interrupt` sched task class (v1 rejects).
- On-target scope-path/drop monitoring (Linux side owns E2E).
- Retiring `system.toml` — it survives as the integrator's AUTHORING
  input (`[deploy]`/sched tasks) that play_launch `resolve` consumes (closing
  RFC-0050's open question). What retires is nano-ros's OWN
  resolution/bake path — see §Canonical path below.
- Dynamic model reload — baked images are one variant by construction.

## Canonical path (maintainer decision, 2026-07-17)

The SystemModel is the **canonical** configuration path. nano-ros's own
resolution machinery — `launch_synth`, `nros plan`'s launch-XML parsing,
`codegen-system`'s direct system.toml+launch consumption — is
**transitional** and retires once model parity lands. Consequences:

- New configuration features land model-side (play_launch resolve or the
  shared ros-launch-manifest crates), never in the legacy bake path.
- play_launch is improved along the way as nano-ros needs surface:
  `Deploy{domain, locator}` schema fields, `resolve` reading the
  integrator's `system.toml [deploy]`, per-target resolves for
  multi-target systems.
- Even embedded-only projects run `play_launch resolve` at BUILD time
  (build hosts are Linux); the target never parses anything — it gets
  the baked slice, as today.
- Retirement trajectory staged in phase-296 (§Retirement).

## Parity gap analysis (2026-07-17 exploration)

Everything the legacy path (`system.toml` + launch XML + `nros-plan.json`)
expresses, versus the model schema. Status: **model** = needs a schema
addition (shared crates); **resolve** = play_launch resolve work;
**local** = stays a nano-ros build knob (never system semantics);
**covered** = already expressible.

| Legacy feature | Source | Status → home |
|---|---|---|
| Node parameters (resolved key→values, baked into component configure) | launch `<param>` / `PlanInstance.parameters` | **model** — `structure.nodes[].params`. ROS parameters ARE system semantics; the "no spawn info" exclusion was about cmd/env/params-FILES, not resolved param values. Embedded has no record to read them from. |
| Remaps | launch `<remap>` | **covered** — the model's wiring carries RESOLVED topic FQNs; entry codegen binds endpoints to those names. Verify at W4.1. |
| Component class (`pkg::Class`) | `[[component]].class` | **covered-by-mapping** — model `NodeInstance.plugin` carries the class for library-component nodes (exec unused); the exec→class lookup for launch-sourced systems stays in cmake metadata. Document in W4.1. |
| RMW selection (`[system].rmw`, `[deploy.*].rmw`, `--rmw`) | system.toml | **model** — `execution.deploy[].rmw` + a system default (meta or execution header). The bake cannot pick a backend without it. |
| domain/locator (system + per-deploy override ladder) | system.toml | **model** — `Deploy{domain, locator}` (already filed as R1 ask #1). |
| `[[transport]]` network identity (ip/mac/gateway/interfaces, wifi ssid/psk, serial/can device+baud, per-transport rmw/locator/domain) — *retired 2026-10-24: sessions are `[[domain]]`, devices are layer 1, wiring is the RMW's own file ([RFC-0109](0109-network-config-layers.md))* | system.toml → `PlanTransport` | **model** — `execution.transports` (typed, per deploy target). Integrator-owned system config; the embedded boot bake (RFC-0045) and bridge sessions read it. Largest single gap. |
| `[[bridge]]` in-binary relays (from/to/topics/bidirectional) | system.toml (RFC-0009) | **model** — `execution.bridges` passthrough; `nros configure`-style type resolution moves behind resolve (types come from layer 1). |
| `[[domain]]` multi-domain routing | system.toml | **model** — folds into `execution.transports` (a transport = (rmw, locator, domain) session). |
| `[lifecycle] autostart` | system.toml | **model** — per-node `lifecycle_autostart` (`none\|configure\|active`) on `NodeInstance` or deploy entry; the contract-layer `lifecycle` flag stays the managed-node marker. |
| Capability axes (`features = [..]`, `[safety]`, `[param_services]`) | system.toml | **model** — `execution.features: [String]` (system-level; per-deploy `features` see build tuning below). Unknown names stay a bake-time error (capability_resolver). |
| Per-deploy build tuning (`profile`, `optimize`, cargo `features`), `kind`, `framework` | system.toml `[deploy.*]` | **model (passthrough)** — `Deploy.extra: map` (consumer-defined, documented keys). In the end state nano-ros must not parse system.toml at all, so even build knobs ride the model — as an open map, not typed schema (they are not cross-runtime semantics). |
| Per-endpoint QoS (manifest per-endpoint QoS + 211.H `qos_overrides.*` launch params) | manifests + launch params | **model** — endpoint contracts (`PubContract`/`SubContract`) gain optional `qos`; the 211.H launch-param overlay retires into it. |
| Board selection | `[deploy.*].board` / cmake | **covered** — `mcu:<board>` target encodes it. |
| `default_launch` / `default_target` | system.toml | **retire** — resolve-input ergonomics; obsolete once resolve is the front door. |
| RFC-0033 message capacities (`nros-codegen.toml`) | workspace file | **local** — message-generation knob, not system semantics. |
| Monitor tables (contracts → baked consts) | new (W3b.4) | **resolve→nano-ros** — emission from model contracts, already planned. |
| Actions wiring | manifest `actions:` | **resolve** — play_launch's ManifestIndex never merges actions; `structure.actions` is always empty today. |

Ordering: the transports/bridges/rmw block is the R1 critical path for any
real embedded system; params + per-endpoint QoS next; lifecycle/features/
extras are small.

## Open questions

- Stamp extraction ABI: peek-decode `Header` in the take path vs codegen
  emitting a per-type `stamp_offset` const (leaning const — no runtime
  type introspection).
- Whether POSIX sched task priorities upgrade to real SCHED_FIFO by default or
  behind a knob (today advisory; play_launch's rt_helper precedent says
  knob + privilege check).
- `bindings` targeting a callback group that the node code never declares:
  bake-time error vs warn (leaning error — same fail-loud family).
- **(realization)** Segment ↔ executor ↔ thread cardinality: strict
  `one segment = one executor = one thread` (analyzable, RTIC-like) vs letting
  the realizer **coalesce** segments onto one executor (fewer threads/stacks
  on tiny MCUs, at the cost of intra-executor priority shaping)?
- **(realization)** Do the six dims attach to the **segment** (leaning — the
  thread inherits them) or per-**callback** within a segment (PiCAS
  granularity — richer, but needs intra-executor scheduling even in the
  run-to-completion case)? nano-ros derives the per-(node, path) facts itself
  from the input contracts, so callback-granularity is available either way.

## Cross-track note — play_launch Phase 45 (Scheduling SSoT), 2026-07-18 — REVERTED

**Superseded 2026-07-20 (maintainer decision):** the sched-embedding this note
proposes **landed** (rlm `78f637d`, `model.execution.sched`) but is being
**reverted**. play_launch is a parser (input only); causality + execution
modeling is each consumer's job (RFC-0050 §"Input model; causality + execution
modeling per consumer"; §"nano-ros execution modeling"). The algorithm is
extracted into standalone reusable crate(s) instead of embedding its output.
Phase-46 (unified **input** model) continues. The original note is kept below
for provenance.

## Cross-track note — play_launch Phase 45 (Scheduling SSoT), 2026-07-18

play_launch's RT-scheduling track (vocab v2 + the `chain_aware` mapper,
play_launch Phases 41/42/44) and the SystemModel track are being unified so
that **the SystemModel is the single source of truth for scheduling**:
`play_launch resolve` runs the mapper once and embeds its *complete* output
into the model; every consumer — including this RTOS mapper — reads
scheduling from the model and never re-derives it. Design of record:
play_launch `docs/design/system-model-sched-ssot.md`; work breakdown:
play_launch `docs/roadmap/phase-45-sched_ssot_unification.md`.

What this RFC's consumer side gains, and the cross-track asks:

- **The model's `execution:` layer gains resolved scheduling structure**
  (Phase 45.2, in the shared `model` crate — a joint decision with this
  track): `mapper` identity, resolved `chains` (FQN-qualified `via` topics +
  the segment/boundary decomposition), and **per-path ranks**
  (`ChainAwareDetail`, one per (node, path)). Today `execution` is
  `sched_tasks` + `bindings` only; these are additive fields.
- **Per-path ranks are exactly what this RFC's callback-granularity mapping
  wants.** play_launch's POSIX apply layer projects per-path ranks down to a
  per-node max (a documented lossy compression); an RTOS executor can
  discriminate at callback granularity (this RFC's `sched_context.rs` already
  has the machinery). Embedding the per-path ranks means nano-ros need not
  inherit play_launch's POSIX lossy projection — the finer fact is carried in
  the artifact.
- **Type sharing (Phase 45.3):** the resolved chain/trigger structs
  (`types::{Trigger,EffectiveTrigger,ChainDecl,...}`, `sched::{ResolvedChain,
  ChainElement,MapperPath,ChainAwareDetail}`) are shared from
  ros-launch-manifest — no third hand-mirror in the `model` crate. The
  translation `types::ChainDecl` + launch DAG → `sched::ResolvedChain` is
  `sched_derive::resolve_chains` (play_launch); it must be shared, not
  reimplemented. This is the same "one schema, no hand-mirroring" rule
  RFC-0050 already states (issue 0160 / FFI struct-mirror lesson).

Ask of this track: (1) confirm where the resolved chain data lands —
`execution:` (alongside `sched_tasks`/`bindings`) vs `contracts:` layer — and
(2) the per-path-rank consumption model on the RTOS side (does the executor
bind a callback-group priority from `ChainAwareDetail.path`, or project to a
per-node task priority like POSIX). play_launch's 45.2/45.3 are held pending
this coordination.

## nano-ros execution modeling — self-derived, per-platform (2026-07-20)

**Supersedes the 2026-07-18 SSoT reconciliation** (maintainer decision). The
earlier version had nano-ros *consume* a resolved chain structure that
`play_launch resolve` would embed in the model. That embedding **landed**
(rlm `78f637d`, `model.execution.sched`) but is being **reverted**:

- **play_launch is a parser** — it gathers all input into the model; it does
  **not** do execution/scheduling modeling, and the model carries **no
  `execution.sched` / no resolved sched plan** (RFC-0050 §"Input model;
  causality + execution modeling per consumer"). The `sched` chain mapper is
  play_launch's Linux-runtime execution model, not a model-crate field.
- **The algorithm is shared, not the output.** `ros-launch-manifest-sched` is
  already the shared, pure crate (no parser deps) both runtimes vendor. The
  arrangement: **split `chain_aware_mapper`** into (a) a platform-agnostic
  **core** — feasibility + clock-segmentation + chain/segment **ranking**, a
  *priorityless* ordered/segmented structure — and (b) the **Linux realizer**
  (`rt_priority_band` → `ResolvedSchedTaskTable`, `posix`-tagged). nano-ros calls
  the agnostic core (a) + its **own RTOS realizer** (below); it does NOT use
  (b). **Derivation is nano-ros's own**, sharing the `MapperInput` type:
  nano-ros builds `MapperInput` from the **input** SystemModel —
  `contracts.node_paths` (input→output, `input: []` = timer boundary) +
  `structure.topics` wiring + the declared `sched_tasks`/`bindings` — where
  play_launch instead derives it from its `LaunchDump` (`sched_derive`,
  parser-coupled, Linux-side). One agnostic core, two derivations, two
  realizers; no consumed `execution.sched`.

The realization design below is unchanged; it operates on the DAG the shared
extraction crate produces, realized with RTOS kernel features.

> **The realizer's priority rule is RFC-0079.** The "*priorityless* ordered/
> segmented structure" this section specifies is exactly what RFC-0079 §3
> derives (deadline-monotonic over the declared contract) and §5 allocates into
> a per-port address plan. Worth recording that the implementation went the
> other way for a while: raw per-platform priority tables in the user's
> `system.toml`, which pushed the realizer's job onto the user and cost 3.5
> hand-written kernel numbers per tier. Nothing in this RFC is retired by that
> — RFC-0079 is the missing half, not a correction.

### The realization — causal segments over sched tasks

The sched task tables in §"The mapper" are a **fixed-priority realization**, not
the model. nano-ros's mapper works on the **causal segment** — a maximal
`input`-triggered run between two timer boundaries, **derived by nano-ros**
from the input (`contracts.node_paths` input→output + wiring; `input: []`
marks a boundary). Each segment → **one executor, run-to-completion in causal
order on one thread** (rclc / PiCAS model). The sched task survives only as the
fixed-priority fallback realization.

**Six-dim agnostic scheduling requirement**, read from the input contract +
declared-sched-task facts (activation from `node_paths`/trigger, deadline/budget/
criticality from the contract + the declared sched task); a **union of intents**
resolved at bake time to `Native | Backfill | Degrade(recorded)`:

| Dim | Intent | Native | Fallback (recorded) |
|---|---|---|---|
| `activation` | Timer{P} \| Event{srcs,sync} \| Sporadic{iat} \| Once | kernel timer / wait-obj; NuttX/Linux sporadic | executor Sporadic SC |
| `urgency` | criticality ordinal | kernel priority (dir-normalized) | — (universal) |
| `deadline` | D | Zephyr/Linux EDF | executor EDF, else deadline-monotonic priority + feasibility check |
| `budget` | C/WCET | Linux/NuttX reservation | executor Sporadic budget, else advisory-only |
| `non_preempt_scope` | run-to-completion / threshold | ThreadX threshold, Zephyr coop, RTIC SRP | priority-ceiling emulation |
| `placement` | core/mask | SMP affinity | ignored (single-core) |

(Expansion — jitter/release-precision, mutual-exclusion groups,
energy/tickless — is deferred; the six are locked for v1.)

**Two scheduling layers — executor/kernel split.** Prefer kernel-native
primitives; the nano-ros executor's own `SchedContext` **backfills** only
what the kernel lacks. Per-platform guarantees therefore differ **by design
and on the record**, never silently.

- **Unified executor** (kernel-can't; ROS-callback-level, portable): callback
  dispatch + wait-set demux; intra-segment causal run-to-completion order;
  LET / TT release; callback-group mutual exclusion; backfill `SchedContext`s
  (Sporadic budget, EDF-among-callbacks) — already present, unwired (§Baseline).
- **Delegated to the kernel** (native where possible): thread priority, EDF
  deadline, reservation budget, preemption scope, core placement.

**Three-layer architecture:**

```
L1  REALIZER (host, bake-time)
      6-dim segment reqs (nano-ros-derived from the input) × board CAPS
      → per-dim realization + degradation record
L2  PlatformSched (runtime, thin board trait)
      const CAPS { edf, reservation, preempt_threshold, affinity, n_prio, prio_dir }
      spawn(ThreadAttrs) / now / block_on / sleep_until [+ set_deadline / replenish]
L3  UNIFIED EXECUTOR (runtime, portable): dispatch, causal order, LET/TT,
      group mutex, backfill SchedContexts
```

`ThreadAttrs = { priority, core?, deadline?, budget?, preempt_threshold? }`.
L1 pre-decides, so the board never silently drops an attribute — anything it
can't honor was already routed to executor-backfill or degraded **and
recorded** (fail-loud, the W2 rejection-table philosophy).

### CAPS provenance — bake-authoritative (decided 2026-07-23)

L1 runs on the host at bake time; the board's *actual* kernel capability
(e.g. Zephyr `CONFIG_SCHED_DEADLINE`) lives in the downstream image build.
For the degradation record to be **honest** — never "L1 claimed `Native`
while the built image lacks the feature" — the capability is
**bake-authoritative from a single source**, not inferred by the CLI:

- A per-deploy capability knob is the SSoT — `[deploy.<board>]` in the
  deploy config (e.g. `edf = true`), integrator-owned like the rest of L3.
- `codegen-system` **fans that one knob out** to the three places that must
  agree: (a) L1's `SchedCaps` (so `realize_rtos` sees the real value, not a
  hardcoded per-platform guess — replaces the string-matched
  `sched_caps_for` default); (b) the generated board config (Zephyr
  `prj.conf` → `CONFIG_SCHED_DEADLINE=y`) so the kernel feature is compiled
  in; (c) a runtime `cfg` gate (board cargo feature) so the native-apply
  call path is live only when the feature is.
- Knob absent/false ⇒ L1 records `Degrade` **accurately** (the same knob
  gated the build), no kernel feature, executor `SchedContext` backfills.

So the board `const CAPS` is the *generated reflection* of the SSoT knob, and
the L1 record is provably consistent with the image. This closes the "where
does `const CAPS` come from" gap the L2 sketch left open.

**Runtime honoring (the L2 seam, minimally).** `set_deadline`/`replenish` need
not be a full trait to start: `run_sched_tasks` calls a `cfg`-gated board shim
(Zephyr `k_thread_deadline_set` on the sched task thread) when the sched task's
`sched_class` is `edf` and the feature is on — mirroring the existing
`k_thread_priority_set` adoption. The executor's `SchedContext` deadline
monitor stays live in **both** the `Native` and `Backfill` cases: under
`Native` the kernel supplies the *ordering* and the monitor supplies the
*miss handler* (`DeadlineAction`); under `Backfill` the monitor is both.
Semantic note for Zephyr: `CONFIG_SCHED_DEADLINE` orders **equal-priority**
threads by earliest deadline (a tiebreak, not a global EDF) — which matches
the one-`k_thread`-per-sched-task model. First realized on Zephyr for the deadline
dim (phase-296 W5.5); the other dims + a formal `PlatformSched` trait follow
the same shape.

Grounding: **PiCAS** (RTAS'21) — chain→priority + node→executor→core;
**rclc** (micro-ROS) — LET + static order on RTOS threads; **Casini et al.**
(ECRTS'19) — reservation-based chains + callback-group concurrency.

The DAG **extraction** is a **shared standalone crate** (reused by
play_launch's Linux runtime too); the **realizer** is nano-ros's (per
platform). play_launch contributes only the input model. See RFC-0050
§"Input model; causality + execution modeling per consumer" + its rework
items (revert `execution.sched`; extract the algorithm crate).
