---
id: 1426
title: "For a cmake image the tier derivation is unreachable twice: codegen-system
  collects callback groups from cargo metadata only, and codegen entry never
  derives at all - derived tiers reach nros-plan.json and no image"
status: resolved
type: bug
area: [cli, codegen, cmake]
severity: high
found: 2026-09-21
related: [issue-1371, issue-1372, issue-1312, issue-1397, issue-1427, phase-459, rfc-0032, rfc-0047, rfc-0052, rfc-0079]
resolved_in: "fix(#1426): the derivation gate asks about a TABLE where the rule is a FACT"
---

## What was observed

Brief D experiments E5b-E5e on the Autoware Safety Island (four C++
components registered with `nros_components_register_node`, one timer each
at 30 Hz or 10 Hz), pin f8655e9b7, re-verified below against 783cdfa14:

* `[[component]] group_tiers = { main = "ctrl" }` without `[tiers.ctrl]`:
  `codegen-system` refuses, `references undeclared tier 'ctrl'`
  (`packages/cli/nros-cli-core/src/orchestration/model_ingest.rs:108`);
  `codegen entry` refuses, `names tier ctrl, which has no [tiers.ctrl]
  definition`.
* With `[tiers.ctrl]` declared: derivation is skipped, because
  `codegen_system.rs:433` derives only when `model.execution.tiers.is_empty()`.
* Groups with no bindings, model resolved without `--system`: refused at
  `model_ingest.rs:212`, the `group_tiers` reached no node.
* With an authored `[tiers.ctrl.zephyr] priority = 5` (E5e) the entry ends in
  `run_tiers` - from the AUTHORED number, written three times.

`build-board/nros-metadata.json` carries `callback_groups: []` for all four
components. The realizer never ran on this image and could not have been made
to from any authored input.

## The two gaps (verified at 783cdfa14)

**Gap 1 - `codegen-system` cannot see cmake groups.** `collect_callback_groups`
(`packages/cli/nros-cli-core/src/orchestration/tier_resolver.rs:35-90`) reads
`[[component]].group_tiers`, then `cfg.component_packages[pkg].nros.callback_groups`.
`NrosConfig::from_workspace` (`orchestration/nros_config.rs:198-212`) returns
`component_packages: BTreeMap::new()` for a workspace with no root
`Cargo.toml`. The cmake keyword `CALLBACK_GROUPS` (`cmake/NanoRosVerbs.cmake:255`
on `nano_ros_add_node`, `:441` on `nros_components_register_node`, emitted by
`cmake/NanoRosNodeRegister.cmake:1252` into `nros-metadata.json`) is read
only by `codegen entry`'s `metadata::enrich_plan`. So on the cmake road the
only way to give `codegen-system` a group is `group_tiers`, and a
`group_tiers` binding needs a declared tier, and a declared tier disables
derivation. The gate in `derive.rs:96` ("a node with no groups stays on the
default tier") is therefore always taken for a cmake image, whatever the
author writes.

**Gap 2 - the entry never derives.** `codegen entry` builds its plan in
`plan_from_model` (`packages/cli/nros-cli-core/src/codegen/entry/mod.rs:751`),
takes tiers from `model.execution.tiers` (`:918`) and resolves them with
`resolve_plan_sched` (`:985`); `grep derive_ packages/cli/nros-cli-core/src/codegen/entry`
is empty. The derived tiers `codegen-system` produces live in its in-memory
`SystemToml` and reach `nros-system/nros-plan.json` (`codegen_system.rs:1135`)
and nothing else. So even with gap 1 closed, `run_tiers` would not be
emitted for a derived schedule; every `run_tiers` in the tree today comes
from an authored `[tiers.*.<rtos>]`.

RFC-0032 section 5.1 describes the degenerate gate as the case where nothing
was declared. On the cmake road it is the only case.

## What is not this issue

* The silence of the groupless note: issue 1371.
* The trigger rate being rebuilt from the output's `min_rate_hz`: issue 1372.
* `--target zephyr-<rmw>` naming no block: issues 1312 and 1397, resolved;
  the Zephyr module passes `--for-entry` (`zephyr/cmake/nros_system_generate.cmake:217-224`).
* The allocation placing rank 0 at Zephyr priority 0, above the transport:
  issue 1427.

## What would fix it

phase-459 W1 and W2: `collect_callback_groups` gains the workspace metadata
as a third source through the `load_workspace_metadata` reader
`model_ingest.rs:344` already uses; `plan_from_model` runs
`derive_tiers_from_contracts` when the model declares no tiers and any node
carries groups, with the board's RTOS key. W3 adds `[tiers.X] derived = true`
so a binding can name a tier and still ask for its priority.

## Acceptance

phase-459's W0 fixture (four components, `CALLBACK_GROUPS main`, no
`group_tiers`, no tiers): `codegen-system` derives two tiers;
`codegen entry --lang cpp --board zephyr` ends in `run_tiers(..., 2u)` with
the two 30 Hz nodes in tier 0 and the two 10 Hz nodes in tier 1. Keyword
removed: four groupless notes, `run_components`.
## Resolution

Measured at `4d439a115` before anything changed, because two of this issue's
bullets had already been fixed by phase-459 W1/W2 and the record has to say
which.

### What was still true at `4d439a115`, bullet by bullet

| the issue said | at `4d439a115` | why |
| --- | --- | --- |
| **Gap 1** — `codegen-system` collects groups from cargo metadata only, so a cmake image's groups never arrive | **CLOSED** | phase-459 W1 (`6918743af`). `tests/derived_tiers_bake.rs::the_cmake_keyword_makes_the_fixture_derive_a_schedule` passed on the pre-fix tree: four components, four placements, two ranks, no groupless note |
| **Gap 2** — `codegen entry` never derives at all | **CLOSED** | phase-459 W2 (`dca14a130`). `derived_tiers_entry::the_entry_derives_its_tiers_and_emits_run_tiers` passed on the pre-fix tree: `ZephyrBoard::run_tiers(..., 4u)`, 30 Hz at 5 and 10 Hz at 6 |
| **bullet 1** — `group_tiers` with no `[tiers.ctrl]` is refused by both | **STILL TRUE, and kept** | a typo in a tier name must not become a derived tier. `derived_tiers_precedence::a_binding_to_an_undeclared_tier_is_refused_by_name` |
| **bullet 2** — with `[tiers.ctrl]` declared, derivation is skipped (`codegen_system.rs:433`) | **STILL TRUE — this is what was fixed** | measured by the pre-fix tree's own test: `an_authored_tier_table_is_not_replaced_by_the_derivation` asserted `plan.tiers.keys() == ["ctrl"]`, "nothing was derived beside it" |
| **bullet 3** — groups with no bindings, model resolved without `--system`, refused at `model_ingest.rs:212` | **STILL TRUE, and kept** | `nros sync` passes `--system` whenever the file exists (`cmd/ws.rs`), so no supported road produces the pairing; silently dropping the binding is issue 0398 |
| **bullet 4** — an authored `[tiers.ctrl.zephyr] priority = 5` reaches `run_tiers` from the AUTHORED number | **STILL TRUE, and correct — but it was SILENT** | the pre-fix test asserted `prio["mrm_handler"] == 7` and nothing else; nothing said what the number beat, and nothing derived for the other three nodes |

The `build-board/nros-metadata.json` carrying `callback_groups: []` for all four
components is also settled and was never the code's fault: a REAL configure of
the W0 fixture, run for this acceptance, writes `["main"]` for all four. The
island's `[]` is the island not having written the keyword.

### The `is_empty()` guard: the decision is right, its SCOPE was wrong

"Authored wins" is the correct rule. It was stated over the whole `[tiers.*]`
TABLE, and the rule is about one FACT — one tier's placement for one target
RTOS. Consequence, on the W0 fixture with one component bound to one authored
`[tiers.ctrl.zephyr]`: the other three components, each with a 30 or 10 Hz timer
and a declared `CALLBACK_GROUPS main`, got no derived tier at all, and nothing
was printed about it. What that costs is not stated here from a reading — it is
what the pre-fix test ASSERTED, `plan.tiers.keys() == ["ctrl"]`, "nothing was
derived beside it". Where those three then run is the synthesised default tier,
whose `priority` is the literal `0` in `resolve_tiers`' `default_tier()`
(`nros-orchestration-ir/src/lib.rs`); on Zephyr that is more urgent than the
transport band at 4, so the same inversion issue 1427 fixes in the ALLOCATOR was
reachable through this GUARD. The allocator half is 1427's; this half is that the
allocator was never asked.

Precedence is now per fact, stated once on
`nros_orchestration_ir::derive::placement_is_unauthored`, and asked by all
three sites that used to spell it themselves (`cmd::codegen_system`,
`codegen::entry::derive_entry_tiers`, `nros::main!`):

1. `[tiers.<n>.<rtos>]` authored for this target ⇒ **AUTHORED**, never
   overwritten, and recorded in `DerivedSchedule::shadowed` with the rank it
   beat. Every caller prints one line naming the tier, the authored number and
   the allocation it displaced; the bake also carries it into
   `nros-plan.json`'s `sched_warnings` (issue 0259's rule: a verdict that
   exists only in scrollback cannot be audited).
2. `[tiers.<n>]` authored with **no platform sub-table at all** ⇒ **ALLOCATED**
   for that tier, at the most urgent rank among its members, installed by
   `install_placement` so the authored head (`class`, `period_us`, …) is
   untouched. Naming a tier is how a `group_tiers` binding gets written; it says
   nothing about a priority, and RFC-0079 is that a priority is allocated rather
   than authored. Members that do not share a rank are a recorded
   `Degradation` — one tier is one thread at one priority, so collapsing two
   ranks loses the rate-monotonic split between them.
3. sub-tables for OTHER targets and not this one ⇒ **neither**. The author did
   place it, for a different board; `TierResolveError::MissingRtosSpec` still
   refuses, and now names both ways out.
4. the group names no tier (`DEFAULT_TIER`) ⇒ **ALLOCATED** as
   `derived-<node>`, as before.

Rule 2 is what makes the issue's dead end — "a binding needs a declared tier,
and a declared tier disables derivation" — have an exit, and it needs **no new
key in the `[tiers.*]` schema**. That matters for reachability: the schema is
`ros-launch-manifest`'s `sched/src/types.rs::TierDef`, which is
`#[serde(deny_unknown_fields)]`, so phase-459 W3's `[tiers.X] derived = true`
is a hard parse error at resolve time until that pin moves. W3 is therefore
still open as a spelling; what it was FOR is reachable today.

### Acceptance — an IMAGE, not a JSON

The issue's point is that derived tiers reached `nros-plan.json` and no image,
so this is measured on a built binary.

The fixture, copied and given the one thing it did not have: a tier with a NAME
and no priority, plus the binding that needs the name.

```toml
[[component]] … name = "mrm_handler"
group_tiers = { main = "ctrl" }        # ×4, one per component
[tiers.ctrl]  class = "real_time"      # no [tiers.ctrl.posix], no [tiers.ctrl.zephyr]
```

`nros sync .` then `nros build demo_bringup:native --workspace . --offline` in
`examples/workspaces/derived-tiers-cpp`, native cmake image, `[image.native]
board = "native"`.

**Step 1 — a real configure writes the keyword.**
`build/nros-metadata/metadata-probe-cmake/build/nros-metadata.json` carries
`callback_groups: ["main"]` for all four components. The island's `[]` was the
island not having written `CALLBACK_GROUPS`, not a reader that could not see it.

**Step 2 — the model carries the request.**
`build/nros/models/demo_bringup/system_model.yaml`:
`execution.tiers = {ctrl: {class: real_time}}` (a name, no placement) and four
`execution.bindings` rows `/<node>/main -> ctrl`. Before this fix that is the
exact shape the table-shaped guard read as "tiers are authored, derive nothing".

**Step 3 — the ALLOCATION reaches the emitted image source**, which is the layer
this issue says a fix has to reach. `build/posix-zenoh-native/cmake/
native_entry_nros_main_generated.cpp`:

```c++
static const char* __nros_tier_0_groups[] = {
    "mrm_comfortable_stop_operator", "/", "main",
    "mrm_emergency_stop_operator",   "/", "main",
    "mrm_handler",                   "/", "main",
    "stop_mode_operator",            "/", "main", };
static const ::nros::board::NativeTierSpec __nros_tiers[1] = {
    { .name = "ctrl", .groups = __nros_tier_0_groups, .n_groups = 4u,
      .priority = 89LL, …, .tier_class = "real_time", … }, };

int main(int, char**) {
    return ::nros::board::LinuxBoard::run_tiers(…, __nros_tiers, 1u);
}
```

`priority = 89` is ALLOCATED — nothing in the workspace writes it. `tier_class =
"real_time"` is the AUTHORED head, untouched by the allocation, which is why
`placements` carries a `TierRtosSpec` rather than a whole `TierDef`. And `main`
is `run_tiers`, not `run_components`.

**The negative control, which is what makes step 3 a measurement rather than an
observation:** `derived_tiers_precedence::
without_the_allocation_an_unplaced_tier_cannot_resolve` feeds `resolve_tiers`
that same tier table and those same bindings with no allocation, and gets
`TierResolveError::MissingRtosSpec`. So before this fix this image was not built
with the wrong priority — `nros build` could not produce it at all.

**Step 4 — the image RUNS, and the derived tier is real OS threads.**

Built and run twice, because the first shape cannot show a priority and saying so
is part of the measurement.

*Run A — the rule-2 shape above (all four nodes bound to ONE `[tiers.ctrl]`).*
`timeout 6` on the linked `native_entry`: rc 124 (it kept running for the whole
window), and each component ticked at its contract rate —
`mrm_emergency_stop_operator` 181 and `stop_mode_operator` 181 (30.2 Hz),
`mrm_comfortable_stop_operator` 59 and `mrm_handler` 59 (9.8 Hz). But
`/proc/<pid>/task` held **one** thread, policy 0, rtprio 0, and stderr was
empty. That is not a defect and not the derivation: `nros_board_native_run_tiers_ns`
runs `tier_slice[0]` — the BOOT tier — on the calling thread and spawns a
`PlatformTask` only for `tier_slice[1..]`, so a one-tier table hands no priority
to the kernel at all. An author who binds every node to one tier gets one thread
by the board's own rule.

*Run B — the pristine W0 fixture, `system.toml` authoring NO tiers and no
`group_tiers`*, which is rule 4 and derives one tier per node. The emitted entry:

```c++
static const ::nros::board::NativeTierSpec __nros_tiers[4] = {
    { .name = "derived-mrm_emergency_stop_operator",   .priority = 89LL, … },
    { .name = "derived-stop_mode_operator",            .priority = 89LL, … },
    { .name = "derived-mrm_comfortable_stop_operator", .priority = 88LL, … },
    { .name = "derived-mrm_handler",                   .priority = 88LL, … }, };
… LinuxBoard::run_tiers(…, __nros_tiers, 4u);
```

30 Hz pair at 89, 10 Hz pair at 88 — rate-monotonic, and both inside the POSIX
application pool. Running it:

* **4 threads** in `/proc/<pid>/task` (the boot tier plus three spawned tier
  tasks). The derived table became OS threads, which is the layer the issue says
  a fix has to reach.
* The allocated number reached the **syscall**, which the kernel's own refusal
  proves: stderr carries `[warn] nros: SCHED_FIFO priority 89 was REFUSED
  (EPERM) — this process may not request real-time scheduling`. Nothing else in
  the image contains 89; it is the number `derive` allocated, carried through
  `install_placement` → the emitted `NativeTierSpec` → `PlatformTask::spawn_with`
  → `pthread_setschedparam`.
* All four components ticked: 175 / 176 / 59 / 59 in 6 s (≈29.2 Hz and ≈9.8 Hz).

### What stays unproven

* **The kernel HONOURING the derived priorities.** EPERM means all four threads
  ran `SCHED_OTHER`, rtprio 0 (measured), so the derived ORDER was not enforced
  here. Granting it needs `setcap cap_sys_nice+ep` on the binary — privilege this
  session does not take. What is proven is that the allocated number is requested
  of the kernel and the refusal is loud, which is the half the derivation owns;
  RFC-0079 already records POSIX as "half-solved" for exactly this reason.
* **Zephyr.** Every Zephyr number in this issue and in `derived_tiers_entry.rs`
  (30 Hz at 5, 10 Hz at 6, pool [5, 14]) comes from the bake and the entry
  emitter, not from a booted `native_sim` image. The fixture has an
  `[image.zephyr]` west application for it; building and running that is not done
  here.
* **Timing.** Tick RATES were measured; no latency or jitter was, and the ranks
  came out of the contract rather than out of a WCET (RFC-0078: none has ever
  flowed).

### Also folded, because the class is "one fact, two readers"

* `install_placement` and `groups_at_default_tier` are one spelling each in
  `nros-orchestration-ir`. The latter had two copies, the second added one wave
  after the first.
* `derive_entry_tiers` built its OWN callback-group map, ignoring `group_tiers`,
  while `resolve_plan_sched` built one that honoured it — so a
  `group_tiers`-bound group read as unbound to the derivation and as bound to
  the resolver. It now receives the resolver's map.
* the `derived-tiers-cpp` fixture harness moved to
  `tests/common/derived_tiers.rs` (two copies; this issue's gate would have
  been the third).

Sweep: `git grep -n 'execution.tiers.is_empty' -- '*.rs'` returns only comments
and one fixture assertion.
