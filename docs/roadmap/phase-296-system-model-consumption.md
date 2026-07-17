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

#### W3b.5 — max_age + node-path latency

- Take-path peek via `RosMessage::STAMP_OFFSET` (landed W3a) +
  W3b.2 epoch hook → `max-age-runtime`; node-path
  (take→publish) latency via the sched-context monotonic clock →
  `max-latency-runtime`.
- TT-window binding (major-frame dispatcher at the run_tiers altitude)
  and the deadline-miss ACTION enum (ignore/warn/skip/fault — distinct
  from the executor's existing DeadlinePolicy inheritance enum) land
  here too, closing the W2 deferral.
- **Done when:** parity fixture extends to a stale-stamp scenario
  (max_age fires both runtimes) and a deadline_policy=skip tier
  provably skips the offending group's remaining callbacks.

### W4 — CMake + ASI pilot

- W4.1 — `nros codegen entry --model`: build the entry plan from the
  model's STRUCTURE layer (nodes/pkg/class from `[[component]]`-shaped
  metadata; the launch parser is bypassed — the model IS the resolved
  launch). Blocked-on nothing; the scoping fact from W3a is that the
  `LAUNCH` keyword drives `codegen entry`, not codegen-system.
- W4.2 — `nano_ros_add_executable(... MODEL <path>)` keyword: mutually
  exclusive with LAUNCH; passes `--model` to BOTH `codegen entry`
  (W4.1) and `codegen-system` (landed W1) invocations.
- W4.3 — ASI pilot: replace `controller_pkg:system.launch.xml` with a
  play_launch-resolved model on the zephyr-fvp lane; validate on AVH.
  Needs the model resolved for `--target zephyr` (play_launch side is
  target-generic already).
- ASI pilot: replace `controller_pkg:system.launch.xml` with a
  play_launch-resolved `system_model.yaml` for the zephyr-fvp lane;
  validate on AVH.
- **Done when:** the ASI actuation image builds from a resolved model and
  the FVP/AVH smoke passes.

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
  - **N (nano-ros):** N1 monitor-table emission from model contracts
    (W3b.4 tail); N2 `codegen entry --model` (W4.1) incl. the
    plugin=class mapping + resolved-wiring remap verification; N3
    boot/transport bake reads `execution.transports` instead of
    `[[transport]]`.
- R2 — migration: ASI pilot (W4.3) + in-tree workspace examples
  (`ws-realtime-rust` first) build from resolved models; book chapters
  switch to the resolve→bake flow.
- R3 — deprecation: `codegen-system` WITHOUT `--model` and `nros plan`'s
  launch-XML path warn; `launch_synth` marked deprecated. One release of
  overlap.
- R4 — retirement: legacy path removed; `nros codegen-system` requires a
  model (or is folded into a slimmer `nros bake`). RFC-0004/0015/0032
  updated to point at the model pipeline; superseded sections archived.
