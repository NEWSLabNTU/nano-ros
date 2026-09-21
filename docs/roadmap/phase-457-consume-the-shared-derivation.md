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

## Parallel plan

One claim per wave (`just claim <id>`; claims are advisory, expire after the
TTL, and an open PR supersedes them). `owns` is the set of files a wave edits;
two waves with disjoint `owns` cannot conflict. Every path below exists in the
tree today unless marked `(new)`.

| claim id | depends on | owns | gate | starts now? |
| --- | --- | --- | --- | --- |
| `phase-457-W1` | rlm R4 (tag v0.1.37); play_launch `phase-78-W4` (the 0.12.0 release) | the `ros-launch-manifest` pin lines in `packages/cli/nros-cli-core/Cargo.toml`, `packages/core/nros-orchestration-ir/Cargo.toml`, `packages/core/nros-macros/Cargo.toml`, `packages/testing/nros-tests/Cargo.toml`; `Cargo.lock`; the play_launch gitlink `packages/cli/third-party/play_launch`; the `#[cfg(test)]` module of `packages/core/nros-orchestration-ir/src/derive.rs` (`contract_model()` at :192 and below); the test module of `packages/cli/nros-cli-core/src/orchestration/model_ingest.rs` (`contract_model()` at :1144); model literals in `packages/testing/nros-tests/tests/*.rs` | `cargo test -p nros-orchestration-ir` and `cargo test -p nros-cli-core` compile and pass on the new pin | no |
| `phase-457-W2` | `phase-457-W1` | `packages/core/nros-orchestration-ir/src/mapper_input.rs` (the whole file); the degradation print beside the derive call in `packages/cli/nros-cli-core/src/cmd/codegen_system.rs` (:433 and the lines that print `degradations`) | `cargo test -p nros-orchestration-ir`; `git grep -n min_rate_hz packages/core/nros-orchestration-ir` returns nothing | no |
| `phase-457-W3` | `phase-457-W2`; rlm R3 (the parity snapshot rlm ships) | `packages/testing/nros-tests/tests/contract_derived_chain_parity.rs` (new); its fixture model under `packages/testing/nros-tests/fixtures/` (new file) | `cargo test -p nros-tests --test contract_derived_chain_parity`; `bash scripts/check-no-tracked-models.sh` | no |
| `phase-457-W4` | `phase-457-W2`; `phase-459-W4` for ORDER only (it changes `realize_rtos` first; see below) | `node_facts` in `packages/core/nros-orchestration-ir/src/rtos_realizer.rs` (:256-:300) and one test in its `#[cfg(test)]` module | `cargo test -p nros-orchestration-ir` (the `max_response_ms` -> `k_thread_deadline_set` test) | no |

No wave of this phase starts today: W1 waits on two releases in other
repositories, and W2-W4 wait on W1. Nothing here is split; W1 is one claim
because the two pins must move in one commit (see Limits, "the window between
pins").

**Files two waves touch, and the order they serialise in.** Across this phase,
phase-459, phase-462, the other session's phases 460/461/463 and play_launch
phase 78:

- `packages/core/nros-orchestration-ir/src/mapper_input.rs`: `phase-457-W2`
  rewrites it; `phase-459-W7` only READS the trigger rate through it and edits
  its own fixture contract. Order: `phase-457-W2`, then `phase-459-W7`.
- `packages/core/nros-orchestration-ir/src/derive.rs`: the body above
  `#[cfg(test)]` (:179) belongs to phase-459 (`phase-459-W1`, `-W2` call
  into it, `phase-459-W4` changes the one `realize_rtos` call at :86,
  `phase-459-W3` names derived tiers); the test module belongs to
  `phase-457-W1`. Order: `phase-459-W1`, `phase-459-W2`, `phase-459-W4`,
  `phase-457-W1`, `phase-459-W3`. `phase-457-W2` leaves `derive.rs:57`
  calling `mapper_input_from_model_with_wcet` and does not edit the file.
- `packages/cli/nros-cli-core/src/orchestration/model_ingest.rs`:
  `phase-459-W1` (the `load_workspace_metadata` reader at :344, only if its
  signature must change), `phase-457-W1` (the test fixture at :1144),
  `phase-459-W3` (the binding refusals at :108 and :197-:212),
  `phase-462-W2` (new `on_violation` rows beside `monitor_rows` :1336).
  Order: `phase-459-W1`, `phase-457-W1`, `phase-459-W3`, `phase-462-W2`.
- `packages/core/nros-orchestration-ir/src/rtos_realizer.rs`:
  `phase-459-W4` (`rank_to_priority` :336, `realize_rtos` :349,
  `sched_caps_for` :140) before `phase-457-W4` (`node_facts` :256-:300).
- `packages/cli/nros-cli-core/src/cmd/codegen_system.rs`: `phase-459-W5`
  (`resolve_target_block` :194), `phase-459-W3` (the "declared tiers always
  win" test at :433), `phase-457-W2` (the print beside it). Order:
  `phase-459-W5`, `phase-459-W3`, `phase-457-W2`. phase-460 W4 edits the
  domain check at :856-:858, a disjoint region; the other session states
  where it lands relative to these three.
- The four `Cargo.toml` pin lines and `Cargo.lock`: `phase-457-W1` bumps to
  v0.1.37 once; `phase-459-W3` bumps again, later, to the first tag that
  carries the `derived` key. Never in the same commit.
- `packages/cli/third-party/play_launch` (the gitlink): `phase-457-W1` only.

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
pin (`model_provenance_stale`, ws.rs) and `nros sync` re-resolves it; that is
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

### W1 - pins and fixtures

Bump both pins. `derive.rs:192 contract_model()` and every model literal in
`nros-tests` that spells a timer as `input: vec![]` plus a `min_rate_hz` gains
`trigger: Some(EffectiveTrigger::Timer { rate_hz })` instead; a fixture that
keeps the old spelling is asserting the reconstruction this phase deletes, and
must fail.

Claim: phase-457-W1. Depends on: rlm R4 (tag v0.1.37), play_launch phase-78-W4 (the 0.12.0 release). Owns: the ros-launch-manifest pin lines in packages/cli/nros-cli-core/Cargo.toml, packages/core/nros-orchestration-ir/Cargo.toml, packages/core/nros-macros/Cargo.toml, packages/testing/nros-tests/Cargo.toml; Cargo.lock; packages/cli/third-party/play_launch; the #[cfg(test)] module of packages/core/nros-orchestration-ir/src/derive.rs; the test module of packages/cli/nros-cli-core/src/orchestration/model_ingest.rs; model literals in packages/testing/nros-tests/tests/*.rs. Gate: cargo test -p nros-orchestration-ir and cargo test -p nros-cli-core on the new pin. Status: not started.

### W2 - the call

Replace the body of `mapper_input.rs` as above. The `WcetProfile ->
DeriveFacts` conversion keeps rlm's boundary identity (`"<node fqn>/<path>"`,
the key `boundaries_without_wcet` reports).

Claim: phase-457-W2. Depends on: phase-457-W1. Owns: packages/core/nros-orchestration-ir/src/mapper_input.rs; the degradation print beside the derive call in packages/cli/nros-cli-core/src/cmd/codegen_system.rs. Gate: cargo test -p nros-orchestration-ir; git grep -n min_rate_hz packages/core/nros-orchestration-ir returns nothing. Status: not started.

### W3 - parity

`nros-tests` gains rlm's `contract_derived_chain` fixture model and asserts
`format!("{:?}", chain_aware_rank(&input))` equals the snapshot rlm ships
beside its own rank-vs-realize parity test. The same test asserts the island
model ranks its four timer paths by period (30 Hz before 10 Hz) with `trigger`
present and NO `min_rate_hz` in the model at all - the contract's promises
deleted from the fixture, so the schedule is shown to come from the timers.

Claim: phase-457-W3. Depends on: phase-457-W2, rlm R3 (the parity snapshot). Owns: packages/testing/nros-tests/tests/contract_derived_chain_parity.rs (new); its fixture model under packages/testing/nros-tests/fixtures/ (new). Gate: cargo test -p nros-tests --test contract_derived_chain_parity; bash scripts/check-no-tracked-models.sh. Status: not started.

### W4 - the deadline fold

Nothing to write: `deadline_us` now folds `srv_endpoints.max_response_ms` in
the shared function. `node_facts` in the realizer reads `MapperNode.deadline_us`
when set, its own fold of the paths otherwise, and a test pins that a
`max_response_ms` reaches `k_thread_deadline_set`. Closes the first bullet of
phase-434's "still open".

Claim: phase-457-W4. Depends on: phase-457-W2; phase-459-W4 for order on rtos_realizer.rs. Owns: node_facts in packages/core/nros-orchestration-ir/src/rtos_realizer.rs and one test in its #[cfg(test)] module. Gate: cargo test -p nros-orchestration-ir. Status: not started.

## Gates

- `cargo test -p nros-orchestration-ir` - W1 fixtures, W3 parity, W4 fold.
- `just check no-tracked-models` still green: the re-resolved island model is
  a build artifact, and W3's fixture model is a test input under
  `nros-tests`, hashed into the test, not a model under `build/`.
- `git grep -n min_rate_hz packages/core/nros-orchestration-ir` returns
  nothing. The runtime readers of `min_rate_hz` - `nros-node`'s
  `PubMonitorCell`, `queue_depth.rs`, `entity_inventory.rs` - are promises
  checked at run time and stay.
- `nros sync` on the island prints `derived-schedule note` for zero paths
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
