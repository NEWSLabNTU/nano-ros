# phase-462 -- the contract's safety vocabulary reaches the target

**Status (2026-09-21). NOT STARTED. Future work, planned only; no wave has an
owner and nothing here is in flight.** Numbered highest + 7 (phase-461 took +6;
456-460 belong to concurrent sessions). Depends on two pieces of work outside
this document: the shared MapperInput derivation phase (a play_launch / rlm
phase being designed in parallel, referred to here by that name and not by a
number) and the tier-derivation fix (the `derive.rs:93-100` gate that lets a
groupless node fall to the default tier with only a note).

## Parallel plan

One claim per wave (`just claim <id>`; claims are advisory, expire after the
TTL, and an open PR supersedes them). `owns` is the set of files a wave edits;
two waves with disjoint `owns` cannot conflict. Every path below exists in the
tree today unless marked `(new)`. The two external dependencies the status
paragraph names by description now have numbers: the shared MapperInput
derivation phase is [phase-457](phase-457-consume-the-shared-derivation.md)
(its W2 is the call), and the tier-derivation fix is
[phase-459](phase-459-cmake-image-reaches-tier-derivation.md) (its W2 is the
wave after which a grouped node has a SchedContext on target).

| claim id | depends on | owns | gate | starts now? |
| --- | --- | --- | --- | --- |
| `phase-462-W1` | none | a new monitor-table region in `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs` (installed before entity creation); `packages/api/nros-cpp/src/lib.rs` (the new `nros_cpp_install_monitors` export only); `packages/api/nros-cpp/src/publisher.rs` and `packages/api/nros-cpp/include/nros/publisher.hpp` (the bump and the stamp); `scripts/rmw-abi-shape.py` (the mirror); `packages/testing/nros-tests/bins/contract-monitor-cpp/` (new); one C++ case in `packages/testing/nros-tests/tests/contract_monitor_parity.rs` | `cargo test -p nros-tests --test contract_monitor_parity`; `just mem-report` on the island image (0 B uncontracted, 14 cells priced) | yes |
| `phase-462-W2` | `phase-462-W1`; `phase-459-W2` (the deadline half needs a derived tier's SchedContext) | `DeadlineAction` in `packages/core/nros-node/src/executor/sched_context.rs` (:140-:166); `silence-runtime` in `packages/core/nros-node/src/executor/monitor.rs`; a `RULE_SILENCE` const in `packages/core/nros-diagnostics/src/lib.rs`; `on_violation` rows beside `monitor_rows` in `packages/cli/nros-cli-core/src/orchestration/model_ingest.rs` (:1336); the `deadline_policy` source in `packages/cli/nros-cli-core/src/codegen/entry/mod.rs` (:437-:582); `packages/core/nros-orchestration-ir/src/violation_agreement.rs` (new, the contract-vs-tier-string refusal in `qos_agreement.rs`'s shape) and its `mod` line; `packages/testing/nros-tests/tests/on_violation_lowering.rs` (new) | `cargo test -p nros-tests --test on_violation_lowering`; `cargo test -p nros-orchestration-ir violation_agreement` | no |
| `phase-462-W3` | `phase-462-W1`, `phase-462-W2`, `phase-459-W2` (the period term), `phase-457-W2` (the reaction chain) | hazard rule consts in `packages/core/nros-diagnostics/src/lib.rs`; a hazard-row fn beside the monitor rows in `packages/cli/nros-cli-core/src/orchestration/model_ingest.rs`; the hazard table region in `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs` and `emit_rust.rs`; the `Fault` hook target in `packages/core/nros-node/src/executor/sched_context.rs`; `packages/core/nros-orchestration-ir/src/ftti_budget.rs` (new) and its `mod` line; `packages/testing/nros-tests/tests/hazard_rows_and_ftti.rs` (new) | `cargo test -p nros-tests --test hazard_rows_and_ftti`; `cargo test -p nros-orchestration-ir ftti_budget` | no |
| `phase-462-W4` | `phase-462-W1`, `phase-462-W2`, `phase-457-W2` | `packages/core/nros-orchestration-ir/src/mode_variants.rs` (new) and its `mod` line; the `Plan` variant tables in `packages/cli/nros-cli-core/src/codegen/entry/mod.rs`; the variant emission in `emit_cpp.rs` and `emit_rust.rs`; `packages/testing/nros-tests/tests/mode_variants.rs` (new) | `cargo test -p nros-tests --test mode_variants`; `just mem-report` prices the second table at the delta of its overrides | no |
| `phase-462-W5` | `phase-462-W2`; an rlm release carrying `deadline` and `liveliness` on endpoints (after R4; no rlm wave id exists for it yet) | `packages/core/nros-orchestration-ir/src/qos_override.rs` (:108-:135) and `qos_agreement.rs` (`MODELLED_POLICIES` widened); `packages/rmw/cyclonedds/nros-rmw-cyclonedds/` (the DDS deadline policy); `packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs` and `packages/rmw/xrce/nros-rmw-xrce/` (the `deadline: not served` const); a `deadline` case in `packages/cli/nros-cli-core/tests/contract_qos_override_agreement.rs` | `cargo test -p nros-orchestration-ir qos_agreement`; `cargo test -p nros-cli-core --test contract_qos_override_agreement` | no |
| `phase-462-W6a` | none | `zephyr/Kconfig` (`NROS_RX_TASK_BUDGET_FRAMES` and its burst twin); the zenoh read task in `zephyr/nros_zenoh_zephyr_system.c`; the knob's ladder entries as `scripts/check-knob-delivery.py` and `scripts/check-kconfig-knob-forwarding.sh` require them; `docs/design/0074-ingress-budget-rate-and-burst.md` (`implements-tracked-by`) | the RFC-0074 flood cell rerun with the budget stated records 0 stalls; a budget of 0 refuses | yes |
| `phase-462-W6b` | `phase-462-W6a`; rlm issue 0760 (the `{ rate_hz, burst }` schema) | the knob derivation in `packages/cli/nros-cli-core/src/cmd/entity_inventory.rs` (the two numbers from the largest declared `rate_hz x burst`) and a test beside it | `cargo test -p nros-cli-core entity_inventory` (the derived pair equals the hand-stated pair on the flood cell's contract) | no |

Two claims can start today: W1 and W6a. W6 is split because its two halves
have different dependencies - the stated knob needs nothing, the derived knob
needs a schema rlm does not have - and the doc already said "W6-stated can
start today". W3 stays one claim: its observer, its reaction and its FTTI check
are three readers of one hazard table, and splitting them would give two
sessions the same table to write.

**Files two waves touch, and the order they serialise in.** Across this phase,
phase-457, phase-459, the other session's phases 460/461/463 and play_launch
phase 78:

- `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs`:
  `phase-459-W2` (the `run_tiers` table), `phase-462-W1` (the monitor
  table), `phase-462-W3` (the hazard table), `phase-462-W4` (variants), then
  the other session's `phase-461-W6` (:1086) and `phase-463-W2` (the census
  entry), whose mutual order it states. Order among ours: `phase-459-W2`,
  `phase-462-W1`, `phase-462-W3`, `phase-462-W4`; W1 lands before 461 W6 and
  463 W2, and W3/W4 rebase over whichever of those has landed.
- `packages/api/nros-cpp/src/lib.rs`: `phase-462-W1` adds one export; the
  other session's `phase-463-W2` edits the hosted boot funnel
  (`nros_board_native_run_components_named`). Disjoint functions; order
  `phase-462-W1` first, because it starts today.
- `packages/cli/nros-cli-core/src/orchestration/model_ingest.rs`:
  `phase-459-W1`, `phase-457-W1`, `phase-459-W3`, `phase-462-W2`,
  `phase-462-W3`, in that order; each of this phase's waves adds a row fn
  beside `monitor_rows` (:1336) and edits nothing above it.
- `packages/cli/nros-cli-core/src/codegen/entry/mod.rs`: `phase-459-W2`
  (`plan_from_model`), then `phase-462-W2` (`deadline_policy` :437-:582),
  then `phase-462-W4` (variant tables).
- `packages/core/nros-node/src/executor/sched_context.rs` and `monitor.rs`:
  `phase-462-W2`, then `phase-462-W3`. phase-461 W2 (the parameter family's
  inbox) is in `nros-node` too but not in the executor; if it reaches these
  files the other session says so.
- `packages/core/nros-diagnostics/src/lib.rs`: `phase-462-W2` (one const),
  then `phase-462-W3` (three consts). Append-only, so a rebase is trivial.
- `packages/core/nros-orchestration-ir/src/qos_agreement.rs` and
  `qos_override.rs`: `phase-462-W5` only, in these three phases.
- `zephyr/Kconfig`: `phase-462-W6a` only, in these three phases; phase-460
  W4 (domain agreement between `system.toml` and Kconfig) reads it, and
  phase-461 W1/W2 add inbox knobs to it - the other session names its slots;
  W6a's block is new and self-contained.
- `packages/core/nros-orchestration-ir/src/mapper_input.rs` and `derive.rs`:
  this phase edits neither; W3 and W4 READ the chain and the periods through
  phase-457 W2 and phase-459 W2 and depend on them.

## Why

The launch contract carries a safety vocabulary, and it stops at Linux.
Measured on this tree (`origin/main` @ `783cdfa14`):

| contract field | checked by | observed at runtime by | read by nano-ros |
| --- | --- | --- | --- |
| `hazards`, `severity_levels`, `ftti` | rlm (`fault-reaction-budget`, criticality derivation, play_launch phase 72) | play_launch phase 73's live fault observer: `hazard-detected`, `hazard-reaction` (warning inside `ftti`, error outside), `hazard-recovered` | nothing |
| `on_violation` | rlm | play_launch phase 71's reaction engine | nothing |
| `safe_state` | rlm | play_launch phase 71 | nothing |
| `functions` | rlm | -- | nothing |
| `modes` and per-mode overrides | rlm; play_launch phase 75 (complete, `docs/design/operational-modes.md` there) | play_launch | nothing |
| `criticality` | rlm (a consequence of hazards since phase 72) | -- | `mapper_input.rs:28` -- the priority bucket, and nothing else |
| `srv_endpoints.max_response_ms`, `tolerance_ms` | carried | -- | nothing (phase-434 "still open") |
| `pub.min_rate_hz`, `node_paths[].max_latency_ms`, `sub.max_age_ms` | rlm | -- | RUST entries only: `model_ingest.rs:1336 monitor_rows`, `:1429 render_monitor_rs` bake `MonitorSpec` / `AgeMonitorSpec` rows; `codegen/entry/emit_cpp.rs` bakes none |
| `deadline`, `liveliness` QoS | rlm model carries reliability, durability, history, depth, lifespan | -- | only as launch `qos_overrides.*` (`nros-orchestration-ir/src/qos_override.rs:108-135`), never from the contract |
| ingress budget `{ rate_hz, burst }` (RFC-0074) | no schema (rlm issue 0760) | -- | no knob: `NROS_RX_TASK_BUDGET_FRAMES` appears only in the RFC (`0074:160`) |

The "nothing" cells were established by grep over `packages/cli/nros-cli-core/src`
and `packages/core/nros-orchestration-ir/src` for `hazards`, `on_violation`,
`safe_state`, `severity_levels`, `max_response_ms`, `tolerance_ms`, `ftti`:
no match in any Rust source. `modes` matches only prose.

The consequence on the one deployment this matters for: the Autoware Safety
Island is a C++ image, so its 14 `min_rate_hz` promises are monitored by
NOTHING on target; its `on_violation` and `safe_state` are enforced by
play_launch on the Linux side of the bridge, which is the side the island
exists to survive the loss of. The executor already has the primitives -- a
monitor drain with `min-rate-runtime`, `max-age-runtime`,
`max-latency-runtime`, `deadline-miss-runtime`, `release-jitter-runtime`
(`nros-node/src/executor/monitor.rs`) and a `DeadlineAction` of
`Ignore | Warn | Skip | Fault` lowered from `[tiers.<t>].deadline_policy`
(`sched_context.rs:140-166`) -- and the contract already has the facts. The
gap is the lowering, and it is a gap in the bake, not in the runtime.

## What it does

Six waves, ordered so that each consumes only what the previous produced and
what the two external dependencies deliver. Every wave is a lowering from a
contract fact the resolver already carries to a runtime table the executor
already reads, or to a build-time check. No wave adds a contract field; the
ingress budget (W6) is the one place a schema is missing, and that schema is
rlm's to add (issue 0760 there).

### W1 [cli] -- C++ monitor table parity

Consumes: `monitor_rows` and `AgeRow`s from `model_ingest.rs`, which the Rust
emitter already turns into `NROS_MONITORS` / `NROS_AGE_MONITORS`.

`emit_cpp.rs` gains a monitor table in the same shape the Rust entry bakes:
one `PubMonitorCell` per contracted publisher, one `SubMonitorCell` per age
contract, installed on the executor before entity creation through a C ABI
(`nros_cpp_install_monitors`) mirrored by `check-rmw-abi-shape`'s method. The
C++ publisher facade bumps the cell on publish and stamps the outgoing header
exactly as the Rust handle does (`observe_publish_stamp`,
`monitor.rs:116`). The violation drain reaches `nros-diagnostics` on C++
images as it does on Rust ones.

Gate: the `contract-monitor` fixture (`nros-tests/bins/contract-monitor`)
gains a C++ twin; a publisher held below its `min_rate_hz` on native_sim
produces one `min-rate-runtime` violation on the drain within one check
window; a C++ image with no contract bakes an empty table and the drain
dead-code-eliminates, measured by `mem-report` on the island's image (target:
0 B for an uncontracted image, and the island's 14 cells priced).

Depends on: nothing external. This is the wave to start with because the
island is C++ and everything after it reports through the drain it installs.

Claim: phase-462-W1. Depends on: none. Owns: a new monitor-table region in packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs; the nros_cpp_install_monitors export in packages/api/nros-cpp/src/lib.rs; packages/api/nros-cpp/src/publisher.rs; packages/api/nros-cpp/include/nros/publisher.hpp; scripts/rmw-abi-shape.py; packages/testing/nros-tests/bins/contract-monitor-cpp/ (new); one case in packages/testing/nros-tests/tests/contract_monitor_parity.rs. Gate: cargo test -p nros-tests --test contract_monitor_parity; just mem-report on the island image. Status: landed in PR #1170 (a885f7761); contract_monitor_parity 4 passed; residue 88 B on the contracted C++ twin, 0 B uncontracted. Follow-ups: MAX_MONITORS = 8 is below the island's 14 rows (install refuses, not truncates); Rust entries bake system_monitors.rs but nros::main! never installs it; sub-side age recording and the publish-stamp readout are outside W1.

### W2 [cli, nros-node] -- `on_violation` lowers to the executor

Consumes: `on_violation` per contract (node, path or endpoint) from the
resolver; the tier a node is bound to (declared, or derived once the
tier-derivation fix lands).

Two targets, because a violation is either "the callback ran too long" or
"the input stopped coming":

- **Deadline.** `on_violation` on a path lowers to `DeadlineAction` on the
  SchedContext of the tier that runs that path's callbacks: `warn` -> `Warn`,
  `skip` -> `Skip`, `fault` -> `Fault`, the vocabulary `from_tier_str`
  already accepts. Today that string is authored per tier in `system.toml`;
  after W2 the contract's field is the source and a tier-table string that
  disagrees is a resolve-time refusal (the same shape as phase-454 W7 for
  `qos_overrides`).
- **Silence.** A per-subscription silence monitor: the executor already
  records take-age (`max-age-runtime`); a subscription whose contract states
  `max_age_ms` and whose cell has not advanced in that window is a
  `silence-runtime` violation, which is the on-target form of a liveliness
  lease. `on_violation` selects the reaction the same way.

Gate: a path with `on_violation: fault` and a callback that overruns its
`max_latency_ms` invokes the fault hook on native_sim; a subscription starved
past `max_age_ms` produces `silence-runtime` once per transition; a contract
field and a tier string that disagree refuse at resolve, naming both.

Depends on: the tier-derivation fix, because a groupless node has no
SchedContext for the deadline half to land on (`derive.rs:93-100` today notes
it and moves on). Until it lands, W2's deadline half applies only to declared
tiers, and this doc must say so at that point rather than count it done.

Claim: phase-462-W2. Depends on: phase-462-W1, phase-459-W2. Owns: DeadlineAction in packages/core/nros-node/src/executor/sched_context.rs; silence-runtime in packages/core/nros-node/src/executor/monitor.rs; one const in packages/core/nros-diagnostics/src/lib.rs; on_violation rows beside monitor_rows in packages/cli/nros-cli-core/src/orchestration/model_ingest.rs; deadline_policy in packages/cli/nros-cli-core/src/codegen/entry/mod.rs; packages/core/nros-orchestration-ir/src/violation_agreement.rs (new); packages/testing/nros-tests/tests/on_violation_lowering.rs (new). Gate: cargo test -p nros-tests --test on_violation_lowering. Status: not started.

### W3 [nros-node, cli] -- `safe_state`, `hazards`, and the FTTI budget

Consumes: `hazards[]` with `severity`, `ftti`, `safe_state` per node and per
hazard; the reaction chain rlm's `fault-reaction-budget` already checks; the
tier plan (declared or derived) with its periods.

What an RTOS image can do is narrower than play_launch's observer and also
more, and this wave decides which:

- **An observer** in play_launch's shape: `hazard-detected` when a monitored
  contract fails, `hazard-reaction` when the executor's reaction ran (with
  the measured time from detection, judged against `ftti`),
  `hazard-recovered` when the guard resumes. This is a naming layer over W1's
  drain and W2's reactions: it groups violations by the hazard the contract
  attributes them to. It costs one table row per hazard and nothing on the
  hot path.
- **The reaction itself.** The island IS the safety mechanism: its
  `safe_state` is a publication (`OperateMrm`, the stop request) that a node
  in the image already produces. The reaction to a hazard is therefore a
  callback the image already has, and `safe_state` lowers to WHICH callback
  runs when `on_violation` fires -- a `DeadlineAction::Fault` hook that
  invokes the node's declared safe-state entry rather than a panic. W3 takes
  this only for nodes whose `safe_state` names a path in the same image;
  anything else stays an observer row.
- **FTTI at build time.** `ftti` is a budget over a chain: detection window
  (W1's check interval, or the silence window) plus the reaction path's
  `max_latency_ms` plus the reaction tier's period. All three are numbers the
  bake has once tiers are derived, so `ftti < detection + reaction + period`
  is a resolve-time refusal that names the chain, the same fail-loud shape as
  the arena-vs-heap gate. Today rlm checks the same inequality against the
  contract's own declared latencies; this is the bake checking it against
  what the image will actually run.

Gate: the island's `mrm_handler` hazards produce hazard rows on the drain
with the same three names play_launch uses, so one log reader serves both
sides of the bridge; an `ftti` shorter than the derived reaction chain refuses
at resolve; on native_sim a forced violation reaches the safe-state callback
within `ftti`, measured by the drain's own timestamps.

Depends on: W1, W2, the tier-derivation fix (for the period term), and the
shared MapperInput derivation phase for the chain that `fault-reaction-budget`
walks, so that nano-ros and rlm agree on which callbacks form the reaction
chain rather than each walking the graph its own way.

Claim: phase-462-W3. Depends on: phase-462-W1, phase-462-W2, phase-459-W2, phase-457-W2. Owns: hazard consts in packages/core/nros-diagnostics/src/lib.rs; a hazard-row fn in packages/cli/nros-cli-core/src/orchestration/model_ingest.rs; the hazard table region in packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs and emit_rust.rs; the Fault hook target in packages/core/nros-node/src/executor/sched_context.rs; packages/core/nros-orchestration-ir/src/ftti_budget.rs (new); packages/testing/nros-tests/tests/hazard_rows_and_ftti.rs (new). Gate: cargo test -p nros-tests --test hazard_rows_and_ftti. Status: not started.

### W4 [cli] -- `modes` as tier-table variants

Consumes: `modes[]` and per-mode overrides (rates, deadlines, `on_violation`)
from the resolver; play_launch phase 75's design as the semantics of record.

A mode is a set of overrides over the same entities; the executor's tables
are static. So a mode lowers to a VARIANT of the tier table and the monitor
table -- one extra table per mode, selected at boot or by a mode switch
callback, never rebuilt at runtime. The bake refuses a mode whose overrides
would change the entity SET (a mode cannot add a subscription), because that
would change the pools. The mode switch is itself a contracted transition
with a `max_latency_ms`.

Gate: an image with two modes bakes two monitor tables and two SchedContext
sets, `mem-report` prices the second at the delta of its overrides only; a
mode override on an entity the base mode does not have refuses at resolve.

Depends on: W1, W2; the shared MapperInput derivation phase for what a mode
override means to the mapper (which is where play_launch phase 75 put it).

Claim: phase-462-W4. Depends on: phase-462-W1, phase-462-W2, phase-457-W2. Owns: packages/core/nros-orchestration-ir/src/mode_variants.rs (new); the Plan variant tables in packages/cli/nros-cli-core/src/codegen/entry/mod.rs; the variant emission in emit_cpp.rs and emit_rust.rs; packages/testing/nros-tests/tests/mode_variants.rs (new). Gate: cargo test -p nros-tests --test mode_variants; just mem-report. Status: not started.

### W5 [cli, rmw] -- deadline and liveliness QoS from the contract

Consumes: `deadline` and `liveliness` / `liveliness_lease_duration` on
contract endpoints, once the rlm model carries them (it carries reliability,
durability, history, depth, lifespan today; rlm `model/src/lib.rs:1181-1191`).

The lowering already exists for the launch road (`qos_override.rs`); this
wave makes the contract the source and the override a checked restatement,
exactly as phase-454 W7 did for the four policies the model already has. On
the RMW side `deadline` maps to the DDS policy where a backend serves it
(cyclone) and to W2's silence monitor where it does not (zenoh, xrce), stated
per backend by a const the same way phase-461 W2 states caller-owned inboxes.

Gate: an island endpoint with `deadline` in the contract and a
`qos_overrides.deadline` that disagrees refuses at resolve; on cyclone the
graph shows the deadline; on zenoh the silence monitor fires in its place and
the graph token says `deadline: not served`, never a silent downgrade.

Depends on: rlm carrying the two policies; W2's silence monitor.

Claim: phase-462-W5. Depends on: phase-462-W2; an rlm release carrying deadline and liveliness on endpoints (after R4, no wave id yet). Owns: packages/core/nros-orchestration-ir/src/qos_override.rs; packages/core/nros-orchestration-ir/src/qos_agreement.rs; packages/rmw/cyclonedds/nros-rmw-cyclonedds/; packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs; packages/rmw/xrce/nros-rmw-xrce/; a case in packages/cli/nros-cli-core/tests/contract_qos_override_agreement.rs. Gate: cargo test -p nros-orchestration-ir qos_agreement. Status: not started.

### W6 [rmw-zenoh, zephyr] -- the ingress budget knob

Consumes: RFC-0074's `{ rate_hz, burst }` on a subscription, once rlm issue
0760 gives it a schema. Until then the knob exists with no derivation, stated
by hand, which is the honest state RFC-0074 itself proposes for the
prototype.

`NROS_RX_TASK_BUDGET_FRAMES` (and its burst twin) on the plain ladder, lowered
to the zenoh read task's per-batch frame budget as RFC-0074 sketches
(`0074:160`). The device-side half only; the router-side pacing rule is
deployment configuration and stays outside the image. When the schema lands,
the two numbers derive from the subscription with the largest declared
`rate_hz x burst` and the knob becomes derivable.

Gate: the flood cell RFC-0074 measured (9-11 stalls per 30 s at ~2 kHz,
FreeRTOS mps2-an385) reruns on the pinned tree with the budget stated and
records 0 stalls; a budget of 0 is a refusal, not "unlimited".

Depends on: rlm issue 0760 for derivation; nothing for the stated knob.

Two claims, because the two halves wait on different things (see the
Parallel plan): W6a is the stated knob, W6b its derivation.

Claim: phase-462-W6a. Depends on: none. Owns: zephyr/Kconfig; zephyr/nros_zenoh_zephyr_system.c; the knob's ladder entries as scripts/check-knob-delivery.py and scripts/check-kconfig-knob-forwarding.sh require them; docs/design/0074-ingress-budget-rate-and-burst.md. Gate: the RFC-0074 flood cell rerun with the budget stated records 0 stalls. Status: not started.

Claim: phase-462-W6b. Depends on: phase-462-W6a; rlm issue 0760. Owns: the knob derivation in packages/cli/nros-cli-core/src/cmd/entity_inventory.rs and a test beside it. Gate: cargo test -p nros-cli-core entity_inventory. Status: not started.

## Order and dependencies, in one place

```
W1  C++ monitor parity            <- nothing
W2  on_violation -> DeadlineAction, silence monitor
                                  <- W1; tier-derivation fix (deadline half)
W3  safe_state, hazards, FTTI     <- W1, W2; tier-derivation fix; shared MapperInput
                                     derivation phase (the reaction chain)
W4  modes as table variants       <- W1, W2; shared MapperInput derivation phase
W5  deadline / liveliness         <- rlm model fields; W2
W6  ingress budget knob           <- nothing (stated); rlm 0760 (derived)
```

W1 and W6-stated can start today. Everything else waits on a fact another
phase produces, and this document should not be opened for work before the
two external dependencies have a phase number to cite.
 As of 2026-09-21 they do:
phase-457 (the shared derivation) and phase-459 (the tier derivation); the
Parallel plan above cites them by claim id.

## Gates for the phase

- Every wave adds its rule name to `nros-diagnostics`'s rule table and to the
  book's monitor page; a rule that fires with no table row is the silent
  class this phase exists to remove.
- `check-roadmap-claims` R1: the header says NOT STARTED and no box is
  ticked; when a wave lands, the header changes in the same commit.
- Each lowering has a resolve-time refusal for a contract/tier disagreement
  and a negative control in the same commit, phase-454 W7's shape.
- No wave changes a pool size; the entity inventory's counts are the same
  before and after, asserted by `nros image-facts` on the island's image.

## Limits

- Nothing here makes an RTOS image a certified safety mechanism; it makes the
  image's behaviour under a violated contract STATED and observable, which is
  what a safety argument can then cite.
- The island today declares no callback groups, so it has no tier and W2's
  deadline half does not apply to it until either groups are declared or the
  tier-derivation fix gives a groupless node one. That is a design question
  the island's report (section 10) records as unanswered; this phase does not
  answer it.
- `functions` is listed and consumed by no wave: it is a grouping of hazards
  for the reader and has no runtime meaning the executor could act on.
- `srv_endpoints.max_response_ms` and `tolerance_ms` gain a reader only
  through W2's silence monitor applied to a client's pending reply; that is a
  natural extension and is not written as a wave because no image in the tree
  has a contracted service client with a response bound.
- The ingress budget's router-side half is out of scope by RFC-0074's own
  split.

## Docs to update when a wave lands

- `docs/design/0052-system-model-rtos-mapper.md`: its "on-target monitors"
  claim holds for Rust images only until W1; say so now, and remove the
  qualifier after.
- the book's monitor page and `nros-diagnostics` rule table, per wave.
- the island's `nxp-deployment.md` section 7 ("the contract sizes the image;
  it does not schedule it") and section 10, when W1 gives it monitors.
- `docs/design/0074-ingress-budget-rate-and-burst.md`: `implements-tracked-by`
  gains this phase at W6.
