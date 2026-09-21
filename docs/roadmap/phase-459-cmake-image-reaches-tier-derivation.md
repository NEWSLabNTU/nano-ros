# phase-459 - a C++ image reaches the rate-monotonic tier derivation from what it authors

**Status (2026-09-21). PROPOSED; nothing landed.** Numbered 459 because
phases 456-458 were being opened concurrently by other sessions; this is
highest-existing (455) + 4. Its sibling, phase-460, covers the formal checks the
same investigation found missing on the contract chain.

Owns [issue 1426](../issues/1426-cmake-callback-groups-never-reach-codegen-system-and-the-entry-never-derives.md)
and [issue 1427](../issues/1427-realizer-allocates-rank-zero-above-the-transport-band.md).
Closes the reachability half of
[issue 1371](../issues/1371-tier-derivation-silent-on-empty-callback-groups.md)
(the silence half stays 1371's) and takes item 2 of
[issue 1372](../issues/1372-contract-trigger-rate-is-dead-data.md) as a
dependency, not as its own work. Builds on RFC-0047 (groups in code, bindings in
`system.toml`), RFC-0052 (the realizer), RFC-0079 (priority is allocated, not
authored) and RFC-0032 section 5.1 (the degenerate gate).

## Why

The rate-monotonic derivation is complete code with tests
(`packages/core/nros-orchestration-ir/src/derive.rs`, tests from line 285), and
for a C++ image built through cmake and west it is unreachable from any authored
input. Measured on the Autoware Safety Island (four C++ components, one timer
each at 30 Hz or 10 Hz, brief D experiments E5b-E5e against pin f8655e9b7 on
2026-09-18; every line below re-verified against `783cdfa14`):

* `[[component]] group_tiers = { main = "ctrl" }` with no `[tiers.ctrl]` is
  refused: `SystemModel binding '/x/main' references undeclared tier 'ctrl'`
  (`packages/cli/nros-cli-core/src/orchestration/model_ingest.rs:108`).
* With `[tiers.ctrl]` declared, derivation is skipped: the call site at
  `packages/cli/nros-cli-core/src/cmd/codegen_system.rs:433` runs only when
  `model.execution.tiers.is_empty()` ("declared tiers always win").
* With groups but no bindings (a model resolved without `--system`), the bake
  refuses at `model_ingest.rs:212` because the `group_tiers` reached no node.
* The one route that would work - groups with no bindings and no tiers - has no
  producer on the cmake road. `codegen-system` collects groups through
  `collect_callback_groups`
  (`packages/cli/nros-cli-core/src/orchestration/tier_resolver.rs:35`), which
  reads `[[component]].group_tiers` and then `cfg.component_packages`, and
  `NrosConfig::from_workspace` returns `component_packages: BTreeMap::new()`
  for any workspace without a root `Cargo.toml`
  (`packages/cli/nros-cli-core/src/orchestration/nros_config.rs:198-212`). The
  cmake keyword `CALLBACK_GROUPS` exists (`cmake/NanoRosVerbs.cmake:255` on
  `nano_ros_add_node`, `:441` on `nros_components_register_node`, written to
  `nros-metadata.json` by `cmake/NanoRosNodeRegister.cmake:1252`) and its
  output is read by exactly one consumer: `codegen entry`'s
  `metadata::enrich_plan`. So the island's `build-board/nros-metadata.json`
  carries `callback_groups: []` for every component because the island never
  wrote the keyword, and had it written the keyword, `codegen-system` would
  still not have seen it.
* Even if `codegen-system` derived tiers, the image would not run them.
  `codegen entry` builds its plan from `model.execution.tiers`
  (`packages/cli/nros-cli-core/src/codegen/entry/mod.rs:918`, inside
  `plan_from_model` at `:751`) and calls no derivation; the derived tiers live
  in `codegen-system`'s in-memory `SystemToml` and reach only
  `nros-system/nros-plan.json` (`codegen_system.rs:1135`). E5e's `run_tiers`
  output came from an AUTHORED `[tiers.ctrl.zephyr] priority = 5`, written
  three times (tier, sub-table, binding), which is exactly the shape RFC-0079
  retires.

Two more facts change what "reaching it" has to mean:

* **The allocation ignores the board's address plan.** `rank_to_priority`
  (`packages/core/nros-orchestration-ir/src/rtos_realizer.rs:336-346`) maps
  dense rank 0 to Zephyr priority 0 (`sched_caps_for("zephyr")`: 32
  priorities, `low_number_is_high`). The island's transport threads sit at
  Zephyr preemptive 4 (`CONFIG_NROS_ZENOH_READ_PRIORITY=200` on the 0..255
  band of `zephyr/Kconfig:461-469`, mapped by
  `packages/platform/nros-platform-zephyr/src/platform.c:501-503` against
  `CONFIG_NUM_PREEMPT_PRIORITIES=15`). A derived control tier at 0 would
  outrank the transport that feeds it - the inversion RFC-0079 exists to
  prevent. The Zephyr plan is declared as DERIVED
  (`packages/boards/zephyr/nros-board.toml:50-62`, resolver
  `scripts/lib/priority_plan.py:resolve_zephyr_plan`, judged by
  `scripts/check-tier-priority-plan-image.py`), and nothing in
  `nros-orchestration-ir` reads it: `grep priority_plan packages/core` is
  empty. Issue 1427.
* **Equal periods collapse to one rank.** The pinned ranker
  (`ros-launch-manifest-sched` v0.1.35, `chain_aware_mapper.rs:364-375`)
  gives every fact with the same `(criticality, budget_ms)` one `fine_group`,
  and `dense_node_ranks` (`rtos_realizer.rs:306-333`) gives a fine group one
  rank. The island's projection in its `docs/nxp-deployment.md` section 7 (two
  30 Hz nodes at rank 0, two 10 Hz nodes at rank 1) is what the code does. The
  claim that `rate_monotonic` spreads equal periods across unequal priorities
  by rank is NOT what this pin does and is not repeated here.

What is already landed and therefore not in this phase: the `--target
zephyr-<rmw>` naming (E5d) was issue 1312 / issue 1397; the Zephyr module now
passes `--for-entry` (`zephyr/cmake/nros_system_generate.cmake:217-224`) and
the platform is resolved once from the image that claims the entry
(`codegen_system.rs:275-310`). A hand-run `codegen-system --target
zephyr-cyclonedds` still names no block and falls back to the host tier key;
W5 gives that form a refusal.

## What it does

A C++ component states its callback groups once, in the cmake call that already
registers it, and that statement reaches every consumer. A binding whose tier
asks to be derived is a request, not an error. The derived priority is
allocated inside the board's application pool, below the transport band the
image's Kconfig implies. The entry emits `run_tiers` from the derived table.
The island then writes one line per component and nothing else.

### The authoring surface, decided

1. **Groups: the cmake keyword, unchanged.** `CALLBACK_GROUPS main` on
   `nros_components_register_node` (the island's verb) or `nano_ros_add_node`.
   No generated registry from `create_callback_group` calls: C++ code does not
   have to create the group at all for node-level placement, because the C++
   emitter places NODES on tiers (`emit_cpp.rs:423-433`, `node_to_tier`) and
   the group name is the metadata token the binding keys on. A component that
   later splits itself (RFC-0047 sub-node) creates the second group in code
   and adds it to the keyword; the two must agree, and W6 makes disagreement a
   refusal. The contract grammar stays as it is: a group is a property of the
   code, a rate is a property of the path, and rlm has only
   `concurrency: { exclusive: [...] }` by design (groups are "never authored"
   there).
2. **The derive request: `derived = true` on the tier, not an undeclared
   name.** `[tiers.ctrl] derived = true` declares the tier's NAME and asks the
   realizer for its priority per target RTOS. A binding to a tier that is
   neither declared nor marked stays a refusal, because a typo in a tier name
   must not silently become a derived tier. `derived = true` is exclusive with
   any RTOS sub-table (`[tiers.ctrl.zephyr]` beside it is refused with both
   lines named); `spin_period_us` beside it is accepted as the RFC-0079
   override and warned. Both `[tiers.*]` parsers gain the key: rlm
   `model/src/system_config.rs` (`deny_unknown_fields`, so the key needs a
   reader before a pin bump) and nano-ros
   `orchestration/cargo_metadata_schema.rs::TierDef`. The zero-grammar route
   (groups, no bindings, no tiers) keeps working and is what W1 tests first,
   so the island can move before the rlm pin does.
3. **Placement is node-level unless the code says otherwise.** A component with
   exactly one declared group binds whole; a component with several is placed
   per group, which is the RFC-0047 sub-node path the C++ emitter already has a
   test for (`emit_cpp.rs`, `typed_emit_group_split_node_falls_back_to_sched_context_path`).

### Waves

**W0 - the fixture.** A cmake workspace under `examples/workspaces/` with four
C++ components mirroring the island's shape: two timers at 30 Hz, two at 10 Hz,
a provider contract with `paths.*.trigger.timer.rate_hz` and matching
`pub.min_rate_hz`, `CALLBACK_GROUPS main` on every registration, no
`group_tiers`, no `[tiers.*]`. `examples/workspaces/realtime-cpp` is the
authored-tier neighbour and is not modified. The expected table is the island's
projection: `mrm_emergency_stop_operator` and `stop_mode_operator` on the most
urgent derived tier, `mrm_comfortable_stop_operator` and `mrm_handler` one
below. The fixture is what every later wave's gate runs against.

**W1 - `codegen-system` reads the cmake metadata for groups.**
`collect_callback_groups` gains a third source after `group_tiers` and the cargo
manifest: the workspace's `nros-metadata.json` files, through the
`load_workspace_metadata` reader `model_ingest.rs:344` already uses for callback
slot counts (one reader, not a second parse). A component whose metadata
declares groups and whose `system.toml` row declares none binds each group to
`DEFAULT_TIER`, which is the input shape `derive_tiers_from_contracts` keys on
(`derive.rs:96`). Gate: on the W0 fixture, `codegen-system` reports `derived 2
scheduling tier(s)` and `nros-plan.json` carries the two tiers with the four
members; the negative control is the same fixture with the keyword removed,
which must produce the groupless note for all four (issue 1371's persisted
form).

**W2 - the entry derives, or reads what the bake derived.** `plan_from_model`
runs the same `derive_tiers_from_contracts` when `model.execution.tiers` is
empty and any node carries groups, with the same target RTOS key the board
resolves to (`board_to_rtos`), so `codegen entry --board zephyr` and
`codegen-system --for-entry zephyr_entry` cannot disagree. The alternative -
the entry reading `nros-plan.json` - was rejected because the entry is
generated by `nano_ros_add_executable` before and independently of the Zephyr
module's bake, and a file dependency between the two would be a second,
ordered reading of one fact. Gate: `codegen entry --lang cpp --board zephyr` on
the W0 fixture ends in `ZephyrBoard::run_tiers(..., __nros_tiers, 2u)`, with
`__nros_tier_0_groups` naming the two 30 Hz nodes and `__nros_tier_1_groups`
the two 10 Hz nodes. `derive.rs` already refuses a `sched_class` the target
cannot honour; this wave adds no dimension.

**W3 - `derived = true`.** The marker in both parsers (item 2 above), the
exclusivity rule, and the spin-period override warning. The rlm half is a
pinned-dependency change and lands first, as its own PR against
ros-launch-manifest, before the nano-ros half bumps the pin. Gate: the W0
fixture with `[tiers.ctrl] derived = true` and `group_tiers = { main = "ctrl" }`
on every component produces the SAME two tiers as W1 (named `ctrl` split by
rank, or `derived-*` with a `ctrl` alias; the test asserts membership and
order, not the name); `[tiers.ctrl] derived = true` plus
`[tiers.ctrl.zephyr] priority = 5` is refused naming both lines; a binding to
`ctrll` is still refused as undeclared.

**W4 - the allocation honours the board's plan.** `realize_rtos` takes a
`PriorityPlan` beside `SchedCaps`. For a STATIC plan (FreeRTOS, NuttX, ThreadX
descriptors) the pool is read from the descriptor; for the DERIVED Zephyr plan
the pool is resolved from the image's `.config` by the same arithmetic
`scripts/lib/priority_plan.py:resolve_zephyr_plan` performs, moved into Rust
under `nros-orchestration-ir` with the script kept as the checker of the Rust
result (two implementations of one formula, one of them a test, not two
sources). Dense rank 0 maps to the most urgent priority INSIDE `pool.app`, and
ranks past the pool's width are clamped with a recorded `Degradation`. On the
island's `.config` (15 preemptive priorities, read and lease bands 200/255)
the transport resolves to Zephyr 4 and the pool to `[5, 14]`, so the derived
table is 30 Hz at 5, 10 Hz at 6, not 0 and 1. The projection in the island's
section 7 changes by that offset, and this phase's documentation update says
so. POSIX stays as RFC-0079 records it (FIFO tiers, SCHED_OTHER transport,
"half-solved"): this wave allocates inside the executor's ordering space there
and does not claim a kernel band. Gate: the W0 fixture's Zephyr bake produces
priorities within the resolved pool, and `check-tier-priority-plan-image.py`
run on the fixture's `.config` reports zero violations; the negative control
pins `CONFIG_NROS_ZENOH_READ_PRIORITY=16` (the pre-0852 band) and the resolver
must report the stale band, as the script's own selftest already does.

**W5 - the hand-run form is refused.** `codegen-system --target <id>` where
`<id>` names no `[image.*]` / `[deploy.*]` block in the bringup refuses with
the list of blocks it could have named, instead of resolving the tier RTOS to
the host. The Zephyr module already passes `--for-entry` and is unaffected.
Gate: a unit test beside `resolve_target_block`.

**W6 - code and keyword agree.** A group created in code that the registration
did not declare, or declared and never created when the node has more than one
declared group, is refused at node construction with a code naming both
(shape of `DECLARED_DEPTH_MISMATCH`, `nros/node.hpp`). Node-level placement
(one declared group, none created in code) is explicitly allowed and is the
island's case. Gate: the existing `realtime-cpp` fixture is unchanged; a new
negative test declares `CALLBACK_GROUPS ctrl telem` and creates only `ctrl`.

**W7 - the trigger rate, once issue 1372 item 2 lands.** The mapper reads
`trigger.timer.rate_hz` in preference to the first output's `min_rate_hz`.
This phase depends on it and does not implement it; until it lands the W0
fixture keeps the two numbers equal, as the island does, and a comment in the
fixture's contract says why.

### What the island then writes

One line per component, in each package's `CMakeLists.txt`:

```
nros_components_register_node(mrm_handler_lib
    PLUGIN autoware::mrm_handler::MrmHandler
    EXECUTABLE mrm_handler
    HEADER autoware/mrm_handler/mrm_handler_core.hpp
    CALLBACK_GROUPS main)
```

and nothing in `system.toml` (W1 route), or additionally `[tiers.ctrl] derived
= true` plus `group_tiers = { main = "ctrl" }` per component once W3 lands, if
the island wants its commented `spin_period_us = 33_333` honoured as the
authored override. No C++ source changes. The island's own gate is the one it
already has for knobs (`check-knob-delivery.py`): the generated entry must end
in `run_tiers`, and the map must no longer list `.bss.nros_tier_threads` under
discarded sections.

## Gates for the phase

| gate | where | what it asserts |
| --- | --- | --- |
| cmake groups reach the bake | W1 unit + fixture test in `nros-cli-core` | W0 fixture: `derived 2 scheduling tier(s)`, four members, order 30 Hz before 10 Hz; keyword removed: four groupless notes |
| the entry runs what was derived | W2 fixture test on `codegen entry --lang cpp --board zephyr` | output ends in `run_tiers(..., 2u)`; group tables match the plan |
| marker semantics | W3 unit tests in both parsers | derived tier accepted; sub-table beside marker refused; undeclared name still refused |
| pool-respecting allocation | W4 unit test + `check-tier-priority-plan-image.py` on the fixture `.config` | every derived priority inside `pool.app`; transport band untouched |
| hand-run target refused | W5 unit test | `--target` naming no block is an error listing the blocks |
| code/keyword agreement | W6 negative test | mismatch refused at construction with both names |

The whole set runs in the fast tier (`just ci-l1`); none needs a board.

## Limits

* This phase derives ORDER and PRIORITY only. `core`, `time_slice_us`,
  `preempt_threshold`, budgets and deadlines stay authored (phase-330 W1.a);
  the island declares none of them and no path declares `max_latency_ms`, so
  the EDF path stays unreachable there by the island's own choice.
* Assignment is per node, or per declared group. No callback is placed by its
  own rate; a node whose subscriptions arrive faster than its timer fires runs
  them at the node's tier.
* Equal periods share a priority. If a workspace needs 30 Hz control above
  30 Hz telemetry, it says so with `criticality`, which the ranker orders
  first; this phase does not invent a tiebreak.
* The measurement is not in this phase. Whether the 30 Hz path meets 33 ms on
  the S32K344 needs the image to link (the island is 53,912 B over its RAM
  with parameter services on) and a run on silicon; the gates here prove the
  table, not the timing.
* WCET stays absent (RFC-0078: no measurement has ever flowed), so the realizer
  keeps reporting `ChainFeasibleWithoutWcet`.

## Docs to update

* `docs/design/0079-priority-is-allocated-not-authored.md` section 4.1 still
  describes the zenoh band as 0..31 with default 16; the Kconfig has been
  0..255 with default 200 since phase-364 W5. Correct the chain and the worked
  example in the same change as W4, which is the wave that makes the RFC's
  pool the realizer's input.
* `docs/design/0032-*` section 5.1: the degenerate gate now has a producer on
  the cmake road; say which keyword.
* `docs/design/0047-*`: add the `derived = true` form beside the authored
  example, and mark the `[tiers.high.posix] priority = 80` example as the
  form RFC-0079 replaces.
* The island's `docs/nxp-deployment.md` section 7: the projection becomes a
  measured table once W1, W2 and W4 land, with the pool offset from W4.
* `packages/platform/nros-platform/src/board/tier.rs` module doc still says
  priorities are on a normalized 0..31 scale; `TierSpec::priority` is raw per
  kernel on the codegen path. Correct it when W4 touches the file.

## Explicitly not in this phase

* Persisting the groupless note into the plan and the summary line at zero:
  issue 1371, items 1-3, small and independent.
* Carrying `trigger.timer.rate_hz` into the model: issue 1372.
* The POSIX band question ("what reserves the transport when the tiers are
  FIFO and the transport is not"): RFC-0079's open question, untouched.
* A `TierSpinGap` change or any spin-period derivation beyond the override
  warning.
