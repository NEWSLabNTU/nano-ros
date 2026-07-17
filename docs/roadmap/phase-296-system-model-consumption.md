# Phase 296 — SystemModel consumption: bake the model into embedded images

Implements RFC-0050 (consumer half) + RFC-0052 (the RTOS mapper).
Producer side is DONE (play_launch phase 43: `resolve` emits the model,
the Linux runtime consumes it; shared schema in the vendored
`ros-launch-manifest` `model`/`sched` crates, already pinned in
`packages/cli/third-party/`).

Status: planned.

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

### W3 — on-target contract monitors (layer 2)

- Stamp extraction: codegen emits per-type `stamp_offset` const for
  Header-leading types; take path computes `now - stamp` when the baked
  monitor table declares `max_age_ms`.
- Publish rate/jitter accounting per endpoint, checked on spin ticks;
  node-path latency via the sched-context monotonic clock.
- Violations → `nros-diagnostic-updater`, play_launch rule-id vocabulary
  (`max-age-runtime`, `rate-hierarchy-runtime`, `max-latency-runtime`),
  assumption/guarantee tagged.
- Zero-cost when uncontracted (empty const table → DCE); measure flash
  delta on mps2-an385 to prove it.
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
