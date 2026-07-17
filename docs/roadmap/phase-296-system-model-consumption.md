# Phase 296 — SystemModel consumption: bake the model into embedded images

Implements RFC-0050 (consumer half) + RFC-0052 (the RTOS mapper).
Producer side is DONE (play_launch phase 43: `resolve` emits the model,
the Linux runtime consumes it; shared schema in the vendored
`ros-launch-manifest` `model`/`sched` crates, already pinned in
`packages/cli/third-party/`).

Status: W1+W2+W3a+W3b.1-.4(machinery) landed (2026-07-17); W3b.4 parity fixture + W3b.5 + W4 remain.

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

#### W3b.4 — rate monitor + baked monitor table (machinery LANDED; parity fixture remains)

Landed: executor/monitor.rs (MonitorSpec + PubMonitorCell statics +
pure check_rate with window/dedup/recovery semantics, unit-tested),
Executor::set_monitor_table/drain_violations + spin-tick hook (one
branch when the table is empty), NodeHandle::set_monitors +
create_publisher cell attach (both constructor sites), relaxed-atomic
publish bump. Remaining here: the codegen-system emission of the table
from model contracts + the native parity fixture vs play_launch.

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
- **Done when:** native pair with violated `min_rate_hz` reports
  `rate-hierarchy-runtime` on `/diagnostics`; compliant twin silent;
  play_launch flags the same violation from the same contract file
  (the cross-runtime parity test).

#### W3b.5 — max_age + node-path latency (machinery LANDED; parity fixture remains)

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
- **Remaining (with the W3b.4 remainder):** the native parity fixture —
  violated `min_rate_hz` + stale-stamp (`max_age`) pair vs play_launch
  on the same contract file.

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
- R2 — migration: ASI pilot (W4.3) + in-tree workspace examples
  (`ws-realtime-rust` first) build from resolved models; book chapters
  switch to the resolve→bake flow.
- R3 — deprecation: `codegen-system` WITHOUT `--model` and `nros plan`'s
  launch-XML path warn; `launch_synth` marked deprecated. One release of
  overlap.
- R4 — retirement: legacy path removed; `nros codegen-system` requires a
  model (or is folded into a slimmer `nros bake`). RFC-0004/0015/0032
  updated to point at the model pipeline; superseded sections archived.
