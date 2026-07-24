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
single-host native entries, `native_entry_robot1/robot2` on the model
`host =` slice (#236 steps 1–3, play_launch 46.1 carries
`<node machine=>` → `deploy.host`, host filter validated E2E), + **`ws-bridge-rust`
and `ws-bridge-xrce-rust`** (2026-07-21 — `execution.bridges` from the bringup
`[[bridge]]`; both native entries bake the bridge backend on the model arm).
2026-07-23: the `{c,cpp,mixed}` monolith native entries (21 CMake) and the
`ws-realtime-{c,c-mps2,cpp-fvp,cpp-mps2,cpp-rclcpp,cpp-subnode,
cpp-subnode-portable}` variants (10 CMake) are migrated too. **Holdout
inventory (classifier): 17 CMake + 7 Rust remain** — see "Remaining migration
+ retirement jobs" below (M1 monolith embedded, M2 templates, M3 blocked
#236/#237 pair, M4 test fixtures, M5 book; then R-code.1–.3 incl. MODEL
becoming the convention-discovered DEFAULT users never spell). C/C++ migration
state lives in the CMake `LAUNCH`/`MODEL` keyword, not the `.c`/`.cpp` source.
**R3 (deprecation warnings)
DONE + merged.** **R4 (legacy-path removal) IN PROGRESS** — the migration
tail is the only blocker; code removal lands once the R3 warning fires in
zero fixture builds (test-suite gated).

**Design (2026-07-20, RFC-0050/0052 — supersedes the 2026-07-19 SSoT note):**
play_launch is a **parser** — it gathers all input into the model (declared
`deploy`/`tiers`/`bindings` stay as input); it does **not** embed a resolved
sched plan. The landed `model.execution.sched` (play_launch 45.2/45.3, rlm
`78f637d`) was **reverted** (W5.0, rlm `f090400`→`f5c0403`; the crate no longer
exposes `ExecutionSched`). **Causality + execution modeling is the consumer's
job**, and the reusable value is the *algorithm*, not the output: the
DAG/causality/segment + chain-resolution algorithm is **extracted into
standalone reusable crate(s)** that both runtimes call; nano-ros derives its
DAG/segments through that crate from the input (`contracts.node_paths` +
wiring), reads the declared tiers/bindings, and runs its OWN RTOS realizer.
This adds **W5 — the RTOS-framework-aware realizer** (LANDED W5.0–W5.4) as a
phase-296 impl wave; **no dependency on `execution.sched`** (it's reverted).
Runtime E2E monitoring stays stamp-based (no chain-id).

**Reconciliation check (2026-07-21) — our consumption is consistent with the
reverted-sched design.** Verified after the W5 landings + rlm pin `f5c0403`:
(1) no nano-ros source reads `execution.sched`/`ExecutionSched` (the only
mention is a comment in `orchestration/mapper_input.rs` noting the field was
reverted); nano-ros derives its own plan via
`mapper_input_from_model → chain_aware_rank → realize_rtos`. (2) No committed
example model carries an `execution.sched` block — models are pure INPUT
(declared `tiers`/`bindings`, baked by the existing `tier_resolver`; the W5
realizer is the landed alternative path, not yet the default). (3) Artifact
hygiene: 31 of 41 committed models still carry a stale `meta.record:` from a
pre-46.5 play_launch binary (the unified design drops it — see the play_launch
Phase 46 note in RFC-0050); harmless (our crate has no `deny_unknown_fields`),
regenerate on next touch. Newly-resolved models (46.5 binary) are clean.

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
  posix (2 tests); the calls type-check against `TierSpec`. ThreadX multi-tier
  `run_tiers` (calling `apply_tier_sched_policy`) landed with **phase-297 W4**
  (runtime e2e = 297 W5); embedded SDK build verification (fixture/CI) remains
  a follow-up.
- W5.5 — **Zephyr Native EDF — first runtime honoring of a `Native` dim
  (design 2026-07-23, RFC-0052 §"CAPS provenance").** Closes the plan/runtime
  gap: today L1 records `deadline_real = Native` for Zephyr (`sched_class="edf"`),
  but the runtime only sets `k_thread_priority_set` — no `k_thread_deadline_set`,
  no `CONFIG_SCHED_DEADLINE` — so the deadline is really the executor's
  cooperative monitor (`Backfill`) mislabeled `Native`. The slice makes the claim
  true end-to-end, or degrades honestly:
  - **SSoT knob (bake-authoritative):** a per-deploy `edf` capability
    (`[deploy.<zephyr>]`) fanned out by `codegen-system` to (a) L1 `SchedCaps.edf`
    (replaces the hardcoded `sched_caps_for("zephyr")` `edf: true`), (b) generated
    `prj.conf` `CONFIG_SCHED_DEADLINE=y`, (c) a `nros-board-zephyr` cargo feature
    gating the apply path. Knob false ⇒ L1 `Degrade` is accurate against the image.
  - **Runtime seam (L2, minimal):** a `cfg`-gated Zephyr shim
    `nros_zephyr_set_current_deadline(deadline_us)` → `k_thread_deadline_set`
    (µs→cycles), called by `run_tiers` for boot + spawned tier tasks when
    `sched_class == "edf"` and the feature is on. Mirrors the existing
    `k_thread_priority_set` adoption. Executor `SchedContext` deadline monitor
    stays live as the miss-handler (`DeadlineAction`) in both cases.
  - **Host (mostly exists):** extend `rtos_realizer` honesty tests so `caps.edf`
    is sourced from the knob (a `[deploy.zephyr] edf = false` → accurate `Degrade`
    record); codegen test: knob on ⇒ `prj.conf` has `CONFIG_SCHED_DEADLINE=y` +
    tier carries `sched_class="edf"`/`deadline_us`; off ⇒ neither.
  - **Build fixture + QEMU e2e:** a Zephyr fixture with ≥2 equal-priority deadline
    tiers builds with the feature on (`CONFIG_SCHED_DEADLINE` + the deadline-set
    path link); boot asserts via trace marker that `set_current_deadline` fired
    per EDF tier (the `Native` claim honored end-to-end).
  - **Done when:** knob-on Zephyr image boots with `k_thread_deadline_set` applied
    (trace-confirmed) and `CONFIG_SCHED_DEADLINE` compiled in; knob-off bakes an
    accurate `Degrade` record and no kernel feature. **Non-goals (follow-ups):**
    *behavioral* earliest-deadline-ordering proof (Zephyr's equal-priority
    tiebreak makes a deterministic QEMU ordering test fiddly); the other five dims;
    a formal `PlatformSched` Rust trait (this slice uses the C-shim + `run_tiers`
    seam); RTOS-side priority band-scarcity collapse.
- W5.6 — **realizer wired into the bake as the DERIVED-schedule path — ✅ DONE**
  (2026-07-23): `model_ingest::derive_execution_from_contracts` engages when a
  `--model` bake declares NO `execution.tiers` — `mapper_input_from_model →
  chain_aware_rank → realize_rtos` (with `sched_caps_from_deploy` honoring the
  per-deploy `edf` knob, now LIVE — unanimous-or-error across entries carrying
  it), then synthesizes the plan into ordinary `[tiers.*]` + `[[node_overrides]]`
  rows (`derived-<node>` tiers; generic class/period/budget/deadline + per-RTOS
  priority sub-table; sched_class left unset — the generic policy carries the
  semantics) so `resolve_system_tiers` → validation → plan → `run_tiers` consume
  them unchanged. Declared tiers always win; ranked nodes with no declared
  callback groups stay on the default tier (loud note); every degradation is
  printed. Unit-tested (derive/groupless/edf-conflict).
- W5.7 — **Zephyr placement (core-pin) consumer — ✅ DONE** (2026-07-23): the
  `core` knob rode the W2-widened pipe into `TierSpec` but no Zephyr consumer
  applied it (silently-dropped knob). The Rust `run_tiers` arm now self-applies
  `k_thread_cpu_pin` per tier (boot + spawned, mirroring the W5.5 deadline
  pattern, via the existing Phase-110.D `nros_zephyr_thread_cpu_pin` shim);
  an unhonorable pin (`CONFIG_SCHED_CPU_MASK_PIN_ONLY` off / bad cpu) warns
  loud and the tier runs unpinned.
- W5.8 — **C/C++ zephyr consumers + tier-spec policy append — ✅ DONE**
  (2026-07-23): the C/C++ zephyr tier image now applies BOTH kernel knobs.
  (a) `core_plus1` consumer: `zephyr_apply_core_pin` (tier task + boot) via
  the Phase-110.D shim, loud-warn on unhonorable. (b) Kernel EDF: the tier
  spec lacked the generic policy entirely, so the ABI was appended
  (append-only, W2 dance) with `tier_class`/`period_us`/`budget_us`/
  `deadline_us`/`deadline_policy` across ALL mirrors — `nros_native_tier_spec_t`
  (main.h), `NativeTierSpec` (main.hpp), `NativeTierSpecC` (nros-cpp), the 4
  board `nros_tier_spec_t` mirrors (zephyr/freertos/nuttx×2, freertos offset
  table extended to 96 B) — and BOTH entry emitters (emit_cpp/emit_c bake the
  5 literals). `zephyr_apply_tier_deadline` (tier task + boot) applies
  `k_thread_deadline_set` when `tier_class=="real_time" && deadline_us>0`,
  printing the `ZEPHYR_EDF_DEADLINE_MARKER` literal ONLY when the shim reports
  the kernel applied it (three-way marker lockstep: entry_tiers.rs +
  zephyr_run_tiers.c + output.rs). Gotcha: Zephyr `printk` returns void — an
  `int` extern is a conflicting-types build break. Compile proof: full zephyr
  west matrix green (C+C+++Rust images, 14-field initializers); zephyr_rust +
  EDF e2es green. UPDATE (2026-07-24, post-#245): the C/C++ consumers are
  now EXERCISED — ws-realtime-{cpp,c}'s model `high` tiers carry
  `class: real_time` + zephyr-scoped `deadline_us` + CONFIG_SCHED_DEADLINE,
  and `zephyr_edf_deadline_applied` is parametrized rust/cpp/c (kernel EDF
  applied in every language arm, marker-confirmed; serial group widened to
  the cpp/c realtime cells). #245 itself (the cells' timeout) was RESOLVED —
  executor storage 32 bytes short of the generated size, heap corruption;
  see `archived/0245-*`.
- W5.9 — **NuttX kernel sporadic server (budget dim Native) — consumers
  LANDED** (2026-07-24): `nros_nuttx_apply_current_sporadic(name, class,
  budget_us, period_us, priority)` in BOTH `nuttx_run_tiers.c` seams
  (self-apply on the calling thread: `pthread_setschedparam(SCHED_SPORADIC)`
  with `sched_ss_{low_priority,repl_period,init_budget,max_repl}`), called at
  tier-thread entry + boot on the C/C++ arm AND externed by the Rust
  `nros-board-nuttx::run_tiers` (both its sites) — one implementation, one
  marker (`nros: sporadic budget set tier=` = `NUTTX_SPORADIC_MARKER`,
  printed only when the kernel ACCEPTED the policy). Config-honest: a tier
  that declares budget+period on a kernel without `CONFIG_SCHED_SPORADIC`
  logs a loud "executor SchedContext only" note (the W3a cooperative
  Sporadic SC stays the enforcement). The helper lives in the board seam C
  so `struct sched_param`'s config-gated sporadic fields lay out per THIS
  kernel's config (the #131 layout-mirror trap avoided; Rust never mirrors
  the struct). `CONFIG_SCHED_SPORADIC=y` + `MAXREPL=3` added to both boards'
  `nuttx-config/defconfig` — takes effect at the NEXT kernel provision; the
  current prebuilt export lacks it, so the #else arm is what compiles today
  (the #ifdef arm is compile-verified only against the header fields, which
  match the export's `sched.h` exactly). All 3 nuttx lanes rebuild green;
  arm cpp/c realtime cells PASS (~13 s). **Not yet exercised end-to-end**
  (needs: a kernel re-provision with the config + a nuttx-scoped
  budget/period — `TierPlatformSpec` has no per-platform budget/period
  fields yet, and a GENERIC head budget would flip every platform's
  executor to Sporadic gating; the rlm schema addition is maintainer-gated
  — submodule push). #246 filed: the nuttx_arm_rust realtime cell times out
  PRE-EXISTING (baseline-verified); riscv trio precondition-skips.
- W5.9b — **sporadic server EXERCISED end-to-end — KERNEL-ACCEPTED** (2026-07-24):
  rlm `TierPlatformSpec` gained per-platform `budget_us`/`period_us` (rlm
  `6a8e287`, pushed — sub-table override, deadline_us precedent) +
  `tier_from_model` hoists the SELECTED platform's pair over the generic head
  (tripwire test extended: posix override wins; freertos falls back to head).
  ws-realtime-{rust,cpp,c} nuttx sub-tables declare `budget_us: 5000` /
  `period_us: 10000` — zephyr/posix bakes unchanged (sub-table scoped).
  `nuttx_sporadic_budget_applied` e2e: boots the cpp arm image and asserts
  the policy is NEVER silently dropped — kernel-accept marker OR the honest
  fallback note; measured **KERNEL-ACCEPTED (SCHED_SPORADIC live)** — the
  fixture lane builds the kernel from the board `nuttx-config/defconfig`, so
  the W5.9 `CONFIG_SCHED_SPORADIC=y` took effect without a separate
  provision. arm cpp/c realtime cells PASS unchanged. Gotchas: a poisoned
  cmake configure leaves a STALE generated TU that the next lane run
  silently reuses (delete the TU to force regen — #222 family); a raced
  regen left a corrupt `nros_config_generated.h` (duplicate tail) — purge
  the generated dir; `just setup-cli` may not rebuild after a submodule-only
  change (run `cargo build --release -p nros-cli` in packages/cli to be
  sure).
- W5.10 — **ThreadX preempt-threshold exercised (non_preempt_scope dim) —
  KERNEL-ACCEPTED** (2026-07-24): the transport + consumers already existed
  (W2 pipe + phase-297's `nros_threadx_create_task` create-time threshold +
  `nros_threadx_set_current_priority` boot reprioritize) but were dormant +
  silent. Added: kernel-accept-gated trace markers at both apply sites
  (`nros: preempt threshold set tier=` = `THREADX_PREEMPT_MARKER`, W5.5
  discipline); ws-realtime-rust declares `threadx.preempt_threshold: 10` on
  the LOW tier (scope: priorities 10-15 can't preempt telem mid-callback;
  ctrl@5 + transport threads unaffected — a threshold on the HIGH tier is
  NOT the demo vehicle while #247 is open); new
  `threadx_preempt_threshold_applied` e2e boots the threadx-linux image and
  asserts exactly 1 kernel-accepted marker — PASS. Serialized with the
  realtime cell (`threadx-realtime-rust-port` group). #247 filed: the
  threadx_linux_rust realtime cell's spawned-high-tier-publishes-zero is
  PRE-EXISTING (baseline-verified) — coordinate with phase-297.
- W5.11 — **Zephyr CPU-pin exercised (placement dim) — fail-loud e2e**
  (2026-07-24): the W5.7 (Rust) + W5.8 (C/C++) `k_thread_cpu_pin` consumers
  already applied the `core` knob but were UNTESTED — the one Native dim
  without a marker+e2e. Single-sourced both literals into
  `nros_tests::output` (`ZEPHYR_CORE_PIN_MARKER` = `nros: core pin tier=` and
  `ZEPHYR_CORE_PIN_FALLBACK_MARKER` = `nros: core pin FAILED tier=`; the
  accept/fallback share no prefix, so the e2e waits the `nros: core pin` stem
  then classifies) with lockstep comments in BOTH arms (entry_tiers.rs
  `::log::info!` + zephyr_run_tiers.c `printk`). ws-realtime-rust `low` tier
  declares `zephyr.core: 0`; new `zephyr_core_pin_applied` e2e asserts the
  placement dim is NEVER silently dropped — kernel-accept marker OR the honest
  fallback note (RFC-0052 fail-loud), the two-mode shape of
  `nuttx_sporadic_budget_applied`. The current single-CPU native_sim fixture
  has no `CONFIG_SCHED_CPU_MASK_PIN_ONLY`/SMP, so `k_thread_cpu_pin` returns
  `-ENOSYS` → the FALLBACK arm fires (loud, unpinned); an SMP fixture would
  flip it to ACCEPT and the same test upgrades automatically. Deliberately NOT
  enabling SMP on native_sim (it shares the image with the EDF/delivery cells
  — global scheduler change = regression risk for no proof gain here). The
  C/C++ arms print the same lockstepped literal (trivial future
  parametrization). zephyr_rust EDF + realtime cells unchanged (core:0 is a
  no-op on the unpinned run).
- W5.11 (NuttX half) — **NuttX SMP core-pin consumer + fail-loud e2e**
  (2026-07-24): the NuttX tier ABI carried `core_plus1` since W2 but had NO
  consumer — a declared `core` was SILENTLY dropped (worse than Zephyr's
  pre-W5.11 loud-warn). Added `nros_nuttx_apply_current_affinity(name,
  core_plus1)` to BOTH board seams (arm + riscv `nuttx_run_tiers.c`):
  `pthread_setaffinity_np` under `#ifdef CONFIG_SMP`, kernel-accept marker
  (`nros: core pin tier=` = `NUTTX_CORE_PIN_MARKER`) gated on `rc == 0`, LOUD
  fallback (`nros: core pin FAILED tier=` = `NUTTX_CORE_PIN_FALLBACK_MARKER`)
  on no-SMP/rejection. Called at spawned-tier entry (via the ctx, which gained
  a `core_plus1` field + copy) AND boot (safe on the session owner — a core pin
  doesn't cap CPU, so unlike the #246 sporadic budget it can't starve the
  shared flush). The Rust `nros-board-nuttx` externs it + self-applies
  (`apply_tier_affinity`, boot + spawned). ws-realtime-rust `low` tier declares
  `nuttx.core: 0`; new `nuttx_core_pin_applied` e2e (two-mode, the
  `nuttx_sporadic_budget_applied` shape) boots the RUST arm — measured HONEST
  FALLBACK (qemu-arm-virt is single-core). case_10 (#246 cell) unchanged
  (core:0 is a no-op unpinned).
- W5.11 (FreeRTOS half) — **FreeRTOS core-pin fail-loud + e2e; port-group fix**
  (2026-07-24): FreeRTOS HAD a `vTaskCoreAffinitySet` consumer but its
  uniprocessor branch was a SILENT `(void)task` (a declared `core` dropped with
  no trace). Made both branches LOUD over the semihosting console
  (`freertos_run_tiers.c` externs `semihosting_write0`): accept marker
  (`nros: core pin tier=` = `FREERTOS_CORE_PIN_MARKER`) on the
  `configUSE_CORE_AFFINITY` path, fallback (`nros: core pin FAILED tier=` =
  `FREERTOS_CORE_PIN_FALLBACK_MARKER`) on uniprocessor. ws-realtime-cpp-mps2
  `low` (spawned) tier declares `freertos.core: 0`; new `freertos_core_pin_
  applied` e2e boots the mps2-an385 image (semihosting captured via
  `-semihosting-config`) — measured HONEST FALLBACK (mps2 is uniprocessor).
  ALSO fixed a latent full-sweep flake in the W5.11 zephyr + nuttx halves: the
  new core-pin e2es share the realtime image's baked router port with their
  realtime_tiers cell but were NOT in a nextest port-serialization group (they
  passed only run-isolated). Added `nuttx-realtime-rust-port` +
  `freertos-realtime-cpp-port` groups and joined `zephyr_core_pin_applied` to
  the existing `zephyr-realtime-rust-port` group. The three placement-dim
  arms (zephyr/nuttx/freertos) now all fail loud, all e2e-verified.
- W5.13 — **ThreadX placement (`SMP core exclude`) consumer + fail-loud e2e**
  (2026-07-24): the 4th (last) RTOS placement arm. `nros_threadx_apply_current_
  core_exclude(core_plus1)` in the shared `threadx_hooks.c` shim self-pins the
  CALLING thread by EXCLUDING every other core via `tx_thread_smp_core_exclude`
  under `#ifdef TX_THREAD_SMP` (returns 1 on accept, 0 on no-SMP/rejection). The
  Rust `nros-board-threadx` externs it + self-applies (`apply_tier_core_exclude`,
  boot + spawned) and prints the accept marker (`nros: core pin tier=` =
  `THREADX_CORE_PIN_MARKER`) or the loud fallback (`… FAILED …` =
  `THREADX_CORE_PIN_FALLBACK_MARKER`). ws-realtime-rust `low` tier declares
  `threadx.core: 0`; new `threadx_core_pin_applied` e2e (two-mode, joined to the
  `threadx-realtime-rust-port` nextest group) boots the threadx-linux host image
  — measured HONEST FALLBACK (threadx-linux is non-SMP). case_15 + the preempt
  e2e (shared fixture) unchanged. Placement dim now fail-loud on ALL FOUR RTOSes.
- W5.15 — **derived-schedule `edf` knob sliced per target_rtos** (2026-07-24):
  `derive_execution_from_contracts` required the `edf` capability knob UNANIMOUS
  across ALL `execution.deploy` entries, so a legal mixed model (zephyr edf=true
  + freertos edf=false) bailed even though the two knobs describe two different
  images' kernels. `deploy_targets_rtos(deploy, target_rtos)` now gates the knob
  loop to entries relevant to THIS bake (MCU target → `board_to_rtos`, Linux →
  posix, unplaced → board-agnostic so the same-image split-brain rejection is
  preserved). Unit-tested (`edf_knobs_sliced_per_target_rtos`; the conflict test
  is preserved).
- W5.12 — **derived-schedule bake E2E through the full `codegen-system` verb**
  (2026-07-24): the capstone's bake half. `codegen_system_derives_tiers_from_
  contract_model` sets up a workspace whose committed model declares NO
  `execution.tiers`, only the CONTRACT layer (`node_paths` deadlines +
  `structure.topics`/`pub_endpoints` rates) plus the two pkgs' callback groups,
  runs the FULL `nros codegen-system --target native` verb, and asserts the
  emitted `nros-plan.json` carries resolved `derived-control_node` +
  `derived-telem_node` tiers with control (5 ms/100 Hz) ranked ABOVE telem
  (100 ms/10 Hz). This exercises derive → apply → resolve → plan through the
  verb, not just the `derive_execution_from_contracts` unit (W5.6). Once the
  plan carries the derived tiers, boot behavior is IDENTICAL to the
  authored-tier path every realtime cell already boots (same
  resolve→run_tiers), so the derivation correctness is the new-covered surface.
  FINDING: the pure-cargo `nros::main!(model=…)` proc-macro does NOT engage the
  derive path — it only converts EXPLICIT `execution.tiers` (main_macro.rs
  `if !model.execution.tiers.is_empty()`); a tier-less model bakes tier-less.
  So derived schedules are a `codegen-system` (C/C++/CMake) capability only —
  wiring derivation into the proc-macro is a separate follow-up (see below).

### Remaining work items (beyond W5.5–W5.13, W5.15, W5.12)

Explicit, individually actionable; each ends with an acceptance check. Two are
tracked as issues because they are limitations, not just unbuilt features.

- **Derived schedule in the pure-cargo `nros::main!` path.** The proc-macro
  converts explicit `execution.tiers` only; a contract-only model bakes
  tier-less on the Rust pure-cargo path (W5.12 finding). *Accept:* the macro
  derives tiers from contracts (reusing `nros-orchestration-ir`'s shared derive)
  when the model declares none, matching the `codegen-system` verb.
- **W5.12 runtime-boot of a derived image (optional).** The bake E2E proves the
  plan; a booted derived image would be behaviorally identical to authored-tier
  boot (shared resolve→run_tiers). Low marginal value; a native C/C++ derived
  fixture would close it if desired.
- **W5.14 — other-board `replenish` / native reservation.** The budget dim's
  replenishment + reservation primitives on the boards that lack them (today
  the executor's cooperative `SchedContext` backfills). *Accept:* each board
  either applies the native primitive (marker-gated) or records a loud
  `Backfill`/`Degrade`, never a silent drop.
- **Placement / non_preempt derivation from the model** — the realizer hardcodes
  both dims to `NotRequested`; the derived-schedule path can never assign a core
  pin or preemption threshold. Design-open (RFC-0052 contract vocabulary).
  Tracked: **issue #259**.
- **SMP kernel-ACCEPT coverage for the core-pin dim** — every realtime fixture
  is uniprocessor, so the SMP core-pin accept path is compile-verified only.
  Needs one SMP fixture to flip a two-mode e2e to accept. Tracked: **issue #260**.

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
  - **`ws-bridge-{rust,xrce}` — DONE (2026-07-21).** Both resolved with
    `--system` (1 bridge each → `execution.bridges`: zenoh→cyclonedds /
    zenoh→xrce), committed `demo_bringup/config/system_model.yaml`, swapped
    `nros::main!(launch=)` → `model = "demo_bringup"`. Native entries build on
    the model arm with the bridge backend baked (proves the model arm reads
    `execution.bridges` to register the two RMWs). `ws-{qos,custom-msg}-mixed`
    were already migrated (their `native_{talker,listener}_entry` use `MODEL`).
  - **`{c,cpp,mixed}` monolith native entries — DONE (2026-07-23).** All 21
    native/service/action/robot CMake entries swapped `LAUNCH` → `MODEL`
    (robots keep `HOST`); 18 models resolved (6 per workspace, 46.5 binary —
    no meta.record/companion; fixed the same `--host` XML-comment bug in each
    `multihost.launch.xml`). Fresh workspace-fixture rebuilds green for all
    three; robot host slices verified in the generated mains (robot1→talker
    only, robot2→listener only). **Seam fix:** the first C/C++ entry PAIR
    sharing one model (robot1/robot2 → multihost_model.yaml) hit ninja
    "defined as an output multiple times" — the codegen depfile's
    CONFIGURE_DEPENDS carried two `../` spellings of the same file;
    `NanoRosEntry.cmake` now REALPATH-canonicalizes each dep before appending.
  - **`ws-realtime-{c,c-mps2,cpp-fvp,cpp-mps2,cpp-rclcpp,cpp-subnode,
    cpp-subnode-portable}` — DONE (2026-07-23).** All 10 CMake entries swapped
    `LAUNCH` → `MODEL`. 5 workspaces batch-resolved with `--system` (tiers +
    bindings carried); `ws-realtime-c` + `cpp-fvp` hand-authored from the
    ws-realtime-cpp template — their multi-`[deploy.*]` system.tomls are
    per-BOARD pickers (cmake `DEPLOY`), not per-node placement, which the
    resolver refuses ("node not placed"), so like the rust flagship the model
    carries NO deploy layer and every board entry keeps all nodes. Native
    fixture rebuilds green (ws-realtime-c native, cpp-rclcpp, cpp-subnode,
    cpp-subnode-portable); embedded entries (nuttx×2/zephyr/freertos×2/fvp)
    are the same board-agnostic MODEL seam, validated in their platform lanes.

### Remaining migration + retirement jobs (2026-07-23 inventory)

The classifier (entry files whose non-comment lines still carry `LAUNCH "…"`
/ `launch = "…"`) now reports **17 CMake + 7 Rust** holdouts — every one
grouped below. Standalone examples (`examples/native`, `examples/qemu-*`,
`examples/zephyr`, `examples/threadx-linux`, `examples/px4`) use plain
`nano_ros_add_executable(<target> <srcs>)` with hand-written mains — no
launch/model bake at all — and are NOT migration targets.

**M1 — monolith embedded entries — SWAPPED + LANE-VALIDATED (2026-07-23).**
All 15 entries migrated (`{c,cpp,mixed}` × {freertos, threadx, zephyr} + c
nuttx CMake; rust `esp32`/`qemu_freertos`/`qemu_nuttx`/`threadx_linux`/
`zephyr`). Platform lanes: **9/12 green** (threadx-linux ×4 langs, freertos
c/cpp/mixed, nuttx-c, esp32-rust). The 3 red rust lanes are **pre-existing
infra breakage, proven independent of the migration** (freertos-rust reruns
with the OLD `launch =` form and fails identically):
`freertos-rust` — `NROS_PLATFORM_FREERTOS_SRC not set` (env overlay);
`nuttx-rust` — `ld: cannot find -lopenamp/-lboard` (NuttX SDK export libs
absent); `zephyr-rust` — `package ID zephyr_entry did not match any packages`
(lane package-graph plumbing). Those lanes were broken on main before the
swap (cf. the broad-build-blockers memory); fixing them is lane infra work,
not R4.

**M2 — templates (3 CMake + 1 Rust):** `multi-node-workspace` (rust),
`multi-node-workspace-cpp`, `pure-c-workspace`, `c-and-cpp-mixed-workspace`
robot entries. Templates are user-facing scaffolds — swap + commit a model +
update the template README so `nros new` users start on the model path.

**M3 — blocked pair:** `ws-safety-{c,cpp}` — **DONE (2026-07-24, #237
resolved)**: the "blocker" was only the ad-hoc plain-cmake validation route;
migrated via fix option 1 (4 entries → per-variant
`safety_{talker,listener}_model.yaml`, validated through the fixture
builder's `-safety-*` rows, both native lanes green). **`zephyr_entry_robot1`
— DONE (2026-07-24, #236 fully resolved)**: rlm `6d64202` makes
`Deploy.target` an `Option` (machine-only deploys UNPLACED, no `target:`
key); the macro/CLI slices treat `None` as board-agnostic (host filter
partitions; `model_unplaced_target_is_board_agnostic` +  play_launch's
multihost golden pin it). zephyr slice bakes talker-only; native robots
unchanged. **Every example consumer is now on the model path** — the only
remaining `launch =` users are the M4 test fixtures + M5 book prose.

**M4 — test fixtures: CLASSIFIED, flips atomically with R-code.1
(2026-07-24).** Every remaining `launch =`/system.toml fixture TESTS the
legacy path itself — `n9_workspace` → `native_main_macro_forms/misuse` (the
launch-arm macro forms), `o4_pkg_index_workspace` → `pkg_index` (resolution
through the launch arm), `orchestration_tiers_*` → `exec_model_matrix` +
orchestration misuse (the launch+system.toml tier bake), and
`multi_pkg_workspace_{zephyr,nuttx,esp_idf,platformio}` → `cli_bringup_*`
(the system.toml CLI bake). Migrating them now would gut the tests they
serve; the R-code.1 commit rewrites/deletes these tests+fixtures together
(launch-form tests become model-form or misuse-error tests).

**M5 — book + docs — DONE (2026-07-24).** All five pages now teach the
model workflow as canonical (`model =`/`MODEL` primary, the resolve command
was already documented, layout comments + the zephyr/C++/mixed snippets
synced, host-slice form added); the launch forms remain only as explicitly
DEPRECATED examples that R-code.1 deletes.

**Retirement (code) — after M1–M5, R3 warning fires nowhere:**

- **R-code.1** Delete the legacy arms — **entry side DONE (2026-07-24)**:
  `nros::main!(launch = …)` is a compile error with the migrate recipe (arm +
  launch-only helpers deleted, −531 lines; the bridge emit was ported to the
  model arm reading `execution.bridges` — it had lived ONLY in the launch arm,
  so a model bridge entry compiled but emitted a plain main); `codegen entry
  --launch`/`--args` are removal errors; the CMake `LAUNCH` keyword is a
  FATAL_ERROR in both fns. **Remaining (next slice): the codegen-system /
  system.toml bake.** Callers that must flip to `--model` first:
  `scripts/build/workspace-fixtures-build.sh:127` (`codegen-system --bringup`
  for every C/C++ workspace fixture — this is why the R3 warning still fires
  in fixture builds), the west `multi_pkg_workspace_{zephyr,nuttx,esp_idf,
  platformio}` fixtures (`cli_bringup_*` consumers), `nros plan`'s launch
  resolution, and `launch_synth` itself. Then delete the system.toml bake +
  `launch_synth` + the R3 `deprecation` module (R-code.3) in one test-green
  change.
- **R-code.2 — MODEL becomes the DEFAULT, not a required arg.** End state:
  users never spell it. Convention discovery — `nros::main!()` (no arg) and
  `nano_ros_add_executable(... )` (no MODEL) resolve
  `<bringup>/config/system_model.yaml` via the entry pkg's bringup dep
  (package.xml exec_depend, same lookup the `"demo_bringup"` shorthand
  already does); explicit `MODEL`/`model =` stays as the override for
  multi-model bringups (the file-override form) and `host =` slicing.
  Missing model file = fail-loud with the resolve command to run.
- **R-code.3** Deprecation plumbing removal: the R3 `nros_cli_core::
  deprecation` warning + `NROS_ALLOW_LEGACY_BAKE` escape hatch go away with
  the arms they guard.

### launch_synth endgame (validated 2026-07-24)

Caller census after the model-first waves: THREE `resolve_launch` sites
remain, all fallback-only — `codegen-system` (configless self-pkg synth;
model branch bypasses), `nros plan` (modelless fallback, R3-warned), and
`ws sync` bridge planning (modelless fallback). Deletion preconditions:

1. **Composable record synthesis** — `plan_record_from_model` emits empty
   `container`/`load_node`; before any composable bringup carries a model,
   extend it to map `NodeInstance.container`/`plugin` into the record's
   container + load_node arrays (else containers silently drop from plans).
2. **orchestration_* fixture strategy — DISSOLVED (2026-07-24).** All four
   remaining tests (conditionals/includes/set_remap_env/e2e) drive committed
   pre-baked `--record` files — they never touch the parser or launch_synth
   at run time and survive the deletion untouched (they exercise
   planner-consumes-record, exactly the plumbing model mode reuses).
   orchestration_composable flipped to a committed model with #1.
3. **Self-pkg synth → model synth — DONE (2026-07-24).**
   `synthesise_self_model` (the model twin of synthesise_xml, same
   discovered pkg/exec inputs) + a plan.rs self-bringup branch: a launchless
   self-pkg dir plans through a synthesized 1-node model via the record
   plumbing — no parser round trip, no R3 warning. 475/475 cli-core tests.

**DELETION LANDED (2026-07-24) — R-code COMPLETE.** The launch-XML parse
path is gone end to end: planner requires a record (committed `--record`,
model-synthesized, or resolve output; launch-arg overrides on pre-resolved
input fail loud — early binding), `parse_launch_file_record` + the M-F.20
ament-prefix synthesis deleted, the plan/ws/codegen-system launch fallbacks
are hard errors carrying the resolve recipe, `launch_synth` slimmed 949 →
~400 lines (the self-bringup discovery + model-synth kernel remains), and
the R3 `deprecation` module (+ `NROS_ALLOW_LEGACY_BAKE`) is deleted with
zero callers. `plan_pipeline_e2e` re-targeted to the model pipeline
(dir-input discovery; fixture gains its committed model). The `nros` CLI
neither parses nor synthesises launch XML anywhere; `nros-launch-parser`
survives only for the separate `nros-build`/`generate_run_plan` build.rs
compat surface (its own retirement track).
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
