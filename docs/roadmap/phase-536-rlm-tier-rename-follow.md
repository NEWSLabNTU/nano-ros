# Phase 536 -- Follow rlm's tier rename

**Status (2026-11-05). IN PROGRESS -- W1 done (#3127); W3 and W4's survey
done in their pull request; W0, W2 and W4's docs sweep not started.
Raised by the play_launch side, as phase 534 was, so nano-ros knows what is
moving, what it owns, and where it is on the critical path.** Decided (user,
2026-11-05): `tier` is renamed, in its own phase on both sides. **The word
(play_launch phase 99 T0, decided by the user 2026-11-05): `task`** -- below,
`<new>` reads `task`: `[tasks.<name>]`, `group_tasks`, `TaskDef`,
`execution.tasks`. rlm's release R is `v0.3.10`; the play_launch release
carrying it is 0.27.0.

The design of record is rlm's
[`docs/design/standard-names.md`](https://github.com/NEWSLabNTU/ros-launch-manifest/blob/v0.3.9/docs/design/standard-names.md)
M7 (rlm design issue 57); the plan for rlm and play_launch, with the
candidate terms and the reasons, is play_launch's
[phase 99](https://github.com/NEWSLabNTU/play_launch/blob/main/docs/roadmap/phase-99-tier-rename.md).
nano-ros's own names (W2) are recorded in an RFC, not here: RFC-0052 (the
execution mapper from the SystemModel to RTOS primitives) gains a "Names" section, as RFC-0074
did in phase 534; RFC-0047 and RFC-0108 get a status line.

Depends on: play_launch phase 99 T0 (the word) and T1 (rlm release R).
**Blocks:** rlm's R+1 (the release that refuses `[tiers.*]` and moves the
model to schema 5) ships only after W1 here is merged.

---

## What moves in rlm

`<new>` is the word phase 99 T0 picks (recommended there: `task`; the
alternatives are `scheduling context` and `executor`). With `task`, the
right-hand column reads `[tasks.<name>]`, `TaskDef`, `execution.tasks`.

| Today | After | When |
|---|---|---|
| `system.toml` `[tiers.<name>]`, `[tiers.<name>.<target>]` | `[<new>s.<name>]`, `[<new>s.<name>.<target>]` | R: old read with a `deprecated-spelling` warning; R+1: refused naming the new key |
| `[[component]] group_tiers = { <group> = <tier> }` | `group_<new>s` | same |
| `sched::TierDef`, `TierPlatformSpec`, `ResolvedTier`, `ResolvedTierTable` (field `tiers`), `DEFAULT_TIER`, `is_single_tier`, `SchedError::UnknownTier`, `NodeMatchedByMultipleTiers`, `AssignRule.tier` | `<New>Def`, `<New>PlatformSpec`, `Resolved<New>`, `Resolved<New>Table` (field `<new>s`), `DEFAULT_<NEW>`, ... | R: renamed, old names kept as `#[deprecated]` aliases; R+1: aliases gone |
| model `execution.tiers` | `execution.<new>s` (schema 5) | R+1; `SystemModel::from_yaml_str` rewrites schema 4 and older for good |

Unchanged: `execution.bindings` (its values are names, and a name is the
user's: `mrm`, `derived-<node>`), `DEFAULT_<NEW>`'s value `"default"`,
`ReservedBand` and `above = "transport"`. No contract key and no rule id
carries the word.

## What nano-ros uses today

Surveyed on `main` at `d1b1b21823` (2026-11-05): every token containing
`tier` in tracked files, `frontier` excluded, **28,611**. A line classifier
(keyword heuristic; the split is approximate, the total exact) puts
**22,875** in the scheduling sense, **5,656** in the CI/test-tier sense and
**80** in TIER IV's `tier4_*` packages.

Scheduling sense, by file kind:

| Kind | Tokens | What |
|---|---|---|
| Rust | 6,324 | see "Model reads" and "Own Rust API" |
| C / C++ | 2,263 | the C ABI, board runners, shims |
| generated / templates | 1,291 | 40 golden files, 17 templates, `__nros_tier_*` symbols |
| scripts | 1,230 | `check-tier-priority-plan-image.py`, `check-tier-spin-gap.py`, `nros-mem-report.py` |
| tests | 1,959 | `realtime_tiers_e2e`, `system_toml_tiers_parse`, fixtures |
| CMake / Kconfig | 234 | three Kconfig symbols, internal cmake variables |
| TOML | 593 | 10 `system.toml` files authoring tiers, `examples/fixtures.toml` |
| docs and book | 8,898 | about half in `archived/` |

All senses, docs by place: `docs/issues` 6,585 (of which `archived/`
6,281), `docs/roadmap` 3,514 (`archived/` 3,334), `docs/design` 1,379,
`docs/reference` 450, `book/` 145, `docs/development` 129, `docs/research`
91, `docs/guides` 69.

### Model reads (follow rlm)

- rlm `sched` types used in **24 files** (431 tokens): `nros-cli-core` (12
  files, incl. `codegen/entry/lower.rs`, `codegen/entry/mod.rs`,
  `entity_inventory.rs`, `orchestration/model_ingest.rs`),
  `nros-orchestration-ir` (6, incl. `derive.rs`, `lib.rs`),
  `nros-entry-lower`, `nros-macros` (`main_macro.rs`), and four test files
  (`derived_tiers_entry.rs`, `derived_tiers_precedence.rs`,
  `on_violation_lowering.rs`, `system_toml_tiers_parse.rs`).
- Field reads of `.tiers` / `execution.tiers`: 270 lines, most in
  `nros-orchestration-ir` `derive.rs` (35) and `lib.rs` (21),
  `codegen/entry/lower.rs` (18), `codegen/entry/mod.rs` (16),
  `nros-board-freertos` `entry.rs` (15).
- Non-Rust readers: `scripts/check-tier-priority-plan-image.py` reads a
  `"tiers"` list, but from nano-ros's own priority-plan record, not from a
  model. W3 re-audits CMake and Python for a direct model read.
- **Authored:** 10 of 262 `system.toml` files write `[tiers.*]` or
  `group_tiers` (102 table/key lines in all TOML): the `realtime-rust`,
  `realtime-cpp`, `realtime-cpp-subnode-portable`, `realtime-c` (two),
  `levels-cpp` and `derived-tiers-cpp` workspaces and the
  `orchestration_tiers_native`/`_freertos` fixtures.

### nano-ros's own names (nano-ros's call)

- **C ABI** (public headers): **64 distinct identifiers, 698 tokens** --
  `nros_tier_spec_t`, `nros_native_tier_spec_t`, `nros_tier_task_memory_t`,
  `nros_tier_backing_t`, `nros_tier_setup_fn_t`, `n_tiers`,
  `nros_board_{native,freertos,zephyr}_run_tiers*`, `nros_cpp_init_tier`,
  `NROS_TIER_TASK_STACK_*`, `NROS_TIER_TASK_MEMORY_*`, the three
  `rclc/detail/tier_task_memory_{zephyr,freertos,nuttx}.h` headers, and
  `NROS_CPP_TIER_BACKING_SIZE_*`. Example code calls almost none of them
  (`run_tiers` 3, `zephyr_apply_tier_deadline` 1); the generated entry and
  the board runners are the callers.
- **Rust API**: 82 `pub` items carrying the word (`TierSpec`,
  `NativeTierSpec`, `TierExecutorBacking`, `TierTaskMemory`,
  `RealtimeTiers`, `BootTier`, `TierResolveError`, `resolve_tiers`,
  `derive_tiers_from_contracts`, `apply_tier_sched_policy`,
  `from_tier_policy`, ...), 551 distinct identifiers in all.
- **Kconfig**: `NROS_ZEPHYR_MAX_TIERS`, `NROS_ZEPHYR_TIER_STACK_SIZE`,
  `NROS_ZEPHYR_TIER_STACKS_IN_DTCM`, hand-written in `zephyr/Kconfig`
  (not in the knob registry `knobs.toml`, which mentions tiers only in
  prose). A user's `prj.conf` may set them.
- **CMake**: internal variables only (`_nra_tier_bss_kb`, `_tier_arg`,
  `_nsw_TIER_RECORD`, `_NROS_FREERTOS_TIER_STACKS_IN_BSS_KB`); no public
  function or keyword argument names a tier.
- **Generated code**: `__nros_tier_executor_storage`, `__nros_tiers`,
  `__nros_entry_setup_tier_<k>[_nodes]`, `__nros_tier_<k>_groups`.
- **Names on disk**: example workspace `derived-tiers-cpp`, fixtures
  `orchestration_tiers_*`, test `realtime_tiers_e2e`, PX4's
  `tier_work_item.hpp` (`TierWorkItem`), the `just check tier-*` gates.
- **If the word is `task`**: RFC-0108's per-callback "task record" (72
  mentions) names a callback's (C, T, D), and would read as the renamed
  unit; it becomes "callback record". "tier task" (614 mentions) becomes
  "task".

### Other senses (stay)

- CI and test tiers (RFC-0061): `CiTier`, `Tier1`/`Tier2`/`Tier2N`/`Tier3`,
  `just ci tier1`, `lane=tier1|tier2|tier2-nightly`,
  `check-tier-has-ci-owner.py`, `check-tier-preconditions.sh`,
  `BucketTier`/`BUCKET_TIERS` (`nros-tests` `buckets.rs`), the coverage
  `Tier` (`nros-tests` `matrix.rs`).
- `CommandTier` (RFC-0117, where a CLI verb sits).
- Rust target tiers ("Tier 3 target, needs nightly") in toolchain files.
- TIER IV's `tier4_*` package names.

## Work items

### W0 -- The mapping, recorded once

When phase 99 T0 fixes the word, write the full old-to-new table for
nano-ros's own names (the C ABI, the Rust API, Kconfig, generated symbols,
names on disk) into RFC-0052 "Names", and the "task record" rename if the
word is `task`.

Acceptance: RFC-0052 has the table; RFC-0047 and RFC-0108 carry a status
line naming the rename; this phase's W2 links it.

### W1 -- Bump to rlm R; migrate the authored files

**Progress (2026-11-05): done in its pull request.** rlm `v0.3.10`,
`packages/cli/third-party/play_launch` at 0.27.0 (`b5243446`, a fast-forward
of the old pin). `play_launch migrate --write` rewrote the nine bringups that
author keys (85 edits; `levels-cpp`'s writes none, only comments name them),
and comments, the book, example READMEs and the RFCs that show a
`system.toml` (0015, 0027, 0047, 0066, 0079, with a status line) write the
new keys. nano-ros's own strict reader (`cargo_metadata_schema::SystemToml`)
reads `tasks` / `group_tasks` with the old keys as serde aliases; its Rust
fields keep their names until W2. `scripts/lib/priority_plan.py` reads a pin
under either key. Not covered by the aliases: rlm renamed
`ResolvedTaskTable`'s field `tiers` to `tasks` (a field cannot be aliased),
so `rtos_realizer.rs`'s two literals and its test reads move here, and the
rlm-qualified type names nano-ros names (`TierDef`, `TierPlatformSpec`,
`ResolvedTier`, `ResolvedTierTable`) take the new names so no build carries a
deprecation warning.

- `just bump-manifest <R>` and `packages/cli/third-party/play_launch` to the
  play_launch release carrying R, as phase 534 W1 did.
- Expected compile impact: none through the `#[deprecated]` aliases; if a
  crate builds with `-D warnings`, the 24 files' type names move here
  instead of in W3 (a mechanical rename, no behaviour).
- `play_launch migrate` (or by hand) over the 10 `system.toml` files:
  `[<new>s.*]` and `group_<new>s`.
- Active RFC and book pages that show a `system.toml` `[tiers.*]` example
  write the new key, with a status line.

Acceptance:

- nano-ros `main` pins R and its CI is green.
- A build of every bringup reports zero `deprecated-spelling` warnings
  (no shipped `system.toml` relies on the alias).
- Tell the play_launch side when it merges: rlm's R+1 waits on it.

### W2 -- nano-ros's own names follow

One vocabulary, as phase 534 W3 decided for `guards`/`ingress`: the own
names move with rlm's, per W0's table.

- **Rust API**: rename. Items re-exported by a published crate keep a
  `#[deprecated]` alias for one nano-ros release.
- **C ABI**: rename the 64 identifiers and the three headers. The old names
  stay for one nano-ros release as `typedef`s, `#define`s and forwarding
  headers that `#warning`, then retire into
  `book/src/reference/retired-names.md`. Struct layouts do not change
  (`check-ffi-struct-mirrors.sh` stays green with no layout edit). Breaking
  fragment in `changelog.d/`.
- **Kconfig**: new symbols in `zephyr/Kconfig`; the old ones kept one
  release as deprecated symbols feeding the new ones' defaults (with a
  CMake warning when a `prj.conf` sets one), then a configure error naming
  the new symbol, listed in `retired-names.md`.
- **CMake internals, generated symbols, templates and goldens**: rename;
  regenerate the 40 golden files.
- **Names on disk** (`derived-tiers-cpp`, `orchestration_tiers_*`,
  `realtime_tiers_e2e`, `tier_work_item.hpp`, the `tier-*` gates): rename,
  with `examples/fixtures.toml` and the CI lanes following.
- "task record" to "callback record" if the word is `task`.

Acceptance:

- The rename is mechanical: applying W0's table as a text substitution to
  the old goldens reproduces the regenerated ones byte for byte.
- The island board build's image (sizes from `nros-mem-report.py`) is
  unchanged.
- C and C++ code written against the old names still builds for one
  release, with a deprecation warning naming the new one (a compile test
  per header).

### W3 -- Model schema 5 together with rlm R+1

- Bump to rlm R+1 (schema 5, aliases gone): the 24 files read the new type
  names and `execution.<new>s`.
- Re-audit non-Rust model readers (CMake, Python) for `tiers`.

Acceptance:

- nano-ros builds and passes CI against R+1.
- A schema-4 model and its schema-5 twin produce the same build artifacts
  (the rendered entry tables and the sizing descriptor), with a negative
  control, as phase 534's
  `a_schema_3_model_lowers_to_the_same_tables_as_schema_4`.

**Progress (2026-11-05): done in its pull request.** rlm `v0.3.11`
(`dd3763f`, R+1: `[tiers.*]` / `group_tiers` refused, model schema 5) and
`packages/cli/third-party/play_launch` at 0.28.0 (`dd660b2d`, a
fast-forward of `b5243446`). The model's task table is `execution.tasks`:
the field reads in `nros-orchestration-ir` (`derive.rs`, `ftti_budget.rs`,
`violation_agreement.rs`), `nros-macros` (`main_macro.rs`), `nros-cli-core`
(`codegen_system.rs`, `codegen/entry/mod.rs`, `entity_inventory.rs`,
`model_ingest.rs`, tests) and `nros-launch-resolve` moved. The strict reader
(`cargo_metadata_schema::SystemToml`) drops W1's serde aliases and refuses
the old keys as rlm does, by name: `[tiers]` is a `RETIRED_SYSTEM_TABLES`
row (so the overlay, leaf and `nros::main!` readers refuse it too), a
component's `group_tiers` is `RETIRED_GROUP_TIERS`; each message names the
new key and `play_launch migrate`
(`the_retired_tiers_keys_are_refused_naming_tasks_and_migrate`,
`a_component_group_tiers_is_refused_naming_group_tasks`). Non-Rust readers:
`cmd/ws.rs`'s committed-model dim guard reads `execution.tasks` (falling
back to a pre-schema-5 model's `execution.tiers`); no CMake or Python tool
reads the model's task table (`scripts/lib/priority_plan.py` reads a
`system.toml`, now `[tasks.*]` only). Acceptance test:
`a_schema_4_model_and_its_schema_5_twin_emit_the_same_entry`
(`nros-cli-core/tests/derived_tiers_precedence.rs`): the resolved task table
and the emitted typed C++ entry (its task table, priorities and executor
backing sizes) are identical for a model and its schema-4 twin, and the
negative control moves the authored priority and sees the entry move.

### W4 -- Nothing authored needs the refusal; docs say the new word

- Re-run W1's survey just before the R+1 bump: no `system.toml` writes
  `[tiers.*]` or `group_tiers`.
- Active docs and the book use the new word for the scheduling sense;
  history (`archived/`, closed issues) keeps the words of its time.

Acceptance: `git grep -i tier` outside `archived/` returns only the other
senses listed above and the retired-names tables.

**rlm R+2 (2026-11-05):** rlm `v0.3.12` and play_launch 0.29.0 retire
`tier` from their own user-facing text (`check-terms`); nano-ros bumps to
them with no change in what it reads. This phase stays open for W0, W2
and W4's docs sweep.

**Survey (2026-11-05, W3 branch):** no tracked `system.toml` writes
`[tiers.*]` or `group_tiers`; comments, examples, the FreeRTOS config
template, `zephyr/Kconfig` and the RFCs that spelled a key `[tiers.*]` now
write `[tasks.*]`. The docs sweep waits for W2's names.

## Order

W0 when the word is fixed; W1 after rlm R is tagged and the play_launch
release carrying it is on PyPI (rlm R+1 waits on W1); W2 any time after
W0, ideally in the same nano-ros release as W1 so users see one rename; W3
with rlm R+1; W4 just before W3.
