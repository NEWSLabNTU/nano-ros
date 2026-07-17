# Phase 296 — SystemModel consumption: bake the model into embedded images

Implements RFC-0050 (consumer half) + RFC-0052 (the RTOS mapper).
Producer side is DONE (play_launch phase 43: `resolve` emits the model,
the Linux runtime consumes it; shared schema in the vendored
`ros-launch-manifest` `model`/`sched` crates, already pinned in
`packages/cli/third-party/`).

Status: W1+W2+W3a landed (2026-07-17); W3b-W4 planned.

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

Exploration (2026-07-17) found three blockers, in dependency order:
1. **No Rust diagnostics surface exists** — nros-diagnostic-updater is a
   C++ header only; no diagnostic_msgs in-tree. The monitors have
   nothing to report through. Build first: generate diagnostic_msgs +
   a minimal Rust DiagnosticArray publisher on /diagnostics.
2. **No epoch clock on embedded** — `ExecutorConfig.clock_us` is
   monotonic; `now - header.stamp` needs a wall-clock hook beside it.
3. **Examples never stamp `header.stamp`** — the parity fixture must
   stamp on publish.
Recommended first slice: publish-rate monitor (monotonic-only, seam at
handles.rs publish_with_buffer) + baked `{topic → min_rate_hz}` const
table from codegen-system, then max_age via STAMP_OFFSET once 2+3 land.
Also here: TT-window binding (needs the major-frame dispatcher at the
run_tiers altitude) and deadline-policy miss ACTIONS (ignore/warn/skip/
fault — note the executor's existing DeadlinePolicy enum is deadline
INHERITANCE, a different concept; the miss-action enum is new).

- Take path `max_age` via STAMP_OFFSET; rate/jitter per spin tick;
  node-path latency via the sched-context clock; violations through the
  (new) Rust diagnostics surface in play_launch rule-id vocabulary;
  zero-cost when uncontracted (empty const table → DCE, flash delta
  measured on mps2-an385).
- **Done when:** a native fixture pair with a violated `min_rate_hz`
  reports the violation through diagnostics while the compliant twin
  stays silent — same contract file checked by play_launch on the Linux
  side (cross-runtime parity test).

### W4 — CMake + ASI pilot

- `nano_ros_add_executable(... MODEL <path>)` (RFC-0048 verb surface)
  driving model-mode codegen-system.
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
