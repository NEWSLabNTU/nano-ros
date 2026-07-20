# Phase 296 — SystemModel consumption: bake the model into embedded images

Implements RFC-0050 (consumer half) + RFC-0052 (the RTOS mapper).
Producer side is DONE (play_launch phase 43: `resolve` emits the model,
the Linux runtime consumes it; shared schema in the vendored
`ros-launch-manifest` `model`/`sched` crates, already pinned in
`packages/cli/third-party/`).

Status (2026-07-20): W1–W4 + W3b.1–.5 all LANDED (incl. the cross-runtime
parity fixture). **R2/R4 migration** in progress — **21 workspaces** on
the model path (ws-realtime-{rust,cpp} flagship, all feature families
across rust/cpp/c/mixed, the launch showcase, the `rust` monolith's 7
single-host native entries, + `native_entry_robot1/robot2` on the model
`host =` slice now that #236 steps 1–3 landed — play_launch 46.1 carries
`<node machine=>` → `deploy.host`, and the macro/CLI host filter is
validated E2E: robot1→talker, robot2→listener); **~14 remain** (the
monolith's embedded entries + `zephyr_entry_robot1` — blocked on the #236
board≠host unplaced-target sub-gap, `ws-safety-{cpp,c}`,
`ws-realtime-{c,cpp-*}` variants,
`ws-bridge-{rust,xrce}`, the 4 templates — see the R4 migration inventory
below + the two sub-blocker issues #236/#237). **R3 (deprecation warnings)
DONE + merged.** **R4 (legacy-path removal) IN PROGRESS** — the migration
tail is the only blocker; code removal lands once the R3 warning fires in
zero fixture builds (test-suite gated).

**Design (2026-07-20, RFC-0050/0052 — supersedes the 2026-07-19 SSoT note):**
play_launch is a **parser** — it gathers all input into the model (declared
`deploy`/`tiers`/`bindings` stay as input); it does **not** embed a resolved
sched plan. The landed `model.execution.sched` (play_launch 45.2/45.3, rlm
`78f637d`) is being **reverted**. **Causality + execution modeling is the
consumer's job**, and the reusable value is the *algorithm*, not the output:
the DAG/causality/segment + chain-resolution algorithm is **extracted into
standalone reusable crate(s)** that both runtimes call; nano-ros derives its
DAG/segments through that crate from the input (`contracts.node_paths` +
wiring), reads the declared tiers/bindings, and runs its OWN RTOS realizer.
This adds **W5 — the RTOS-framework-aware realizer** as a phase-296 impl wave;
**no dependency on `execution.sched`** (it's reverted). Runtime E2E monitoring
stays stamp-based (no chain-id).

## Waves

### W1 — model ingestion into `codegen-system`

- `nros codegen-system --model system_model.yaml` (mutually exclusive
  with the launch/system.toml pair): parse via the vendored `model`
  crate (schema-version gated), select this image's node slice by
  `execution.deploy` + board, map tiers/bindings through the existing
  `tier_resolver` → `nros-plan.json` + `run_tiers` const table, bake
  domain/locator (RFC-0045 rung) + endpoint wiring into
  `system_config.h`.
- Schema seam: `From<ros_launch_manifest_sched::TierDef> for
  nros_orchestration_ir::TierDef` + an every-field round-trip test
  (mirror-drift guard). No type replacement — orchestration-ir stays
  proc-macro-friendly.
- **Done when:** a play_launch-resolved `system_model.yaml` (rt_workspace
  shape: tiers + bindings + one `mcu:*` deploy entry) produces an
  `nros-plan.json` byte-equivalent to the same config authored in
  `system.toml`, and `nros check` passes on it.
- Landed: `orchestration/model_ingest.rs` (load + tier conversion with
  core/deadline hoisting + fail-loud bindings), `--model` on
  codegen-system, byte-equivalence + fail-loud integration tests, manual
  PLAN-IDENTICAL validation on ws-realtime-rust. Note: model `Deploy`
  lacks domain/locator fields — schema follow-up filed with W4.

### W2 — widen the tier pipe (kill the lossy narrowing)

- `PlanTierDoc` + `TierSpec` + every `run_tiers` carry `core`,
  `preempt_threshold` (ThreadX), `class`, `period_us`, `budget_us`,
  `deadline_us`, `deadline_policy`; fix the documented FreeRTOS
  `stack_bytes` drop (`freertos_run_tiers.c`).
- Platform-inapplicable fields in the selected target's sub-table =
  bake-time error (RFC-0051 rejection table).
- `budget_us`/`period_us` feed the existing sporadic `SchedContext`;
  `time_triggered` class binds the existing TT window from the tier
  table.
- **Done when:** per-platform fixture builds assert the new fields reach
  the task-creation calls (FreeRTOS stack regression test included), and
  a `preempt_threshold` on a zephyr sub-table fails the bake loudly.
- Landed: full pipe widened (PlanTierDoc, Rust TierSpec + macro tokens,
  the C ABI append across nros_native_tier_spec_t / NativeTierSpec /
  NativeTierSpecC / 4 board mirrors — core_plus1 + preempt_threshold,
  offsets documented), FreeRTOS stack drop fixed + SMP core pin
  (configUSE_CORE_AFFINITY-gated), shared
  `validate_tier_platform_applicability` in orchestration-ir called from
  BOTH the CLI bake and `nros::main!`. Follow-ups: zephyr/nuttx core-pin
  consumers need shim-API changes (transport complete, consumers
  pending); budget/period→SchedContext + TT-window binding moved to W3
  (one coherent executor wave with the monitors).

### W3a — LANDED: sched-context wiring + stamp-offset codegen

- `Executor::set_default_sched_context` (slot-0 replacement incl. the
  sporadic sibling state): the run_tiers model is one executor per tier,
  so the tier policy IS the default SC. Posix/native `run_tiers` lowers
  `class = "real_time"` + budget/period → Sporadic, `best_effort` →
  BestEffort, `deadline_us` → SC deadline, at both tier-run sites.
- `RosMessage::STAMP_OFFSET: Option<usize>` (trait default None) +
  rosidl-codegen emission: `Some(4)` for Header/Time-LEADING types only
  (CDR LE, 4-byte encapsulation, Time 4-byte aligned) — the raw-buffer
  peek constant the max_age monitor will use. Predicate unit-tested
  (Header-first, Time-first, header-not-first → None).

### W3b — on-target contract monitors (layer 2)

The three explored blockers become the first three steps — each is a
small, independently landable prerequisite with its own done-when.

#### W3b.1 — Rust diagnostics surface (blocker 1)

- Vendor `diagnostic_msgs` interface sources at
  `packages/cli/interfaces/diagnostic_msgs/` (the #204 bundled-share
  mechanism — same path std_msgs/builtin_interfaces took, so no-ROS
  hosts generate it too).
- New `packages/core/nros-diagnostics` (no_std + heapless): a thin
  `DiagnosticReporter` that owns one publisher on `/diagnostics`
  (`DiagnosticArray`) and exposes
  `report(rule_id, severity, fqn, message)` with the play_launch rule-id
  vocabulary as consts (`rate-hierarchy-runtime`, `max-age-runtime`,
  `max-latency-runtime`) + the assumption/guarantee tag in `values`.
  Thin-wrapper discipline (RFC-0019): no aggregation logic, one publish
  per report, rate-limited by a min-interval knob.
- **Done when:** a native example publishes a violation entry visible via
  `ros2 topic echo /diagnostics` (interop lane), and the crate builds
  `no_std` for one embedded board.

#### W3b.2 — epoch clock hook (blocker 2)

- `ExecutorConfig.epoch_us: Option<fn() -> u64>` beside `clock_us`
  (wall-clock µs since UNIX epoch). Posix/native default:
  `SystemTime::now()`. Embedded boards: wired from the platform layer
  where the board HAS wall time (RTC / synced transport), else `None` —
  and a `None` epoch source with a baked `max_age_ms` contract is a
  BAKE-TIME error via the monitor-table emitter (fail-loud, not a
  silently-dead monitor).
- **Done when:** posix executor exposes epoch time to monitors; an
  embedded bake with max_age contracts and no epoch source refuses with
  an actionable message.

#### W3b.3 — stamping (blocker 3)

- `nros-node` helper `Node::stamp_now()` (epoch hook →
  `builtin_interfaces/Time`) so nodes fill `header.stamp` in one call;
  the parity fixture pair (talker with Header-leading msg) stamps every
  publish. Book note in the first-node chapter's message section.
- **Done when:** the parity fixture's wire traffic carries non-zero
  stamps (asserted via the listener's received msg in-test).

#### W3b.4 — rate monitor + baked monitor table (LANDED, incl. parity fixture)

Landed: executor/monitor.rs (MonitorSpec + PubMonitorCell statics +
pure check_rate with window/dedup/recovery semantics, unit-tested),
Executor::set_monitor_table/drain_violations + spin-tick hook (one
branch when the table is empty), NodeHandle::set_monitors +
create_publisher cell attach (both constructor sites), relaxed-atomic
publish bump, codegen-system emission from model contracts, AND the
native cross-process parity fixture (below).

- `codegen-system --model` emits a per-node
  `static const nros_monitor_spec_t { topic, min_rate_hz, jitter_ms,
  max_age_ms, stamp_offset }[]` (C) / `const MONITORS: &[MonitorSpec]`
  (Rust macro path) from `contracts.pub_endpoints`/`sub_endpoints` —
  empty table when uncontracted (DCE; flash delta measured on
  mps2-an385).
- Publisher-side: per-endpoint counter + EWMA period at
  `publish_with_buffer`, checked on spin ticks against `min_rate_hz`/
  `jitter_ms` (monotonic clock only — independent of W3b.2).
- Violations through W3b.1's reporter.
- **Done — parity fixture LANDED (2026-07-18):**
  `packages/testing/nros-tests/bins/contract-monitor` (one crate, three
  bins: `pub`/`sub`/`diagsink`) + `tests/contract_monitor_parity.rs`. A
  native three-process topology over a real zenoh graph: the pub bakes a
  `min_rate_hz` contract and publishes a stale-stamped `std_msgs/Header`,
  the sub bakes a `max_age_ms` contract, both drain their violations
  through the `nros-diagnostics` reporter onto `/diagnostics`, and the
  diagsink observes. The violating case (2 Hz < 10 Hz + 2 s stale)
  surfaces BOTH `rate-hierarchy-runtime` and `max-age-runtime`; the
  compliant twin (20 Hz, fresh) stays silent while still delivering. The
  rule ids ARE the play_launch runtime vocabulary (RFC-0050 / the shared
  `nros-diagnostics::RULE_*` consts), so the same contract reports in the
  same words on the Linux runtime — the cross-runtime parity. Executor
  API added: `nros::monitor` re-export (umbrella access to the baked
  types) + hosted builds default `epoch_us_fn` to `SystemTime` so native
  age monitors activate without extra wiring (+ `Executor::set_epoch_clock`
  for boards with a synced RTC).

#### W3b.5 — max_age + node-path latency (LANDED, incl. parity fixture)

Landed (2026-07-17):
- `max-age-runtime` — `SubMonitorCell` + `AgeMonitorSpec` table
  (`Executor::set_age_table`); the take path peeks `header.stamp` from
  the raw CDR buffer at `M::STAMP_OFFSET` (`monitor::peek_stamp_us`,
  LE `Time` words; unstamped/pre-epoch = no sample) and records
  `epoch_now - stamp` (fetch_max). Hooked at ALL take sites: arena
  buffered (triple + ring), arena in-place, and the session-direct
  `Subscription::try_recv` (NodeHandle path; auto-seeded from the
  executor's table + epoch at create_node). Attachment needs a stamped
  type AND an epoch source — otherwise the hook is `None` (one branch).
- `max-latency-runtime` — dispatch elapsed time (std `Instant`, no_std
  `clock_us` hook) attributed to every monitored publisher whose
  counter advanced during that dispatch (upper bound on take→publish);
  window max drained per monitor tick. Budgets come from
  `contracts.node_paths[..].max_latency_ms` attached to each path's
  OUTPUT endpoints (`MonitorSpec.max_latency_ms`; latency-only rows
  get `min_rate_hz_milli: 0`).
- `deadline-miss-runtime` + `DeadlineAction` (ignore/warn/skip/fault,
  distinct from the `DeadlinePolicy` inheritance enum;
  `SchedContext.deadline_action`): post-dispatch elapsed vs the bound
  SC's `deadline_us`. `skip` masks the SC's remaining callbacks for
  the REST of that spin cycle (per-cycle bitmask; behavior-tested:
  `deadline_skip_masks_remaining_same_sc_callbacks`), `fault` invokes
  `set_fault_handler` (panic when unset). Violation fields generalized
  to `measured`/`declared` (unit per rule).
- TT-window binding at the run_tiers altitude: posix
  `apply_tier_sched` lowers `class = "time_triggered"` + `period_us`
  to `register_time_triggered_dispatcher(period)` + a default SC with
  `tt_window_duration_us = budget_us | period` (offset 0; multi-window
  = schedule-table API). Other boards: tier-sched glue still TODO
  (posix is the only `apply_tier_sched` — W2 note stands).
- Emitter: `codegen-system --model` bakes `NROS_AGE_MONITORS` (+
  `SubMonitorCell` statics) and `max_latency_ms` into
  `system_monitors.rs`; `nros_install_monitors` also calls
  `set_age_table`. Plan gains a skip-empty `age_monitors` section.
- **Done — the stale-stamp (`max_age`) half of the parity fixture
  LANDED (2026-07-18)** alongside the rate half: see W3b.4's
  `contract-monitor` fixture + `contract_monitor_parity.rs`. The
  violating sub receives 2 s-stale headers and reports `max-age-runtime`
  on `/diagnostics`; the compliant sub (fresh stamps) stays silent.

### W4 — CMake + ASI pilot

- W4.1 — LANDED (= R1-N2): `nros codegen entry --model`.
- W4.2 — LANDED: `nano_ros_add_executable(... MODEL <path>)` /
  `nano_ros_entry(... MODEL <path>)` keyword, mutually exclusive with
  LAUNCH, passing `--model` to the codegen-entry invocation
  (codegen-system --model landed W1; wiring both into one configure
  flow is the ASI pilot's job).
- W4.3 — ASI pilot (WIRED 2026-07-17; FVP smoke pending): ASI
  `controller_bringup` commits the resolved artifact
  (`config/system_model.yaml`, `play_launch resolve launch/… --system
  system.toml`) and the entry switched `LAUNCH` → `MODEL` (ASI 52d6ce7,
  nano-ros pin 4ea1f4a2e in its west.yml). Two cross-repo fixes fell
  out: play_launch `model_builder` now FILLS `NodeInstance.params`
  from the record (R1-M4 producer gap — params never rode the model;
  play_launch d1df358), and `plan_from_model` board slicing accepts
  the deploy's `kind` (extra.kind = "zephyr") so a concrete-board
  deploy (`mcu:fvp-aemv8r-smp`) matches the entry codegen's FAMILY
  key — covered by `plan_from_model_matches_deploy_kind_family`,
  which mirrors ASI's exact model shape.
- **Done when:** the ASI actuation image builds from the resolved model
  and the FVP/AVH smoke passes (needs the ASI dev container / AVH lane —
  not runnable on this host; ASI phase-3 §W3.b tracks the checkbox).

### W5 — RTOS-framework-aware realizer over a shared extraction crate (DESIGN LANDED 2026-07-20, impl future)

nano-ros does its OWN causality + execution modeling from the **input** model
(RFC-0052 §"nano-ros execution modeling"): no dependency on play_launch
embedding scheduling. The reusable value is the *algorithm*, extracted into
standalone crate(s) both runtimes call; nano-ros adds its RTOS realizer.
Prereq: the two cross-repo rework items (RFC-0050 §rework) — revert
`model.execution.sched`, and extract the algorithm crate.

- W5.0 — **cross-repo rework (prereq; tracked in play_launch phase-45 §45.10)**:
  (a) ~~revert `model.execution.sched`/`ExecutionSched` + `sched`-struct
  re-exports in `model`~~ **DONE** (rlm `f090400`; play_launch phase-45
  §45.10.a); (b) ~~split `chain_aware_mapper`~~ **DONE** (rlm `f5c0403`; play_launch phase-45
  §45.10.b): `chain_aware_rank(&MapperInput) -> RankedPlan` is the platform-agnostic
  core (feasibility + clock-segmentation + priorityless `Vec<RankItem>`; order =
  priority order, `fine_group` = segment membership); `realize_posix` is the
  `posix` Linux realizer. W5 consumes `RankedPlan` via `chain_aware_rank` /
  `ChainAwareMapper::rank`. play_launch keeps `sched_derive`
  (`LaunchDump → MapperInput`) + `realize_posix`.
- W5.1 — **derive `SystemModel → MapperInput` — ✅ DONE** (`c2c9cf31f`,
  `orchestration/mapper_input.rs`): `MapperNode` from `structure.nodes` (scope,
  criticality) + `contracts.node_paths` → `MapperPath` (`EffectiveTrigger`:
  empty input = `Timer` at the output's contracted rate, else `Input`;
  `max_latency_ms`; `exec_ms` None). Chains empty in v1 → the core degrades to
  criticality-bucketed RM/DM. `rank_from_model()` runs the pipeline to a
  `RankedPlan`. (Follow-up: chain-declaration input — needs a model contracts
  addition — for full chain-aware ranking.)
- W5.2 — **realizer** `L1` — ✅ DONE (`59c176a01`,
  `orchestration/rtos_realizer.rs`): `realize_rtos(&RankedPlan, &MapperInput,
  &SchedCaps) -> RtosPlan`. Six dims → per-dim `Native | Backfill |
  Degrade(recorded)`: urgency→priority (rank+direction), activation→Timer
  period, deadline→EDF-native-or-DM-priority, budget→reservation-or-executor-
  Sporadic; `non_preempt_scope`/`placement` `NotRequested` pending derivation.
  Flat `Degradation` record (fail-loud). (Follow-up: priority band-scarcity
  collapse; core placement from `execution.deploy`.)
- W5.3 — **`SchedCaps` board seam — ✅ DONE (host half)** (`rtos_realizer.rs`
  `sched_caps_for`): per-platform `SchedCaps` grounded in the scheduler survey
  (posix EDF+reservation; zephyr EDF, low=high; freertos fixed-prio; threadx
  preemption-threshold, low=high; nuttx reservation; bare-metal single-core).
  Drives the realizer; kept consistent with W2's applicability. **Done-when
  met:** the same ranking realizes differently on Zephyr (EDF native) vs
  FreeRTOS (deadline→DM-priority, recorded). Remaining (folds into W5.4): the
  **runtime** `PlatformSched` trait (`spawn(ThreadAttrs)`/`set_deadline`/
  `replenish`) so boards apply the native attrs at run time; `n_priorities`
  refinement from the board descriptor.
- W5.4 — **wire the realization into the bake — ✅ DONE (host half)**
  (`rtos_realizer.rs` `rtos_plan_to_tier_table`): convert `RtosPlan` →
  `ResolvedTierTable` (one tier per realized node; `class`/`period_us`/
  `budget_us`/`deadline_us`/`core`/`preempt_threshold` ride through; urgency-
  ordered per board direction) so the existing `codegen-system` plan emitter +
  `run_tiers` const table consume it unchanged. The full pipeline now exists:
  `SystemModel → mapper_input_from_model → chain_aware_rank → realize_rtos →
  rtos_plan_to_tier_table → ResolvedTierTable → bake`. The executor already
  lowers `class`/budget/period/deadline → `SchedContext` (Sporadic/EDF/TT) for
  posix/native (W3a).
- **Embedded runtime lowering — ✅ DONE** (W5.4 follow-on): the W3a
  tier→SchedContext lowering is now a **portable** method
  `ExecutorNodeRuntime::apply_tier_sched_policy(class, period_us, budget_us,
  deadline_us, deadline_policy)` (nros `node_runtime.rs`), shared by every
  board (posix refactored to delegate; **zephyr/freertos/nuttx** `run_tiers`
  call it after `set_active_groups`). So `class`/budget/period/deadline lower to
  `SchedContext` (Sporadic/EDF/TT) on the embedded boards too. Host-verified via
  posix (2 tests); the calls type-check against `TierSpec`. ThreadX (single-
  executor entry — needs multi-tier first: **RFC-0053 / phase-297**) + embedded
  SDK build verification (fixture/CI) are follow-ups.
- Remaining: the runtime `PlatformSched` primitives (`set_deadline`/`replenish`)
  so boards apply *kernel-native* EDF/reservation (today the executor's own
  `SchedContext` backfills); hook the realizer path into `codegen-system` as an
  alternative to `tier_resolver`.
- **Done when:** a two-boundary chain crossing two platforms bakes distinct
  realizations (e.g. Zephyr EDF vs FreeRTOS executor-EDF) from the SAME
  self-derived DAG, with the guarantee difference recorded; and the realizer
  produces a plan PLAN-equivalent to the tier path for the degenerate
  single-segment case.
- Open forks (RFC-0052 §Open questions): segment↔executor↔thread cardinality;
  dims-on-segment vs dims-on-callback (nano-ros derives the per-(node,path)
  facts itself, so callback-granularity is available either way).

## Notes / risks

- `[deploy]` SSoT decision (RFC-0050 open question) closes as: deploy
  lives in `system.toml`; play_launch `resolve` consumes it as its
  system-config input. Requires a small play_launch follow-up (read
  `[deploy.*]` → `execution.deploy`) — file there when W1 lands.
- W3's stamp ABI is the riskiest piece (repr/CDR offset assumptions) —
  keep it codegen-const, never runtime introspection; Kani harness on
  the offset math.
- Runtime monitors must respect the no-heap discipline (`heapless`
  counters, fixed-size EWMA state per endpoint).

## Retirement trajectory (canonical-path decision, 2026-07-17)

The SystemModel is canonical; nano-ros's custom resolution retires at
parity. Ordered gates (each verifiable before the next):

- R1 — model parity (gap inventory: RFC-0052 §Parity gap analysis).
  Concrete items, dependency-ordered:
  - **M (shared model schema, ros-launch-manifest): LANDED (b44465d,
    2026-07-17)** — all six additive, golden-fixture + round-trip
    covered; play_launch builder fills the new fields with defaults
    until P1 lands.
    M1 `Deploy{domain, locator, rmw}` + `Deploy.extra` open map;
    M2 `execution.transports` (typed PlanTransport equivalent: ip/mac/
    gateway/interfaces/ssid/psk/device/baud + per-transport rmw/locator/
    domain) — the largest gap, folds `[[domain]]`;
    M3 `execution.bridges` + `execution.features`;
    M4 `structure.nodes[].params` (resolved values — ROS params are
    system semantics; embedded has no record to read);
    M5 endpoint contracts gain optional `qos` (retires the 211.H
    launch-param overlay);
    M6 per-node `lifecycle_autostart`.
  - **P (play_launch resolve): P1+P2 LANDED (efdc92d + manifest
    484a411, 2026-07-17)** — `resolve --system` fills execution (deploy
    placement via `[deploy.<name>].nodes` FQN lists, RFC-0004 ladder,
    transports/bridges/features, provenance-hashed); the loader merges
    `actions:`. **P3 DECIDED: one model carries all targets** — TierDef
    already holds every platform sub-table and transports/deploy are
    per-node; consumers slice by their board. Limitation recorded: the
    mapper-DERIVED sched path stays per-target (synthesized tiers carry
    only the resolve target's placement); declared tiers are
    target-complete.
  - **N (nano-ros):** N1 **LANDED** — `codegen-system --model` bakes
    `system_monitors.rs` (one PubMonitorCell static per contracted
    publisher + the MonitorSpec table + installer fn; empty = nothing,
    legacy byte-identical) and a `monitors` plan section; orphan
    contracts (endpoint with no owning topic) refuse the bake. N2
    **LANDED** — `codegen entry --model` (plan_from_model: board slice,
    params, group_tiers from bindings, features/lifecycle; plugin nodes
    take their class's bare name as exec, typed metadata enrichment
    unchanged) + W4.2 CMake `MODEL` keyword on nano_ros_add_executable /
    nano_ros_entry (mutually exclusive with LAUNCH). N2 tail **LANDED** —
    the executor auto-seeds every `NodeHandle` with its monitor table
    (`create_node` / bridge factory / `with_node`), so
    `set_monitor_table(&SYSTEM_MONITORS)` alone activates publisher
    attachment; the baked `system_monitors.rs` is `include!`-able from
    any Rust entry. Deferred with a note: C/C++ entry monitor FFI
    (install + drain surface through `nros_c`) — Rust-only until a
    C consumer exists. N3 **LANDED** — model `execution.transports`
    rides into the bake plan (`transports` section in `nros-plan.json`,
    same `PlanTransport` shape the board network bake consumes; unknown
    kind = refused bake; transport-free model stays byte-identical).
- R2 — migration (IN PROGRESS): ASI pilot (W4.3, wired) + in-tree
  workspace examples build from resolved models; book chapters switch to
  the resolve→bake flow.
  - **`nros::main!(model = "…")` arm LANDED (2026-07-18)** — the
    canonical Rust bake path. Reads a committed
    `<bringup>/config/system_model.yaml` (default path) instead of
    re-parsing launch XML + system.toml: slices nodes by the entry's
    board, resolves params/identity/lifecycle from the model, and
    resolves the tier table through the SHARED
    `nros_orchestration_ir::tier_from_model` + `resolve_tiers` (the same
    path `codegen-system --model` uses — no drift). `launch`/`model` are
    mutually exclusive; `launch` stays the transitional arm.
  - `tier_from_model` relocated from nros-cli-core into
    nros-orchestration-ir (the crate whose charter is "shared by the CLI
    codegen + the nros::main! proc-macro"), with its drift-guard test.
  - **`ws-realtime-rust/native_entry` migrated** to
    `nros::main!(model = "demo_bringup")` + a committed
    `demo_bringup/config/system_model.yaml` (no deploy layer — the
    homogeneous multi-board demo keeps every node on each board and
    resolves tiers for the entry's own RTOS). Validated: the
    `realtime_tiers` native-rust e2e passes on the model-baked entry.
  - **`ws-realtime-cpp/native_entry` migrated** to
    `nano_ros_add_executable(... MODEL <config/system_model.yaml>)` — and
    this surfaced + fixed a real CMake bug: the component auto-link
    sidecar (`<exe>_link_libs.cmake`) and the generated-TU `target_sources`
    were gated on `_NRA_LAUNCH` ONLY, so any TYPED **MODEL** entry never
    linked its `<pkg>_<exec>_component` libs → the generated TU failed with
    `<pkg>/<Class>.hpp: No such file` (the component libs carry the class
    headers' include dirs). Now gated `(_NRA_LAUNCH OR _NRA_MODEL)`. This
    also unblocks the ASI W4.3 pilot's full C++ build (only its codegen
    was dry-run before). Validated: the `realtime_tiers` native-cpp e2e
    passes on the model-baked entry.
  - **`ws-realtime-cpp` RTOS entries DONE (2026-07-18)** — `nuttx_entry`,
    `riscv_nuttx_entry`, `zephyr_entry` migrated to
    `nano_ros_add_executable(... MODEL <config/system_model.yaml>)`.
    Validated: `case_08_nuttx_arm_cpp` and `case_13_nuttx_riscv_cpp` pass
    on the model-baked entries. `case_06_zephyr_cpp` fails IDENTICALLY on
    LAUNCH and MODEL on this host (native_sim low-tier never scheduled —
    a pre-existing lane issue, not a migration regression), so the
    migration is validated-equivalent + CI-gated. **ws-realtime-cpp is
    fully migrated.**
  - **Book chapters DONE** — the getting-started Rust entry-pkg + C++
    workspace chapters document the `model =` / `MODEL` canonical path.
  - **Rust RTOS entries DONE (2026-07-18)** — `ws-realtime-rust`'s
    `nuttx_entry` (qemu-arm-nuttx), `riscv_nuttx_entry` (rv-virt), and
    `zephyr_entry` (native_sim) all migrated to
    `nros::main!(model = "demo_bringup")` off the SAME committed model.
    Validated end-to-end (cross-compile + QEMU `realtime_tiers` e2e):
    `case_10_nuttx_arm_rust`, `case_11_nuttx_riscv_rust`,
    `case_05_zephyr_rust` all pass on the model-baked entries. Prereq
    provisioned canonically: `nros setup --tool genromfs` (the rv-virt
    board bakes an `etc/` ROMFS at export) — the `just doctor` MISSING
    message + the SDK-index comment were corrected to the canonical
    `nros setup` path (they had pointed at apt / claimed genromfs
    unneeded). **ws-realtime-rust is fully migrated.**
- R3 — deprecation **DONE (2026-07-18)**: the legacy launch-XML /
  `system.toml` bake paths emit `warning[deprecated]` (once per process,
  silence with `NROS_ALLOW_LEGACY_BAKE=1`) — `codegen-system` without
  `--model`, `nros codegen entry --launch`, `nros plan`, and
  `nros::main!(launch = …)` (build-time notice from the proc-macro).
  Shared helper: `nros_cli_core::deprecation`. `launch_synth` carries a
  module-level deprecation note (no `#[deprecated]` attribute — its
  in-crate callers compile under `-D warnings` until R4 deletes both).
  RFC-0004/0015/0032 gain a canonical-path banner pointing at the model
  pipeline. CLI test suite green with the warnings (416 pass).
- R4 — retirement **IN PROGRESS (2026-07-18)**: migrating the ecosystem
  to the model path family-by-family (models hand-authored — play_launch's
  `system_config` reads features/deploy but not `[tiers]`/`[lifecycle]`,
  so a resolve would drop those; hand-authoring keeps the model faithful).
  Each family: author `<bringup>/config/system_model.yaml`, swap its
  entries to `model`/`MODEL`, rebuild fixtures, run e2e — kept green.
  When the R3 deprecation warning fires in zero fixture builds, the code
  removal (require `--model`, delete the `launch` arm + `launch_synth`)
  lands as one test-green change. Progress tracked in the inventory below.
  Original blocker analysis:
  removing the launch-XML / `system.toml` bake path (make
  `codegen-system` require `--model`, delete the `nros::main!(launch)` arm
  + `launch_synth`) breaks **~145 unmigrated consumers** (52 Rust
  `nros::main!(launch)`, 65 CMake `LAUNCH`, 28 C++ `NROS_MAIN(…launch…)`)
  vs the 8 migrated ws-realtime entries — the full `build-test-fixtures` /
  `test-all` suite would go red. R4's non-breaking parts are DONE (the
  RFC canonical-path banners above); the code removal is gated behind
  migrating those consumers to `model` / `MODEL`, one example family at a
  time, until the deprecation warning fires nowhere. The test suite is the
  merge gate that enforces this — R4 code-removal is not mergeable until
  the ecosystem is green on the model path.

### R4 migration inventory (2026-07-18)

The retirement WILL happen; the remaining work is mechanical and
low-friction — per the design intent, the user-side CMake / build-script
change is tiny. Each entry is a **one-line keyword swap**:

```cmake
-    LAUNCH  "demo_bringup:system.launch.xml"
+    MODEL   "${CMAKE_CURRENT_SOURCE_DIR}/../demo_bringup/config/system_model.yaml"
```
```rust
-nros::main!(launch = "demo_bringup");
+nros::main!(model  = "demo_bringup");
```

plus **one committed `<bringup>/config/system_model.yaml` per workspace**
(resolved once with `play_launch resolve … --system … -o …`, or authored
directly — the ws-realtime models are ~40 lines). No source, wiring, or
runtime change; the emitters/IR/`run_tiers` seam are identical.

**Migration units — 35 distinct example workspaces** (each = 1 model +
the per-entry swap), plus the `packages/testing/nros-tests/fixtures/*`
entry fixtures:

- **Migrated (16 workspaces so far, 2026-07-18/19):**
  - **rust (9):** `ws-realtime-rust`, `ws-realtime-cpp` (flagship, tiers);
    `ws-lifecycle-rust` (native `case_11` + zephyr `case_14`);
    `ws-params-rust` (zephyr `case_12` — launch `<param>` rides
    `structure.nodes[].params`); `ws-qos-rust` (zephyr `case_13`);
    `ws-custom-msg-rust` (build-validated); `ws-safety-rust` (native
    `case_14` — MULTI-MODEL, three launch variants → three models via the
    `model = "demo_bringup:config/<file>.yaml"` file-override form);
    `ws-launch-rust` (the launch showcase — the `<group ns=>` fix gives
    it `/alpha/talker`).
  - **cpp (3):** `ws-lifecycle-cpp` (native `case_13`, resolve-generated);
    `ws-qos-cpp` (native `case_09`), `ws-custom-msg-cpp` (native
    `case_02`), `ws-params-cpp` (build-validated). All resolve-generated.
  - **c (4):** `ws-{params,qos,custom-msg,lifecycle}-c` — resolve-
    generated, compile-validated (same typed-C++ codegen the cpp cases
    runtime-validate; CI runs the c runtime cases).
  Lesson: the model must capture EVERY launch detail — node params,
  remaps, lifecycle, features, namespaces — or the platform test catches
  the gap (params failed until `publish_period_ms: 250` was added; the
  `<group ns=>` parser fix was needed for namespaced showcases).
- **Monolith native (7 entries) DONE (2026-07-19):** the single-host
  native entries of `examples/workspaces/rust` (native_entry +
  service_{server,client,inprocess} + action_{server,client} + showcase)
  bake from per-launch resolved models (resolved WITHOUT `--system` — the
  monolith's multi-`[deploy]` system.toml carries no tiers/lifecycle/
  features, so the no-deploy model has each board-entry keep all its
  nodes). native_entry runtime-validated (`deployed_native_system_e2e`).
- **Remaining tail (~18) + two sub-blockers to fix first:**
  - **Multihost (`<node machine=>`) — LANDED (2026-07-20).**
    `native_entry_robot1/robot2` now bake with
    `nros::main!(model = "demo_bringup:config/multihost_model.yaml", host =
    "robotN")`. play_launch 46.1 carries `machine` through launch_dump →
    `model_builder` → `execution.deploy[fqn].host`; the macro + CLI `host`
    filter keep host-matching + unhosted nodes (mirror `Plan::for_host`),
    validated E2E (robot1→talker, robot2→listener). **`zephyr_entry_robot1`
    stays on `launch`** — board≠host orthogonality: a launch-only model
    defaults the machine-only deploy to `target: linux`, which the zephyr
    board slice rejects. Needs a play_launch *unplaced* target so board is
    entry-determined. Tracked: issue #236 (“Remaining sub-gap”) + the
    RFC-0050 reply flagging the field. Also fixed the example's invalid XML
    comment (`--host` → literal `--` inside `<!-- -->`, which spec-strict
    roxmltree rejected; our lenient `nros-launch-parser` had tolerated it).
  - **`ws-safety-{cpp,c}` safety build flag.** Their node sources use
    `create_subscription_with_safety`, gated behind the safety-e2e build
    flag the plain workspace cmake configure does not set (the fixture
    builder uses a `-safety-*` build_subdir with the flag). Migrate once
    the build wires it. Tracked: issue #237.
  - **Straightforward remainder** (no blocker, resolve-batchable like the
    families above): `ws-{safety,qos,params,custom-msg,lifecycle}-mixed`,
    the `ws-realtime-{c,cpp-*}` board/shape variants, `ws-bridge-{rust,
    xrce}` (`[[bridge]]` relays — model carries `execution.bridges`), the
    4 templates, and the monolith's embedded entries (zephyr/freertos/
    nuttx/esp32/threadx — need per-platform fixture rebuilds).
- **`play_launch resolve` is now the batch tool for the simple/tiered
  tail (2026-07-18).** play_launch's `system_config` reader was extended
  (ros-launch-manifest `468504a`, play_launch `4a735b0`; nano-ros vendored
  pin bumped to `468504a`) to read the nano-ros inline `system.toml`
  sections it previously ignored: `[tiers.*]` → `execution.tiers`,
  `[[component]].group_tiers` → `execution.bindings`, `[lifecycle].
  autostart` → `structure.nodes[].lifecycle_autostart`. Verified:
  `play_launch resolve --system system.toml` now emits COMPLETE models
  for `ws-lifecycle-rust` (lifecycle) and `ws-realtime-rust` (2 tiers +
  bindings), matching the hand-authored ones. So the remaining simple +
  tiered workspaces (cpp/c/mixed feature families, the realtime board
  variants) can be batch-resolved rather than hand-authored.
- **play_launch `<group ns=>` gap — ROOT-CAUSED + FIXED (2026-07-18).**
  `ws-launch-rust` (the `<arg>`/`$(var)`/`<group ns=>`/`<remap>`/
  `<include>` showcase) resolved to a model whose node FQNs DROPPED the
  group namespace (`/alpha/talker` → `/talker`). Root cause: play_launch's
  `play_launch_parser` deliberately ignored the `ns=` attribute on
  `<group>` (`GroupAction::from_entity` set `namespace = None`, with a
  comment wrongly claiming ROS 2 rejects it) — while nano-ros's own
  `nros-launch-parser` (RFC-0024) HONORS it, so the two parsers DISAGREED
  (nano-ros launch → `/alpha/talker`; play_launch model → `/talker`), the
  exact cross-runtime inconsistency the model exists to prevent. Fix
  (play_launch_parser `7582c77`, play_launch `af0c614`, nano-ros vendored
  pin `19b04f606`): `GroupAction` parses `ns`/`namespace`, and the entity
  traverser pushes it onto the namespace stack for the group body (scoped
  groups pop it via save/restore_scope). 420 parser tests green; verified
  `ws-launch-rust` now resolves `/alpha/talker` + `/alpha/listener`. So
  ns-using workspaces are now migratable.
- **`<remap>` — NOT a gap for nano-ros (design finding).** The SystemModel
  schema carries no per-node remaps, but nano-ros's entry codegen does not
  ROUTE remaps either (`nros-launch-parser` parses `<remap>` into
  `NodeSpec.remaps`, but neither the `nros::main!` launch arm nor the
  model arm bakes them — nodes use their declared topic names; the
  codegen carries a "future `<remap>` routing" TODO). Launch and model
  therefore AGREE (both ignore remaps), so no inconsistency today. If
  nano-ros ever routes remaps, the model needs a `NodeInstance.remaps`
  field + the consumer to apply it — tracked as future work, not an R4
  blocker.
- **Still complex:** `ws-bridge-rust` / `ws-bridge-xrce-rust` (`[[bridge]]`
  in-binary relays — the model carries `execution.bridges`, but the
  workspaces also need `nros-bridge.toml` wiring checked) and the
  16-entry `examples/workspaces/rust` monolith (many bringups × platforms).
- **Remaining workspaces** (`examples/workspaces/`): `rust`, `c`, `cpp`,
  `mixed`; `ws-{safety,lifecycle,qos,params,custom-msg,bridge,bridge-xrce,
  launch}-{rust,c,cpp,mixed}` (per language variant); the
  `ws-realtime-{c,c-mps2,cpp-fvp,cpp-mps2,cpp-rclcpp,cpp-subnode,
  cpp-subnode-portable}` board/shape variants.
- **Templates** (`examples/templates/`): `multi-node-workspace`,
  `multi-node-workspace-cpp`, `c-and-cpp-mixed-workspace`,
  `pure-c-workspace`.
- **Keep on `launch` until R4 deletes it:** the tests that deliberately
  exercise the deprecated form — `native_main_macro_forms.rs`,
  `native_main_macro_misuse.rs`, and the `nros-macros` doc examples. These
  are validators OF the launch arm, not consumers to migrate; they move to
  `model` (or are removed) in the same commit that deletes the arm.

Suggested cadence: migrate one workspace family per PR (author the model,
swap its entries, rebuild its fixtures, run its e2e), so each step stays
green. When the R3 deprecation warning fires in zero fixture builds, R4's
code removal (require `--model`, delete the `launch` arm + `launch_synth`)
becomes a mergeable, test-green change.
