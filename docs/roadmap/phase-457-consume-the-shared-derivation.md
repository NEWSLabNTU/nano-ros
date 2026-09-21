# Phase 457 - consume the shared mapper-input derivation, keep the realizer

**Status (2026-09-21). Planned; nothing landed.** Numbered 457 rather than 456
on purpose: 455 was the highest phase on `main` when this was written and a
concurrent branch may take 456; a clash in numbers is worse than a gap.
Consumer side of ros-launch-manifest design issue #52
(`docs/design-issues.md`) and play_launch phase 78
(`docs/roadmap/phase-78-one-derivation-two-consumers.md`). Successor to
[phase-434](phase-434-contract-seams-closed.md), which closed the `miss`,
`max_jitter_ms` and `node_concurrency` seams and left this one, and to
[phase-296](phase-296-system-model-consumption.md) W5, which wrote
`mapper_input.rs` in the first place.

## What was open

RFC-0050's 2026-07-20 split ("input model; the algorithm is shared, not the
output") also said "derivation stays per-consumer", and this side wrote its
own. Verified against `783cdfa14`, rlm `ea5cbea` and play_launch `5eaa3191`:

- `nros-orchestration-ir/src/mapper_input.rs:72-81` decides that a path with
  an empty `input` is a timer and takes its rate from the FIRST output's
  `pub_endpoints[].min_rate_hz`, 0.0 when there is none. The resolver lowers
  Timer, Once, Spontaneous and Unclassified alike to `input: []` and drops
  `trigger.timer.rate_hz` (play_launch `model_builder.rs:954-960`), so this
  side cannot tell a 10 Hz control loop from a `once` map loader, and a timer
  whose output promises no rate has period `None` and never ranks. On the
  island the four timer outputs all promise the timer's rate, which is why
  the derived order is right - by the presence of a redundant declaration
  the resolver itself flags 14 times as `derivable-min-rate`.
- `:155` passes `chains: Vec::new()`. play_launch derives `ResolvedChain`s
  from scope paths and the global graph's critical path
  (`sched_derive.rs:251`); here `chain_aware_rank` never sees a chain, so
  the "chain-aware" tier order is the criticality bucket fallback on every
  image this repository has ever built.
- `:130` reads the criticality LABEL. play_launch ranks by the hazard-derived
  criticality (its phase 72) and writes only the label into the model, so a
  node whose criticality comes from a hazard is High there and `None` here.
- `:140` claims concurrency when "some path is outside every exclusive set";
  play_launch claims it when "no merged group covers every path". The two
  disagree on `exclusive: [[a, b], [c]]` over three paths.
- `rtos_realizer.rs:268` derives the deadline from paths only; play_launch's
  `deadline_us` also folds a service's `max_response_ms`, the field
  phase-434 listed as "carried and read by nothing here".

Six facts, two answers each. The tier a node gets on Zephyr and the
`SCHED_FIFO` priority it gets on Linux are computed from different inputs and
agree by coincidence.

## What closes it

**Pins.** `ros-launch-manifest` v0.1.35 -> v0.1.37 (four Cargo.toml files:
`nros-cli-core`, `nros-orchestration-ir`, `nros-macros`, `nros-tests`) and the
play_launch gitlink `07f0461e` (v0.9.0-158) -> the 0.12.0 tag, so that
`nros-launch-resolve` emits the fields the shared function reads. The
`system_model.yaml` under `build/nros/models/` goes stale with the resolver
pin (`model_provenance_stale`, ws.rs) and `just sync` re-resolves it; that is
the designed path, not a special step.

**The model.** rlm v0.1.37 carries, per path, `trigger` (a
`sched::EffectiveTrigger`, adjacent-tagged `kind`/`value`), `sync`,
`min_latency_ms`; per subscription, `buffer`; per contract, `severity_levels`
and the EFFECTIVE `node_criticality`. `trigger: None` means Unclassified and
is never a timer; the reconstruction rule above is not kept as a fallback.

**Mapper input.** `mapper_input.rs` becomes a `DeriveFacts` builder and one
call:

```text
let facts = DeriveFacts { path_exec_ms: wcet.map(profile_exec_ms).unwrap_or_default(), ..Default::default() };
let (input, report) = ros_launch_manifest_derive::mapper_input_from_model(model, &facts);
```

Deleted: `pub_rate_hz`, `node_paths_for`, `parse_criticality`, the local
`claims_concurrency`, and `rank_from_model`'s private copy of the pipeline.
`mapper_input_from_model_with_wcet` stays as the entry point `derive.rs:57`
calls, now a thin wrapper. `report.paths_without_trigger` is printed by
`codegen-system` beside the degradations: a path the model left unclassified
is said once, by name, never ranked silently.

**What stays here, unchanged in role.** `realize_rtos` (`rtos_realizer.rs:349`)
over the shared `RankedPlan` AND the shared `MapperInput` - it reads
`node_facts` from the input, which is why the input must be the same object
play_launch ranks from, not merely the same type. `SchedCaps`,
`Degradation`, `TierRtosSpec`, the callback-group gate (`derive.rs:97`), the
`[wcet]` profile selection, `rtos_plan_to_tier_table`. RFC-0079's priority
plan is untouched.

## Waves

- **W1 - pins and fixtures.** Bump both pins. `derive.rs:192
  contract_model()` and every model literal in `nros-tests` that spells a
  timer as `input: vec![]` plus a `min_rate_hz` gains `trigger:
  Some(EffectiveTrigger::Timer { rate_hz })` instead; a fixture that keeps
  the old spelling is asserting the reconstruction this phase deletes, and
  must fail.
- **W2 - the call.** Replace the body of `mapper_input.rs` as above. The
  `WcetProfile -> DeriveFacts` conversion keeps rlm's boundary identity
  (`"<node fqn>/<path>"`, the key `boundaries_without_wcet` reports).
- **W3 - parity.** `nros-tests` gains rlm's `contract_derived_chain` fixture
  model and asserts `format!("{:?}", chain_aware_rank(&input))` equals the
  snapshot rlm ships beside its own rank-vs-realize parity test. The same
  test asserts the island model ranks its four timer paths by period (30 Hz
  before 10 Hz) with `trigger` present and NO `min_rate_hz` in the model at
  all - the contract's promises deleted from the fixture, so the schedule is
  shown to come from the timers.
- **W4 - the deadline fold.** Nothing to write: `deadline_us` now folds
  `srv_endpoints.max_response_ms` in the shared function. `node_facts` in the
  realizer reads `MapperNode.deadline_us` when set, its own fold of the paths
  otherwise, and a test pins that a `max_response_ms` reaches
  `k_thread_deadline_set`. Closes the first bullet of phase-434's "still open".

## Gates

- `cargo test -p nros-orchestration-ir` - W1 fixtures, W3 parity, W4 fold.
- `just check-no-tracked-models` still green: the re-resolved island model is
  a build artifact, and W3's fixture model is a test input under
  `nros-tests`, hashed into the test, not a model under `build/`.
- `git grep -n min_rate_hz packages/core/nros-orchestration-ir` returns
  nothing. The runtime readers of `min_rate_hz` - `nros-node`'s
  `PubMonitorCell`, `queue_depth.rs`, `entity_inventory.rs` - are promises
  checked at run time and stay.
- `just sync` on the island prints `derived-schedule note` for zero paths
  without a trigger, and the four `derived-*` tiers (when callback groups are
  declared) carry the same order `play_launch check --explain` prints for the
  same contract.

## Limits

- **Callback groups still gate the tier.** A ranked node with no declared
  groups stays on the default tier (`derive.rs:97`). The island declares
  none, so the shared ranking changes what `codegen-system` PRINTS on the
  island and not, yet, what it bakes. That gate is phase-296 W5's and RFC-0032
  section 5.1's, and it is right; this phase only makes the thing it gates
  correct.
- **WCETs remain this side's fact.** `DeriveFacts.path_exec_ms` is filled
  from the `[wcet]` profile; no Linux `budget_us` reaches an image, and no
  profile has ever been measured (QEMU has no DWT). `ChainFeasibleWithoutWcet`
  keeps saying so.
- **The window between pins.** A model resolved by play_launch 0.11.0 has no
  `trigger`; against rlm v0.1.37's function it ranks nothing. W1 moves both
  pins in one commit so no build in this repository lives in that window;
  the note from `DeriveReport` is for a stale `build/` tree, which
  `model_provenance_stale` already refuses to continue with.
- **Cross-scope checks are not moved.** The checker's rate derivation and the
  fault-reaction route stay in play_launch on `ManifestIndex`; this side still
  cannot run them on a model alone. Play_launch phase 78's Limits name the
  second port that would change that.
