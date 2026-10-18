# phase-462 -- the contract's safety vocabulary reaches the target

**Status (2026-10-16). IN PROGRESS: W1, W2 and W3 landed; W4 and W6a are open;
W5 and W6b wait on rlm.** (Planned 2026-09-21.) Numbered highest + 7 (phase-461 took +6;
456-460 belong to concurrent sessions). Depends on two pieces of work outside
this document: the shared MapperInput derivation phase (a play_launch / rlm
phase being designed in parallel, referred to here by that name and not by a
number) and the tier-derivation fix (the `derive.rs:93-100` gate that lets a
groupless node fall to the default tier with only a note).

> **phase-494 (2026-10-01):** `codegen/entry/emit_cpp.rs` and `emit_c.rs` are
> DELETED — every C-family pack renders `nros_entry_lower::LoweredEntry`
> through one renderer. Where a wave below names a region or a line in
> `emit_cpp.rs`, read: the fact as a field built in `codegen/entry/lower.rs`,
> and its spelling as a block in `packs/entry/cpp/` (and `packs/entry/c/`,
> which now gets the same field for free).

## Parallel plan

One claim per wave (`just claim <id>`; claims are advisory, expire after the
TTL, and an open PR supersedes them). `owns` is the set of files a wave edits;
two waves with disjoint `owns` cannot conflict. Every path below exists in the
tree today unless marked `(new)`. The two external dependencies the status
paragraph names by description now have numbers: the shared MapperInput
derivation phase is [phase-495](archived/phase-495-consume-the-shared-derivation.md)
(its W2 is the call), and the tier-derivation fix is
[phase-459](archived/phase-459-cmake-image-reaches-tier-derivation.md) (its W2 is the
wave after which a grouped node has a SchedContext on target).

| claim id | depends on | owns | gate | starts now? |
| --- | --- | --- | --- | --- |
| `phase-462-W1` | none | a new monitor-table region in `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs` (installed before entity creation); `packages/api/nros-cpp/src/lib.rs` (the new `nros_cpp_install_monitors` export only); `packages/api/nros-cpp/src/publisher.rs` and `packages/api/nros-cpp/include/nros/publisher.hpp` (the bump and the stamp); `scripts/rmw-abi-shape.py` (the mirror); `packages/testing/nros-tests/bins/contract-monitor-cpp/` (new); one C++ case in `packages/testing/nros-tests/tests/contract_monitor_parity.rs` | `cargo test -p nros-tests --test contract_monitor_parity`; `just mem-report` on the island image (0 B uncontracted, 14 cells priced) | yes |
| `phase-462-W2` | `phase-462-W1`; `phase-459-W2` (the deadline half needs a derived tier's SchedContext) | `DeadlineAction` in `packages/core/nros-node/src/executor/sched_context.rs` (:140-:166); `silence-runtime` in `packages/core/nros-node/src/executor/monitor.rs`; a `RULE_SILENCE` const in `packages/core/nros-diagnostics/src/lib.rs`; `on_violation` rows beside `monitor_rows` in `packages/cli/nros-cli-core/src/orchestration/model_ingest.rs` (:1336); the `deadline_policy` source in `packages/cli/nros-cli-core/src/codegen/entry/mod.rs` (:437-:582); `packages/core/nros-orchestration-ir/src/violation_agreement.rs` (new, the contract-vs-tier-string refusal in `qos_agreement.rs`'s shape) and its `mod` line; `packages/testing/nros-tests/tests/on_violation_lowering.rs` (new) | `cargo test -p nros-tests --test on_violation_lowering`; `cargo test -p nros-orchestration-ir violation_agreement` | no |
| `phase-462-W3` | `phase-462-W1`, `phase-462-W2`, `phase-459-W2` (the period term), `phase-495-W2` (the reaction chain) | hazard rule consts in `packages/core/nros-diagnostics/src/lib.rs`; a hazard-row fn beside the monitor rows in `packages/cli/nros-cli-core/src/orchestration/model_ingest.rs`; the hazard table region in `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs` and `emit_rust.rs`; the `Fault` hook target in `packages/core/nros-node/src/executor/sched_context.rs`; `packages/core/nros-orchestration-ir/src/ftti_budget.rs` (new) and its `mod` line; `packages/testing/nros-tests/tests/hazard_rows_and_ftti.rs` (new) | `cargo test -p nros-tests --test hazard_rows_and_ftti`; `cargo test -p nros-orchestration-ir ftti_budget` | no |
| `phase-462-W4` | `phase-462-W1`, `phase-462-W2`, `phase-495-W2` | `packages/core/nros-orchestration-ir/src/mode_variants.rs` (new) and its `mod` line; the `Plan` variant tables in `packages/cli/nros-cli-core/src/codegen/entry/mod.rs`; the variant emission in `emit_cpp.rs` and `emit_rust.rs`; `packages/testing/nros-tests/tests/mode_variants.rs` (new) | `cargo test -p nros-tests --test mode_variants`; `just mem-report` prices the second table at the delta of its overrides | no |
| `phase-462-W5` | `phase-462-W2`; an rlm release carrying `deadline` and `liveliness` on endpoints (after R4; no rlm wave id exists for it yet) | `packages/core/nros-orchestration-ir/src/qos_override.rs` (:108-:135) and `qos_agreement.rs` (`MODELLED_POLICIES` widened); `packages/rmw/cyclonedds/nros-rmw-cyclonedds/` (the DDS deadline policy); `packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs` and `packages/rmw/xrce/nros-rmw-xrce/` (the `deadline: not served` const); a `deadline` case in `packages/cli/nros-cli-core/tests/contract_qos_override_agreement.rs` | `cargo test -p nros-orchestration-ir qos_agreement`; `cargo test -p nros-cli-core --test contract_qos_override_agreement` | no |
| `phase-462-W6a` | none | `zephyr/Kconfig` (`NROS_RX_TASK_BUDGET_FRAMES` and its burst twin); the zenoh read task in `zephyr/nros_zenoh_zephyr_system.c`; the knob's ladder entries as `scripts/check-knob-delivery.py` and `scripts/check-kconfig-knob-forwarding.sh` require them; `docs/design/0074-ingress-budget-rate-and-burst.md` (`implements-tracked-by`) | the RFC-0074 flood cell rerun with the budget stated records 0 stalls; a budget of 0 refuses | yes |
| `phase-462-W6b` | `phase-462-W6a`; rlm issue 0760 (the `{ rate_hz, burst }` schema) | the knob derivation in `packages/cli/nros-cli-core/src/entity_inventory.rs` (the two numbers from the largest declared `rate_hz x burst`) and a test beside it | `cargo test -p nros-cli-core entity_inventory` (the derived pair equals the hand-stated pair on the flood cell's contract) | no |

Two claims can start today: W1 and W6a. W6 is split because its two halves
have different dependencies - the stated knob needs nothing, the derived knob
needs a schema rlm does not have - and the doc already said "W6-stated can
start today". W3 stays one claim: its observer, its reaction and its FTTI check
are three readers of one hazard table, and splitting them would give two
sessions the same table to write.

**Files two waves touch, and the order they serialise in.** Across this phase,
phase-495, phase-459, the other session's phases 460/461/463 and play_launch
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
  `phase-459-W1`, `phase-495-W1`, `phase-459-W3`, `phase-462-W2`,
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
  phase-495 W2 and phase-459 W2 and depend on them.

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
| ingress budget `{ rate_hz, burst }` (RFC-0074) | at planning no schema (rlm issue 0760); since rlm v0.1.64 `sub.<e>.ingress` | -- | at planning no knob; since W6a/W6b `NROS_ZENOH_RX_TASK_BUDGET_*`, derived by the entity inventory |

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

Claim: phase-462-W1. Depends on: none. Owns: a new monitor-table region in packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs; the nros_cpp_install_monitors export in packages/api/nros-cpp/src/lib.rs; packages/api/nros-cpp/src/publisher.rs; packages/api/nros-cpp/include/nros/publisher.hpp; scripts/rmw-abi-shape.py; packages/testing/nros-tests/bins/contract-monitor-cpp/ (new); one case in packages/testing/nros-tests/tests/contract_monitor_parity.rs. Gate: cargo test -p nros-tests --test contract_monitor_parity; just mem-report on the island image. Status: landed in PR #1170 (34ef4405f); contract_monitor_parity 4 passed; residue 88 B on the contracted C++ twin, 0 B uncontracted. Follow-ups: MAX_MONITORS = 8 is below the island's 14 rows (install refuses, not truncates); Rust entries bake system_monitors.rs but nros::main! never installs it; sub-side age recording and the publish-stamp readout are outside W1.

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

Claim: phase-462-W2. Depends on: phase-462-W1, phase-459-W2. Owns: DeadlineAction in packages/core/nros-node/src/executor/sched_context.rs; silence-runtime in packages/core/nros-node/src/executor/monitor.rs; one const in packages/core/nros-diagnostics/src/lib.rs; on_violation rows beside monitor_rows in packages/cli/nros-cli-core/src/orchestration/model_ingest.rs; deadline_policy in packages/cli/nros-cli-core/src/codegen/entry/mod.rs; packages/core/nros-orchestration-ir/src/violation_agreement.rs (new); packages/testing/nros-tests/tests/on_violation_lowering.rs (new). Gate: cargo test -p nros-tests --test on_violation_lowering. Status: landed 2026-09-24 (d2adf4b7c1, with 65db78ecd1 for the age-state byte budget); on_violation_lowering 4 passed, violation_agreement 9 passed. Scope as landed: rlm carries no warn|skip|fault word, so the action is the reacting path's `miss.action` (`continue | skip_next | abort`), reached directly or through an endpoint's `on_violation.reaction` pointer. The deadline half reached declared tiers only when it landed; phase-459 W2 has since given a groupless node a derived tier.

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

**Re-planned 2026-10-16 against the model this tree pins.** The sketch above
predates `contracts.hazards`' present shape (ros-launch-manifest model, rev
33577ff): a hazard carries `guards` (topic FQNs, one fault each, `all_of` for a
redundant set), `on` (fault classes; empty means all), `ftti_ms`, and a
`reaction` that is a key into `contracts.modes` (whose own `reaction` is a path)
or a path key directly; `safe_state` sits on the reacting PATH (`emits`,
`settle_ms`). No reaction-chain walker exists in either tree, and phase-503
already carries a route's supervised reaction to a hook (`reaction_id`,
`ReactionEvent`, `set_reaction_hook`). So W3 lands in three units:

- **W3a -- the FTTI budget at build time.** `nros-orchestration-ir::ftti_budget`.
  Detection is what the executor itself observes on a guard topic, from the
  rows this image bakes: `silence-runtime` / `max-age-runtime` at the
  subscriber's `max_age_ms`, `min-rate-runtime` at two rate windows (10 s); a
  class with no detector here (`reported`, `overrun`) is listed as unobserved
  and never counted. Reaction is the resolved path's `max_latency_ms`; release
  is its timer period plus stated jitter, else the period of the tier that runs
  its first input's node. A budget provably over `ftti_ms` refuses on both bake
  roads (the CLI entry emitter and `nros::main!`), naming each term; an unknown
  term abstains.
- **W3b -- the observer.** `hazard-detected` / `hazard-reaction` /
  `hazard-recovered` rows grouping the drain's violations by the hazard whose
  guard they fall on, with the three names play_launch uses.
- **W3c -- the reaction.** A hazard whose reacting path states `safe_state` and
  runs in this image is wired to that path through phase-503's reaction hook,
  rather than a new `Fault` hook target.

W3a status: landed in this change. `cargo test -p nros-orchestration-ir
ftti_budget`: 9 passed, among them the refusal naming every term and its
negative control (the same image under a looser FTTI). The rate window the
check restates is pinned to the executor's by `nros-tests`'
`ftti_detection_bounds` (`trigger-test`, in `check required-features-tests`).
W3b status (Rust roads): landed in this change. The observer is
`nros-node`'s `check_hazard` over a baked `HazardSpec` table
(`Executor::set_hazard_table`), run on every monitor tick after the rows. It
adds no detector: a guard member is an age row (faulted while its
`silence-runtime` lease is reported) or a rate row (faulted while its last
window was short), so detection and recovery follow the rows by construction.
`hazard-reaction` fires when the `safe_state.emits` publisher's counter moves
after detection, which is why `monitor_rows` now gives that publisher a
count-only row. State lives in a `static HazardCell` per hazard, so the
`Executor` value grows by one slice (the issue-0961 byte budget holds).
A hazard lowers onto an executor's tables only when they hold EVERY monitor
its guard topics have anywhere in the model, so a multi-tier or multi-board
image never observes one hazard twice. Both Rust roads bake it: the CLI's
`system_monitors.rs` (`render_monitor_rs_full`) and `nros::main!`
(`install_contract_hazards` after `install_contract_monitors`). Rules
`hazard-detected` / `hazard-reaction` / `hazard-recovered` are appended to
`RULE_IDS` (codes 12-14), `nros-diagnostics` and the SWD decoder. Tests:
`nros-node` `hazard_tests` (4, one compiling the renderer's own output),
`nros-orchestration-ir` `hazard_row_tests` (4), `nros-macros`
`a_hazard_the_table_observes_is_baked_and_installed`.

W3b status (C/C++ road): landed in this change. `nros-cpp` exports
`nros_cpp_install_hazards` over C rows (`nros_cpp_hazard_row_t`, its guards
and members) and one caller-provided static buffer it carves the runtime's
cells, specs, guards and members out of -- the monitor install's shape, sized
by `NROS_CPP_HAZARD_{ROW,GUARD,MEMBER}_STORAGE` (the row bound is
pointer-width dependent and stated in the header with `sizeof(void *)`; const
asserts hold each bound to the real layout). `nros-entry-lower`'s
`MonitorTable` carries the hazards each executor's SLICE observes (the Rust
roads' `hazard_rows`), and both packs bake the arrays and install them after
`nros_cpp_install_monitors`. Tests: one per pack
(`typed_emit_bakes_the_hazard_table_after_the_monitors`, over one shared
model), and the rendered C and C++ were syntax-checked against the regenerated
header with `gcc`/`g++` (C also with `arm-none-eabi-gcc`, a 32-bit target).

W3c status: landed in this change. A hazard whose reacting path states a
`safe_state` carries that path's reaction id, and on `hazard-detected` the
executor calls phase-503's reaction hook with it (`rule_code` hazard-detected,
`row` the hazard's index), once per detection and not before arming -- the
application's `nros_app_reaction_hook` starts the safe-state path, rather than
a new `Fault` hook target. Reaction ids are numbered once per MODEL
(`contract_monitors::model_reaction_ids`: every supervised route's reaction and
every hazard path with a `safe_state`), so an id names one path whichever
executor reports it and whichever rule asks; a model with no hazard numbers
what it numbered before. The C row gains `reaction_id`; both packs declare the
weak hook and install it when any hazard has one, listing each hazard's id in
a comment. Tests: `a_detected_hazard_starts_its_safe_state_reaction` (the
executor, with a clock the test drives), the packs'
`typed_emit_wires_a_hazards_safe_state_reaction_to_the_hook`, and the rendered
C and C++ syntax-checked with the hook declared and installed.

What W3 leaves to the image, deliberately: what the safe-state path DOES (it
is the application's callback, started through the hook), and a guard on a
fault class the executor cannot observe (`reported`, `overrun`), which W3a
lists as unobserved and W3b does not watch.

Claim: phase-462-W3. Depends on: phase-462-W1, phase-462-W2, phase-459-W2, phase-495-W2. Owns: hazard consts in packages/core/nros-diagnostics/src/lib.rs; a hazard-row fn in packages/cli/nros-cli-core/src/orchestration/model_ingest.rs; the hazard table region in packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs and emit_rust.rs; the Fault hook target in packages/core/nros-node/src/executor/sched_context.rs; packages/core/nros-orchestration-ir/src/ftti_budget.rs (new); packages/testing/nros-tests/tests/hazard_rows_and_ftti.rs (new). Gate: cargo test -p nros-orchestration-ir ftti_budget (W3a); cargo test -p nros-tests --test hazard_rows_and_ftti (W3b/W3c). Status: landed (W3a, W3b, W3c).

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

W4 re-plan (2026-10-17). Like W3, W4 lands in three units. They depend on
different things.

- **W4a, the lowering, the runtime switch and the Rust roads: landed in this
  change.**
  - The input: each mode override's target in the MODEL's keys. That is
    `ModeOverrideContract.resolved` (ros-launch-manifest v0.1.63, filled by
    play_launch 24a962a1).
  - `nros-orchestration-ir::mode_variants`:
    - `apply_mode` applies a mode to a copy of the model;
    - `variant_rows` derives that copy's monitor and age rows. It refuses a
      mode whose rows are not the base's rows, row for row: a mode can change
      a requirement's value but not whether it exists. A rate taken to 0
      counts as a removed row, because 0 means "unjudged" to the executor.
    - An override no on-target row observes (a scope path's budget, a
      topic's rate, a ceiling) is listed in `not_lowered`, which
      `nros codegen-system` prints. It is never applied silently and never
      refused. `nros::main!` cannot warn on stable Rust, so it does not print
      the list.
  - Runtime (`nros-node`):
    - `ModeTables` is one variant of an executor's two tables. It shares the
      base's cells, so every binding and hazard row index survives a switch.
    - `Executor::set_mode_tables` refuses a variant of another length.
    - `monitor::request_mode(id)` is an atomic STATE. Each executor applies
      it at its next spin, before any rule judges, so a tiered image switches
      without one thread touching another's executor.
  - Both Rust roads bake it:
    - `nros::main!` gives each tier's table its sliced variants,
      `install_contract_modes`, and one `NROS_MODE_<NAME>` id constant per
      mode;
    - `nros codegen-system` writes `system_monitors.rs` with
      `nros_install_mode_tables`.
  - A model without modes bakes byte-identically.
  - Tests:
    - `nros-orchestration-ir` `mode_variants` (5);
    - `nros-node` `a_requested_mode_judges_by_its_variant_table`, plus the
      rendered text compiled in `monitor::tests::rendered_modes`;
    - `nros-macros` `a_contract_mode_bakes_a_variant_table_and_an_id`.
- **W4b, the C/C++ road: landed in this change.**
  - `nros-cpp` exports `nros_cpp_install_modes` over
    `nros_cpp_mode_tables_t`. A mode's rows carry ONLY its numbers
    (`nros_cpp_mode_row_t`: rate, latency, route budget, route deadline;
    `nros_cpp_mode_age_row_t`: age). The install copies the installed base
    specs, cells included, so a C entry states no topic or cell twice.
  - It refuses a mode whose row counts are not the base's, and a short or
    misaligned buffer. The buffer is sized by `NROS_CPP_MODE_{TABLE,ROW,AGE}_STORAGE`,
    whose bounds depend on pointer width; const asserts hold them.
  - `nros_cpp_request_mode(id)` is the C spelling of `request_mode`.
  - `nros-entry-lower`: each `MonitorTable` carries its sliced `modes`, and
    `LoweredEntry::modes` names each id. Both packs bake the arrays, install
    them after the monitors (and hazards), and `#define NROS_MODE_<NAME>`.
  - `nros codegen entry` prints the `not_lowered` list.
  - Tests: `typed_emit_bakes_a_modes_numbers_after_the_monitors` in tests_c
    and tests_cpp. The rendered C and C++ compile against the regenerated
    header with `gcc -std=c11`, `g++ -std=c++17` and `arm-none-eabi-gcc` (a
    32-bit target).
- **W4c, the tier-table half: not started.** It covers:
  - SchedContext deadlines per mode;
  - the mode switch as a contracted transition with its own
    `max_latency_ms`;
  - the gate's `mem-report` pricing of a second table;
  - `nros-tests` `mode_variants`.

  The tier table is built from the plan, not from the monitor rows, so this
  half needs the plan to carry per-mode deadlines first.

Claim: phase-462-W4. Depends on: phase-462-W1, phase-462-W2, phase-495-W2. Owns: packages/core/nros-orchestration-ir/src/mode_variants.rs (new); the Plan variant tables in packages/cli/nros-cli-core/src/codegen/entry/mod.rs; the variant emission in emit_cpp.rs and emit_rust.rs; packages/testing/nros-tests/tests/mode_variants.rs (new). Gate: cargo test -p nros-tests --test mode_variants; just mem-report. Status: W4a and W4b landed; W4c not started.

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

W5 re-plan (2026-10-17). Three facts were stale when this section was written:
- rlm carries the two policies already. `Qos.deadline_ms`,
  `lease_duration_ms`, `liveliness` and `lifespan_ms` exist since rlm
  v0.1.32, and play_launch writes them into `qos_overrides.*` itself, in
  nanoseconds, so the contract already reaches the runtime table on every
  road through `lower_all`.
- zenoh SERVES `deadline`. Its subscriber fires `RequestedDeadlineMissed`.
  It does not serve a subscriber-side lease, and xrce serves neither: a
  contract deadline on xrce is refused at entity creation, at boot.
- The zenoh graph token cannot carry a `deadline: not served` string. Its
  format is upstream rmw_zenoh's (issue 1495).

So W5 is two units.

- **W5a: landed in this change.** `qos_agreement` compares all eight
  modelled policies. It used to compare only the four capacity ones, which
  meant a launch `qos_overrides.*.deadline` silently outranked the contract's
  `deadline` (the resolver lets an authored parameter win).
  - Durations compare in the wire's milliseconds, each side converted by the
    runtime's own rule.
  - A disagreement names the contract's own key: `lease_duration` for
    `liveliness_lease_duration`.
  - `nros codegen-system`, the Zephyr road, now makes the same model refusals
    as the entry road and `nros::main!` (QoS agreement and FTTI). It made
    neither before.
  - Tests: `nros-orchestration-ir` `qos_agreement` (3 new cases) and
    `qos_override` (`a_liveness_statement_is_the_baked_value`).
    `contract_qos_override_agreement`'s
    `a_disagreeing_deadline_refuses_at_resolve` is the gate's first half.
    `agrees` gains its contract `deadline`.
- **W5b: not started.** A per-backend statement of which liveness policies
  each backend serves:
  - a const on `nros_rmw::Session`, plus `nros-rmw.toml` for the C backends,
    which share `CffiSession`;
  - a contract `deadline` the backend does not serve lowers to W2's silence
    row (`AgeMonitorSpec` gains a `silence_ms`, so the deadline does not also
    arm max-age), and its override code is stripped from that endpoint's
    baked table;
  - the build states the substitution through the image facts, not the graph
    token.

Claim: phase-462-W5. Depends on: phase-462-W2; an rlm release carrying deadline and liveliness on endpoints (after R4, no wave id yet). Owns: packages/core/nros-orchestration-ir/src/qos_override.rs; packages/core/nros-orchestration-ir/src/qos_agreement.rs; packages/rmw/cyclonedds/nros-rmw-cyclonedds/; packages/rmw/zenoh/nros-rmw-zenoh/src/shim/subscriber.rs; packages/rmw/xrce/nros-rmw-xrce/; a case in packages/cli/nros-cli-core/tests/contract_qos_override_agreement.rs. Gate: cargo test -p nros-orchestration-ir qos_agreement. Status: W5a landed; W5b not started.

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

W6a status: the knob landed. zenoh-pico's `_zp_unicast_read_task` takes
`Z_RX_TASK_BUDGET_FRAMES` / `Z_RX_TASK_BUDGET_REST_US` (fork `nano-ros` branch),
and nano-ros states them as `NROS_ZENOH_RX_TASK_BUDGET_FRAMES` / `_REST_US`, two
wire knobs on the RFC-0049 ladder. The phase's earlier spelling,
`NROS_RX_TASK_BUDGET_FRAMES`, became `NROS_ZENOH_*` like every other zenoh knob,
and the burst twin is the rest interval, RFC-0074's `REST`. Roads:
- cargo: `nros-zpico-build` writes the defines into `zenoh_generic_config.h`,
  and only when a rung STATED the frame budget;
- Zephyr: `nros_rmw_zenoh.cmake` writes them from the resolved value.
-1 or unset bakes nothing; 0 is refused by the build and by zenoh-pico's own
`#error`. The unit test is
`nros-zpico-build` `the_read_task_budget_is_baked_only_when_stated`.

Not yet run: the gate's flood cell. It lives in the evaluation workspace RFC-0074
measured in (`results/issue506_task_budget.md`), not in this tree, so the
0-stall rerun is still owed. The fork commit is the same shape as the one RFC-0074
measured: 12/12 budgeted runs with 0 stalls.

Claim: phase-462-W6a. Depends on: none. Owns: zephyr/Kconfig; zephyr/nros_zenoh_zephyr_system.c; the knob's ladder entries as scripts/check-knob-delivery.py and scripts/check-kconfig-knob-forwarding.sh require them; docs/design/0074-ingress-budget-rate-and-burst.md. Gate: the RFC-0074 flood cell rerun with the budget stated records 0 stalls. Status: knob landed; flood-cell rerun pending.

W6b status: the derivation landed. rlm v0.1.64 settled issue 0760
(`sub.<e>.ingress: { rate_hz, burst }`, both required, top level on the
endpoint), and the entity inventory reads it (`RxTaskBudget::from_model`): the
subscriber with the largest `rate_hz x burst` gives `FRAMES = burst` and
`REST_US = round(burst x 10^6 / rate_hz)`, clamped into the Kconfig range
`1..=1000000`. A multi-image merge keeps the heavier PAIR. Roads:
- Zephyr: `NROS_DERIVED_ZENOH_RX_TASK_BUDGET_FRAMES` / `_REST_US` in the
  inventory fragment, resolved by `_nros_resolve_derivable_knob`. Both Kconfig
  defaults are now the `-1` DERIVE sentinel (REST_US was 1000; the C lane and
  `nros-zpico-build` keep 1000 as the builtin), so -1 is "not stated".
- cargo (leaf, workspace, cmake): the sizing descriptor's
  `[image] rx_task_budget_frames` / `_rest_us`, which `nros-zpico-build` reads
  as a rung below every stated one. Deliberately NOT a leaf-sidecar `[env]`
  key: the wire tenant resolves env-first, so a sidecar row would outrank a
  board's `[knobs.zenoh.wire]` value.
No ingress declared: nothing derived, the descriptor fields refused, the task
unbounded and the image byte-identical. Tests: `entity_inventory`
`the_read_task_budget_follows_the_declared_ingress` and
`the_read_task_budget_rounds_clamps_and_merges_as_a_pair`, `resolve`
`resolved_toml_and_cmake_agree_on_the_read_task_budget`, `sizing_descriptor`
`the_read_task_budget_is_stated_only_when_ingress_is_declared`.

Follow-up, not in W6b:
- ~~`c = 0`.~~ **Landed as W6c (below).**
- The gate's "derived pair equals the hand-stated pair on the flood cell's
  contract" needs the flood cell, which lives outside this tree (see W6a).

Claim: phase-462-W6b. Depends on: phase-462-W6a; rlm issue 0760. Owns: the knob derivation in packages/cli/nros-cli-core/src/entity_inventory.rs and a test beside it. Gate: cargo test -p nros-cli-core entity_inventory. Status: derivation landed (c = 0); the per-board cost and the slack check landed as W6c.

W6c status: the per-board frame cost and RFC-0074's two resolve-time checks
landed.
- **Where `c` lives.** `[board.rx]` in the board's `nros-board.toml`,
  `frame_cost_us` (required) and `floor_us` (optional), typed by
  `RxFrameCost` (`deny_unknown_fields`) as `BoardDescriptor::rx`. A MEASURED
  fact, not a `[knobs]` rung: no compiler reads it, and a knob would have
  grown an env front-end and a ladder for a number nothing sets per build.
  `nros-board-mps2-an385-freertos` states RFC-0074's measurement
  (`frame_cost_us = 940`, `floor_us = 11000`); no other board states one.
  Absent: `c = 0`, nothing checked, the image byte-identical to W6b.
- **Derivation.** The inventory now keeps the declarations
  (`DeclaredIngress`, every subscriber's `rate_hz`/`burst` in key order) beside
  the W6b pair. A road that knows the board calls
  `EntityInventory::set_board_rx_cost`, and `rx_task_budget()` re-derives from
  the same heaviest subscriber as `REST = burst x (10^6/rate_hz - c)`
  (`RxTaskBudget::from_ingress_with_cost`; `c = 0` is `from_ingress` exactly).
- **Checks** (`check_rx_ingress`, RFC-0065 D2 refusals naming inputs and
  remedy): (1) `rate_hz <= 10^6 / c` for every declared subscriber; (2)
  `FRAMES x c + floor <= slack` of the tightest authored tier, where slack is
  `D - B` (`D` = the tier's `deadline`, else its `period`; `B` its `budget`).
  A tier with no `budget` is checked against its whole deadline, an UPPER bound
  of its slack: the refusal is sound and the pass is said to be "within the
  deadline". A model whose tiers state no `deadline`/`period` has no slack to
  compute, and constraint 2 is reported `NOT CHECKED` with the reason (stderr,
  on every road) -- never a guessed slack. Only the portable head is read; a
  platform sub-table that tightens a deadline is not seen (an over-stated
  slack, the same direction). A stated `frame_cost_us = 0` is refused.
- **Roads.** The cost reaches every producer of the pair: the cargo leaf road
  and the Zephyr west Rust leaf (`LeafImage::rx_cost` -> `write_for_leaf`,
  `west-leaf-sizing`); the workspace cargo road (stage 3.5 sets it on the
  composition, so `resolved.toml` and the descriptor agree; stage 3.5 does not
  check, since it cannot fail a build, and `write_for_model` does); the cmake
  and west configure lines (`-DNROS_BOARD_RX_FRAME_COST_US` /
  `_FLOOR_US`, always passed and empty when unstated -- issue 1653's cache
  reason), forwarded by `cmake/NanoRosBoardRxCost.cmake` to both
  `nros ws sizing-descriptor` (`--board-rx-frame-cost-us` /
  `--board-rx-floor-us`) and `nros ws entity-inventory` (the Zephyr fragment's
  `NROS_DERIVED_ZENOH_RX_TASK_BUDGET_*`). A refusal fails the producer, hence
  the configure / the build. A bare `cmake` or `west build` passes nothing and
  derives at `c = 0`.
- Tests: `entity_inventory`
  `the_board_frame_cost_shortens_the_rest_and_absent_keeps_it_identical`,
  `a_rate_the_board_cannot_receive_is_refused_naming_the_subscriber` (with its
  negative control under the ceiling), `a_burst_longer_than_the_tightest_tier_slack_is_refused`
  (with its negative control one floor smaller, and the budget-less upper-bound
  case), `the_slack_check_is_stated_as_not_checked_without_tier_timing`;
  `sizing_descriptor` `the_model_road_refuses_an_ingress_the_board_cannot_receive`;
  `board_descriptor` `board_rx_states_the_frame_cost_and_refuses_a_typo`;
  `build` `the_board_rx_cost_reaches_the_configure_line_and_clears_when_unstated`.
- Not done: one `c` per board is RFC-0074's stated approximation (payload size
  almost certainly enters it); the check reads every authored tier as sharing
  the read task's core, the stricter reading, because the model places no tier
  on a core.

Claim: phase-462-W6c. Depends on: phase-462-W6b. Owns: `[board.rx]` (`BoardDescriptor::rx`), `RxFrameCost` / `DeclaredIngress` / `TierSlack` / `check_rx_ingress` in packages/cli/nros-cli-core/src/entity_inventory.rs, the road plumbing, `cmake/NanoRosBoardRxCost.cmake`. Gate: cargo test -p nros-cli-core entity_inventory. Status: landed.

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
phase-495 (the shared derivation) and phase-459 (the tier derivation); the
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

## Open issues that name this phase

Open issues whose `related:` names this phase (listed 2026-10-12, so the
link runs both ways). Listing one here records the link, not a claim that
this phase will fix it.

- [issue 0506](../issues/0506-transport-band-unbounded-preemption.md) — Transport tasks above application tiers is the right default but has no budget — inbound overload preempts every tier …

## Issues homed here (2026-10-17)

Homed when every open issue was given a phase. Each issue has a `**Home:**` line pointing here.

| issue | why it belongs here |
| --- | --- |
| [#1374](../issues/1374-contract-cannot-express-qos-durability.md) | A contract's `qos:` carries only depth, so durability the code depends on is invisible to the contract, which is the vocabulary this phase brings on target. |
