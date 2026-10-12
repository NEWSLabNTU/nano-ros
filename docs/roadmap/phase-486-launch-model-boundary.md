# Phase 486 — the launch model carries what every realizer reads; nano-ros's own facts move to an overlay

**Status (2026-10-13). W0–W4, W7 and W8a–W8c LANDED (W4 #2034, W7 #2038, W8a #2067, W8b rlm `v0.1.57`, W8c this pin bump); W5 landed in play_launch (`ca7eb1a`, its phase 86 W1–W4) and W6 in rlm (`v0.1.55`/`v0.1.56`). W9 in review (#2076). The 0.6.0 deletion of `[deploy.*]`/`[param_services]` is ready and HELD for the version bump (issue 1809, draft #2081). Design revised 2026-10-12 and 2026-10-13 (below).** Implements RFC-0060's
[2026-10-10 amendment](../design/0060-launch-toolchain-three-layers.md#amendment-2026-10-10--what-the-systemmodel-may-carry-and-where-nano-ross-own-facts-go)
and RFC-0100's
[2026-10-10 ruling](../design/0100-rmw-agnostic-sizing-model.md#ruling-2026-10-10--a-stated-fact-earns-a-file-whatever-it-came-from).
Spans three repositories, which move in a fixed order (below):
`ros-launch-manifest` (rlm, `docs/model-boundary.md`), `play_launch`
(phase 86) and nano-ros (this doc).

Related: issue 1706 and PR #1830 (the parameter-store exception this retires),
issue 1766 / PR #1843 (Form-1 `nros::main!` reads a leaf's declared switches
directly — one of the readers W3 moves), RFC-0065 D6 (`[image.*]`), RFC-0072
§5 (`[board_config.*]`), issue 0293 (two parsers for one file).

## The problem

The SystemModel is shared by two realizers: play_launch on Linux and nano-ros's
embedded build. rlm's `SystemConfigToml` parses nano-ros's `system.toml` and
projects three nano-ros build facts into it:

| model field | from | meaning outside nano-ros |
| --- | --- | --- |
| `execution.features` | `[system] features`, `[param_services]` | none. rclcpp nodes have parameter services unless their code opts out |
| each lifecycle node's `lifecycle_autostart`, set system-wide | `[lifecycle] autostart` | the per-node field is ROS (Jazzy `LifecycleNode(autostart=)`); the system-wide default is not |
| `execution.deploy.<n>.extra` and `.target` (revised 2026-10-12: `target` leaves too) | `[deploy.<n>]` board/framework/profile/optimize/features/kind, `[deploy.<n>.nros]`; `extra.host_name` from `[host.<n>]` | none. These describe a build; nano-ros itself moved them to `[image.*]` and `[board_config.*]`. No realizer reads `target` (see the 2026-10-12 revision) |

Measured consequences:

- six nano-ros readers take `execution.features` from the model
  (`cmd/entity_facts.rs`, `codegen/entry/mod.rs`, `entity_inventory.rs`
  `InfraServices::from_model`, `orchestration/model_ingest.rs`,
  `nros-orchestration-ir/src/leaf_system.rs`, `nros-macros/src/main_macro.rs`);
- because the switch reached the sizing descriptor as a model fact, PR #1830
  needed an exception to RFC-0100's "a model-only fact earns no file";
- the switch is declared once per SYSTEM while its cost is per IMAGE
  (`examples/workspaces/features` comments that its switches are native-only);
- play_launch's parser does not handle `<lifecycle_node>` at all, so a launch
  file cannot be the source of the per-node lifecycle fact that IS portable.

## Revision 2026-10-12 — what re-reading rlm and play_launch `main` changed

rlm moved `v0.1.40` → `v0.1.54` and play_launch gained 7 commits past our pin
(`d23720c7`) since this phase was written. Both repositories adopted the
design as docs (rlm `docs/model-boundary.md`, play_launch phase 86) and
implemented none of it. Re-checking the plan's claims against their code
found five things wrong or missing; the waves below are corrected in place.

1. **The lifecycle default is projected by play_launch, not rlm.** The loop
   that copies `[lifecycle] autostart` onto every lifecycle node is in
   play_launch's resolver (`src/ros-launch-resolve/resolve/src/model.rs`),
   through rlm's `lifecycle_autostart()` accessor; `apply_to_launch` writes
   the execution layer only. The deletion moves from W6 (rlm) to W5
   (play_launch phase 86, new W0 there). rlm deletes the accessor at W8.
2. **Correction 1 below rested on a false premise.** play_launch reads
   nothing from `execution.deploy` — not `target`, not `Mcu`, not `extra`
   (`git grep` on its `main`). `Mcu`'s only writer is `[deploy.<n>] board`,
   used by 0 of 218 tracked `system.toml`. So `Deploy.target` leaves with the
   rest of `[deploy.*]`, and nano-ros's two board-slice readers (issue 0358's
   duplicate) are DELETED at W3, not moved: they act on a table nobody writes.
3. **`Deploy.extra` also carries placement keys.** `[host.*]` (23 tracked
   files) writes `extra.host_name`, `[deploy.*]` writes `deploy_name`. Nothing
   reads either. They leave with `extra`; the typed placement facts `domain`,
   `locator` and `rmw` stay.
4. **The model is schema 2 now.** rlm `v0.1.51` made a list parameter a list
   (`ParamValue::StrList`) and bumped `SCHEMA_VERSION` to 2, upgrading a v1
   model on read. Today nano-ros reads with `v0.1.40` (schema 1) and our
   play_launch pin's resolver writes with `v0.1.49` (schema 1). **Moving the
   play_launch pin past `476f8a86` without bumping nano-ros's rlm tag to
   `v0.1.51`+ in the SAME commit makes every resolved model unreadable**
   (`unsupported SystemModel schema version 2`). nano-ros reads parameter
   values only through rlm's `resolved_params`, which renders lists itself,
   so the bump should be mechanical. Folded into W7.
5. **rlm names its realizers.** `model/src/system_config.rs` mentions
   nano-ros 49 times and `model/src/lib.rs` 20, and the crate holds a lax
   second parser of nano-ros's file (issue 0293). Ruled the same day (rlm
   `model-boundary.md`, RFC-0060's amendment): `SystemConfigToml` becomes
   rlm's own format for the shared layers only, every other table is ignored
   by design, and rlm's doc comments name no realizer. nano-ros parses each
   key once — shared keys through rlm's struct, its own through `SystemToml`
   — and stays strict on the union by refusing what both ignored
   (`serde_ignored`; `#[serde(flatten)]` cannot combine with
   `deny_unknown_fields`). New W8 rows.

## Revision 2026-10-13 — W8 against a generic rlm

W8 was written while rlm still owned part of nano-ros's file. Since `v0.1.56`
rlm is a generic system-config reader that names no realizer and ignores
every table it does not declare. Re-reading its `main` and nano-ros's against
that rule changes three things. The W8 table below is corrected in place.

1. **rlm still carries nano-ros vocabulary, and W8 removes all of it.**
   - Two realizer-owned keys:
     - `SystemConfigToml.lifecycle`, `.param_services`;
     - `SystemDefaults.features`, and the `lifecycle_autostart()` accessor.
   - The whole `[deploy.*]` table:
     - `DeployBlock`;
     - the build fields it refuses unknown keys for;
     - the placement path in `apply_to_launch`, whose `kind = "embedded"`
       filter is nano-ros's board-build concept;
     - the `[deploy.<n>.nros]` sub-table.
   - The read-tolerant model fields: `Execution.features`, `Deploy.extra`,
     `Deploy.target` and `Target`. They are deleted in the same release:
     both known consumers already pin `v0.1.56`, and no tracked model
     carries them.

   Placement in rlm is then `[host.*]` alone. A `[deploy.*]` table is
   ignored like any other table rlm does not own. play_launch reads none of
   these (`git grep` on its `main`: no `Target::`, `.extra`,
   `execution.features`, `DeployBlock` or `lifecycle_autostart()`), so its
   part is a tag bump.
2. **nano-ros owns `[deploy.*]` until 0.6.0** (issue 1809, which keeps the
   removal at the minor-version boundary phase-383 promised).
   - The type moves into nano-ros as its own deprecated struct. It stops being
     a re-export of rlm's `DeployBlock`, which no longer exists.
   - Build fields keep parsing with the deprecation warning, as today.
   - **Placement: decided 2026-10-13, option (a).** rlm stops reading it. Today's warning
     tells users "`[deploy.*]` keeps PLACEMENT (kind / nodes / launch) — that
     half is upstream's". Two options:
     - **(a) Refuse a placement-bearing block by name**, pointing at
       `[host.*]`. A block is placement-bearing if it has `nodes`, `launch`,
       or is one of several non-embedded blocks. No tracked file has one
       (`git grep '^\[deploy' -- '*.toml'` is empty). The cost: the promise
       in the warning breaks for an out-of-tree 0.5.0 user who partitions with
       `[deploy.*]`, a case the 0.6.0 deferral was meant to protect.
     - **(b) Translate it.** `nros sync` hands the resolver a derived
       `system.toml` in which deprecated `[deploy.*]` placement is rewritten
       as `[host.*]`. The promise holds, at the cost of translation code for a
       path nothing in-tree exercises.

     Chosen: (a). The promise is about build fields reaching 0.6.0;
     placement has had a non-deprecated home (`[host.*]`) since issue 0951. A
     refusal that names that home loses nothing silently.
   - `[param_services]` follows the build-field rule. nano-ros's own
     `SystemParamServices` keeps parsing with its warning until 0.6.0. rlm's
     copy goes, since rlm only ever ignored it.
3. **The issue-0293 row assumed rlm's struct is nano-ros's parser.** It was
   written to make nano-ros parse the shared keys through
   `SystemConfigToml`, refusing what both readers ignored (`serde_ignored`).
   A generic, lax rlm reader is rlm's view of the file, not the schema of
   nano-ros's `system.toml`, so the row is replaced:
   - **One strict parse stays in nano-ros:** `SystemToml`,
     `deny_unknown_fields`, over nano-ros's whole file.
   - **Where a shared table's shape is identical, nano-ros uses rlm's type:**
     `[host.*]` is `HostBlock`. This is already true.
   - **Where nano-ros extends a shared table, it keeps its own type:**
     `[system]` (`SystemHeader`), `[[component]]` (`SystemComponentEntry`),
     `[[bridge]]`, and `[tiers.*]`. (The first draft of this item said
     `[tiers.*]` already used rlm's shared `TierDef`. It does not: nano-ros
     has its own `TierDef` in `nros-orchestration-ir`, with `*_us` integers
     where rlm's carries durations.) An agreement test checks that each SHARED
     key in every tracked `system.toml` means the same to both readers: equal
     values through rlm's `parse_system_config` and through `SystemToml`. That
     is the drift issue 0293 hit (a key one reader dropped), caught as a red
     test instead of a silent difference.

   Measured against the old row: nano-ros reads the shared fields at about
   400 sites. Routing them through a second struct would have moved all of
   them to protect against a drift the agreement test catches directly.

## Ordering — why the waves run in this order

nano-ros pins rlm by tag, so nothing breaks until nano-ros bumps the tag. The
one ordering constraint that matters is: **nano-ros must stop reading the
fields (W3) before it bumps to an rlm that no longer writes them (W7).**
Everything before W7 is additive and must leave every image byte-identical.

```
W0 census ─► W1 overlay (additive) ─► W2 per-image switches ─► W3 readers move ─► W4 descriptor rule
                                                                        │
             play_launch phase 86 (W5: lifecycle_node autostart;
                                   drop the [lifecycle] projection) ────┤
             rlm (W6: stop writing features/extra/target; neutral
                  system config; tag) ──────────────────────────────────┤
                                                                        ▼
                                                     W7 pin bumps ─► W8 retirement ─► W9 carriers
```

## W0 — census, before anything moves

- Every reader of `execution.features`, `lifecycle_autostart`,
  `execution.deploy.*.target`/`extra` in nano-ros, play_launch and rlm. Six
  nano-ros readers of `execution.features` are known; confirm the count and
  find the rest.
- Every tracked `system.toml` key that feeds a moving field: `features` (3
  files), `[lifecycle]` (1), `[param_services]` (0), `[deploy.*]` (0) of 189
  as of 2026-10-10.
- Every play_launch fixture, test or doc that writes those keys or reads those
  fields. The legacy `system.toml` bridge (`--sched foo.toml`) is the only
  route, so expect almost none.
- The images W1–W4 must leave byte-identical: the 7 sizing-descriptor
  consumers phase-457 diffed, plus the param-store images from PRs #1823,
  #1830 and #1846.

**Acceptance:** a census table committed in this doc. Any reader W0 finds
outside the list becomes a W3 row.

### W0 result (2026-10-11)

**Readers: three code sites and one funnel, not six.** Every read of a
moving field in `packages/` (tests excluded), by `git grep`:

| Field | Reader | What it does |
| --- | --- | --- |
| `execution.features` | `entity_inventory.rs` `InfraServices::from_model` | the FUNNEL. It is the only model reader feeding `cmd/entity_facts.rs`, `cmd/sizing_descriptor.rs` (×2), `cmd/build.rs` (the store infra) and `orchestration/model_ingest.rs`. The leaf roads already use `InfraServices::from_features` over the leaf's own `system.toml`, so they are overlay-shaped today |
| `execution.features` | `codegen/entry/mod.rs` (`param_services`, `safety` for the entry `Plan`) | direct read |
| `execution.features` | `nros-macros/src/main_macro.rs` (launch-arm axis asserts, PR #1872) | direct read |
| `lifecycle_autostart` | `codegen/entry/mod.rs`, `entity_inventory.rs` (`InfraServices`), `main_macro.rs` | all three REDUCE per-node to one image value: `find_map` / `any` over `structure.nodes`, first node wins |
| `Deploy.target = Mcu{board}`, `Deploy.extra["kind"]` | `codegen/entry/mod.rs` and `main_macro.rs` (the same board-slice rule, twice: issue 0358's class) | which nodes an entry keeps for its board |
| `Deploy.extra` (any other key) | none. `nros-orchestration-ir/src/derive.rs` records that the `edf`/`cores` readers were removed (issue 0951) | — |

**Inputs (189 tracked `system.toml`):** `[system] features` in 3 files
(`examples/workspaces/{features,managed,safety}`), `[lifecycle]` in 1
(`features`), `[param_services]` in 0, `[deploy.*]` in 0. No tracked model
(they are build artifacts).

**Byte-identical set for W1–W4:**
- every `[image.*]` of the three bringups above: `features` (lifecycle, params,
  custom-msg images), `managed` (`native_managed`) and `safety` (four
  native C/C++ images);
- the store images from PRs #1823, #1830 and #1846: the AN536 FreeRTOS Cyclone
  C++ entry, `param-store-nuttx-qemu-arm` and `param-store-threadx-riscv64`;
- one bringup with no switches, as the negative control:
  `examples/workspaces/cpp`.

**Correction 1 (SUPERSEDED 2026-10-12, see the revision above — play_launch
does not read `Deploy.target`, and `Mcu` has no tracked producer; `target`
leaves and both slice readers are deleted): `Target::Mcu{board}` stays in the model.** The board-slice
readers use it as PLACEMENT ("this node runs on that board"), and placement
passes the RFC-0060 test: play_launch reads `Mcu` as "not a node this machine
runs", which is the same meaning. What retires is `Deploy.extra`, including
its `kind` fallback in the slice. No authoring path writes `[deploy.*]` (0 of
189), so that fallback is reachable only from the deprecated table. W6 and W8
are narrowed to `extra`. The RFC-0060 amendment's table is corrected to match.

**Correction 2: the lifecycle reduction needs a rule.** The runtime has ONE
executor-wide autostart (`nros_cpp_lifecycle_autostart(executor, level)`), and
all three readers collapse per-node values with first-node-wins. Once W5 lets
launch files state `autostart` per node, two lifecycle nodes in one image can
disagree. W3 replaces first-wins with one shared reducer:
- every node that states a value agrees, and that value wins;
- if none states one, the overlay default applies;
- if two disagree, sync refuses and names both nodes.

The reducer replaces the three copies.

## W1 — the overlay, written but not read (additive)

- `nros sync` writes `<ws>/build/nros/models/<bringup>/nros.toml` beside the
  model, from nano-ros's own strict `SystemToml`.
- A locator in `nros_orchestration_ir` next to `model_location`; no consumer
  derives the path.
- Schema v1: `[meta] version`; `[image.<id>] features` (resolved:
  image → `[image_defaults]` → `[system]`); `[lifecycle] autostart`.
  `deny_unknown_fields` on read.
- A single-package leaf gets one too. Its `system.toml` is already nano-ros's;
  the overlay is just the resolved copy.
- No overlay content ⇒ no file, the same rule as the descriptor.

**Acceptance:** for every tracked bringup, the overlay's switches equal the
model's `execution.features` and its lifecycle default equals what the model
projected. A test asserts this agreement (it is W3's safety net). No image
changes.

### W1 result (2026-10-11)

- `nros_orchestration_ir::overlay`: `Overlay` (features, lifecycle default),
  `overlay_path_for_model` (beside the model, one per directory) and
  `write_beside` (write-if-changed; empty ⇒ a stale file is deleted).
- **Both model producers write it:** `nros sync` (`cmd/ws.rs`, on the resolve
  path AND on the "model is current" path, so a tree synced before W1 gains
  one without a re-resolve) and `model_location::ensure_model` (the
  proc-macro, `nros-build` and the CLI's own resolves), on its resolve and
  fresh paths.
- **Agreement:** `overlay_agrees_with_model_projection` (renamed at W7, see below) compared the overlay
  with rlm's own `parse_system_config` + `apply_to_launch` +
  `lifecycle_autostart` for every tracked `system.toml`. It passes, with 3 of
  them stating something. Mutation: dropping the lifecycle read turns it red.
- **Byte-identical by construction:** nothing reads the overlay yet, and no
  build input changes (`system.toml` was already registered by every
  producer).

## W2 — per-image switches

- `[image.<id>] features` and `[image_defaults] features` in `SystemToml`.
  `[system] features` stays as the default.
- Unknown switch names are refused by name, at sync, naming the image.
- `examples/workspaces/features` states its switches on the native image only,
  as its own comment asks.

**Acceptance:** a two-image bringup with `param_services` on one image builds
the store in that image only (`IMPLIED_STORE_SLOTS` 32 vs 0), measured. Every
existing bringup is byte-identical, since none states per-image switches yet.

### W2 result (2026-10-11)

- **Found:** `[image.<id>] features` already parsed in `ImageBlock`, documented
  as "capability axes for this image, over the system's own list", and
  `ImageBlock::merged_over` already concatenated `[image_defaults]` into it.
  But **nothing read it**, so an image that stated a switch was silently built
  without it. W2 keeps the documented ADDITIVE meaning rather than the
  override the design sketched. Additive matches `merged_over`, and turning a
  system-wide switch OFF for one image is not needed by any tracked bringup:
  the `features` workspace's switches are native-only, and all of its images
  are native.
- The overlay carries `[image.<id>] features` (only the images that add
  something), and `Overlay::features_for_image(id)` returns the system list
  plus the image's additions.
- An unknown per-image switch is refused when the overlay is derived, so at
  `nros sync` and in `ensure_model`, naming the image and the known axes. The
  system-level list keeps rlm's lax reading until W8.
- The `IMPLIED_STORE_SLOTS` 32-vs-0 measurement moves to W3, the first wave
  where a reader asks per image.
- Tests: 3 new unit tests. Mutation: disabling the axis check turns the
  refusal test red.

## W3 — every reader moves to the overlay

- The six readers, and any W0 adds, read the overlay. The model's
  `execution.features` is read by nothing in nano-ros.
- The lifecycle default is applied by nano-ros: for each lifecycle node whose
  model `lifecycle_autostart` is `None`, the overlay default applies. A
  per-node value from the launch file or contract wins.
- `nros::main!` Form 1 (PR #1843's `leaf_capabilities`) reads the leaf's
  overlay through the same reader as every other road.
- **The two board-slice readers are deleted** (`codegen/entry/mod.rs`'s
  `board_mentioned`/`keep`, and its copy in `main_macro.rs`), with the
  `Deploy.extra["kind"]` fallback. Revision 2026-10-12: they read only
  `[deploy.*]`, which no tracked `system.toml` writes. An entry keeps every
  node its own launch selection names. The `deploy_facts` session rung
  (issue 0794: `domain`/`locator`/`rmw`) stays — it reads typed placement.
- **Gate:** `check-launch-model-boundary` — no nano-ros source outside a
  named allowlist reads `execution.features`, `Execution::features`, a
  `Deploy` `extra` key, `Deploy.target` / `Target::Mcu`, or projects a
  system-wide lifecycle default from the model. The allowlist is empty at W8.

**Acceptance:** every W0 image is byte-identical: knobs diffed per consumer,
plus the sizes headers and `OUT_DIR` outputs (the PR #1830 method). Mutation:
pointing one reader back at an empty model field turns a test red.

### W3 result (2026-10-12)

- **One loader.** `nros_orchestration_ir::overlay::LoadedModel` (model +
  overlay, `load` / `with_overlay_beside`) derefs to the model;
  `model_ingest::load_model` and the inventory and sizing loaders return it.
  A model that still carries switches with no overlay beside it is refused,
  naming `nros sync` — the migration rule, implemented.
- **Switches** come from `LoadedModel::features(image)`. `nros build`'s store
  seed asks per image (`[image.<id>] features`, W2); every other reader asks
  for the system list.
- **Lifecycle:** `reduce_lifecycle_autostart` replaces the three first-wins
  reads (`codegen::entry`, `nros::main!`, the inventory). Nodes that state a
  value agree and win; none ⇒ the overlay default; a disagreement is refused
  naming both nodes.
- **Board slice deleted** from both entry emitters. Placement is one shared
  rule, `overlay::placed_here`: a node absent from a NON-EMPTY
  `execution.deploy` was placed elsewhere. Found while deleting it: the old
  `keep` also carried this `[host.*]` partition (23 tracked files use
  `[host.*]`), which is placement and stays.
- **Gate** `check-launch-model-boundary` (fast line): no source names
  `execution.features`, `Deploy.extra`, `Deploy.target` or a first-wins
  lifecycle read. Two allowlisted sites: the migration guard, and a test
  proving issue 0951's EDF reader stays removed. Self-test first; a stale
  allowlist entry fails.
- **Byte-identical, measured** after `nros sync` of every tracked
  workspace, old CLI (main) vs new: `nros ws entity-facts` 84/84 launch
  files, `nros ws entity-inventory` 78/78 models, and `plan_from_model`
  390/390 (78 models × native/freertos/zephyr/nuttx/threadx). The
  `nros::main!` reader shares the reducer and `placed_here`, and its
  unit tests pass; a compiled Rust entry was not diffed.
- **Not yet:** the per-image `IMPLIED_STORE_SLOTS` 32-vs-0 measurement
  (W2 moved it here). The reader is per image now, but no tracked bringup
  states a per-image switch, so measuring it needs a two-image fixture; it
  moves to W4 with the descriptor rule.

## W4 — the sizing descriptor composes over both inputs

- `[params] store` is composed from the overlay (switch) plus the model and
  contract (seed, `params:`).
- The file rule becomes "no stated fact ⇒ no file" (RFC-0100 2026-10-10). The
  store-only branch from PR #1830 folds into the ordinary write.
- The 2026-10-09 ruling is marked superseded (already done in the RFC); the
  CLAUDE.md line PR #1830 added is replaced by a pointer to the 2026-10-10
  ruling.

**Acceptance:** the AN536 FreeRTOS Cyclone C++ entry (switch, no contract
`params:`) still carves 32 slots at a 640 KiB heap and delivers. A
parameter-less image is byte-identical. Stale-file deletion still works, so a
removed switch stops the carving on an incremental build.

### W4 result (2026-10-12)

- **One writer.** `ModelImage.inventory` became `wiring: Wiring`:
  `Composed(&EntityInventory)` or `Unwired { params, infra, roots }`.
  `write_for_model` / `write_for_runtime` return `Option<WrittenDescriptor>`:
  `None` when nothing is stated, and a file at the path is deleted whichever
  shape wrote it. `StoreOnly` / `write_store_only` are deleted; all six model-
  and leaf-road call sites (`nros build`, `nros ws sizing-descriptor
  --from-model`, its runtime form, `--from-leaf`) go through the two writers.
  An unwired file's bytes are unchanged: `[params]` alone, `[meta]` refused.
- **`[params] store` composes over both inputs** already since W3:
  `InfraServices::from_model(model, image)` reads the overlay's switches, and
  `ParamDeclarations::from_model` the model's seed and `params:`.
- **Byte-identical, measured:** main's CLI vs this one over every tracked
  bringup's resolved models and every tracked standalone cmake leaf, 242
  writes, each preceded by a stale file at the target path: 21 full files and
  28 parameter-store-only files (11 of them runtime files) identical, 168
  "nothing stated" cases where both removed the stale file, 13 where both left
  it (cargo leaves the leaf producer stands down for), 12 erroring the same
  way on both. 0 differences.
- **W2's acceptance, measured:** one bringup, two images, `param_services` on
  image `a` alone (`[image.a] features`). Image `a`'s descriptor states
  `store = "param_services"`; image `b` gets no file. Building `nros-params`
  against each: `IMPLIED_STORE_SLOTS = 32` for `a`, `0` for `b`
  (`a_per_image_switch_builds_the_store_in_that_image_only`).
- **Mutation:** disabling the stale removal turns three tests red, including
  the new `a_deleted_contract_with_no_store_removes_the_full_descriptor` (a
  contract's full file, then the contract deleted, must leave no file).
- **Not run:** the AN536 FreeRTOS Cyclone C++ entry on QEMU. Issue 1706's
  run of it used UNCOMMITTED local edits (`param_services` added, heap cut to
  640 KiB); the tracked `workspaces/cpp` bringup states `store = "none"`, and
  its 6 writes above are byte-identical. The wired-with-switch and
  unwired-with-switch shapes it exercised are among the identical writes
  (21 full, 28 store-only), and the cmake side reads only that file, so the
  carve follows from the bytes; the boot was not repeated.
- RFC-0100's 2026-10-10 ruling is marked in effect; CLAUDE.md's descriptor
  line now states "no stated fact ⇒ no file".

## W5 — play_launch: the per-node lifecycle fact from the launch file (play_launch phase 86)

- **Drop the system-wide projection** (phase 86 W0, added 2026-10-12): delete
  the `[lifecycle] autostart` loop in `resolve/src/model.rs`. It lands with
  the pin bump, after nano-ros W3 applies the default from its overlay.
- The XML and YAML frontends dispatch `<lifecycle_node>`, including Jazzy's
  `autostart` (bool → `Active`).
- The Python `LifecycleNode` mock captures `autostart`.
- Both parsers produce the same `lifecycle` / `lifecycle_autostart` (the
  parity gate).
- `up` honours `Active` by driving configure and activate, as stock
  `launch_ros` does on Jazzy.

**Acceptance:** play_launch's parity gate passes with a lifecycle fixture, a
differential test against Jazzy `launch_ros` agrees, and a Humble-only file is
unchanged.

## W6 — rlm stops projecting nano-ros facts (rlm `docs/model-boundary.md`)

- `apply_to_launch` no longer writes `execution.features`, `Deploy.extra`
  or `Deploy.target`. (It never projected `[lifecycle]`; that is W5.)
- `SystemConfigToml` is documented as rlm's own neutral format for the shared
  layers; its doc comments stop naming realizers (rlm `model-boundary.md`,
  2026-10-12).
- `SystemConfigToml` stays LAX: the keys still parse (into nothing), so a
  valid nano-ros `system.toml` still resolves. They are deleted from the
  struct only after W8, together with nano-ros's re-exports of them.
- The model types keep deserialising old models: `Execution.features`,
  `Deploy.extra` and `Deploy.target` become read-tolerant. They load and are ignored, the
  `jitter_ms` precedent (rlm phase 68), and the golden fixture keeps them on
  disk. No schema bump for this (rlm is at schema 2 for an unrelated reason).
- A new tag. Field census (`scripts/field_census.py` in play_launch) updated:
  the fields leave the model's consumer list.

**Acceptance:** rlm tests green; a model resolved from every play_launch
fixture is byte-identical except for the removed fields; a nano-ros
`system.toml` carrying every moved key resolves without error.

## W7 — pin bumps, in one direction

1. play_launch: `just bump-manifest <tag>` (three manifests, three locks),
   then W5's work lands on that tag. Fast-forward push to `main`.
2. nano-ros: bump the rlm tag in every manifest that names it (one tag across
   the tree: `git grep 'ros-launch-manifest.*tag' -- '*/Cargo.toml'`), move the
   `play_launch` submodule pin forward, `just setup-launch-resolve`, then
   `nros sync` every bringup. **Both moves in ONE commit**: the resolver past
   play_launch `476f8a86` writes SystemModel schema 2, which an rlm below
   `v0.1.51` refuses (revision item 4). The jump crosses `v0.1.40` →
   `v0.1.51`+ in one step, so W7 also absorbs that range's API changes
   (`ParamValue::StrList`, `ScopeInfo.ns`, `NodeInstance.on_exit`, the
   contract additions of `v0.1.46`–`v0.1.49`).

**Acceptance:** every W0 image is byte-identical to W4's. The models no longer
carry `features`, and nothing in nano-ros noticed. Tier 2 run
(`just build-test-fixtures lane=tier2` + `just ci matrix`), since codegen and
cmake inputs moved.

### W7 result (2026-10-12)

- **Step 1 was already done upstream:** play_launch `main` (`15258108`) pins
  rlm `v0.1.56` (its phase 86 W5, `af13c3b7`).
- **One commit moves both:** rlm `v0.1.40` → `v0.1.56` in the four manifests
  that name it and all twelve locks (four packages each, version and source
  only; the two NuttX FFI locks were edited to the same lines by hand, since
  their `libc` patch source lives in an uninitialised submodule), and the
  `play_launch` pin `d23720c7` → `15258108`, a fast-forward. Registry pin set:
  0 pins moved.
- **API absorbed:** one site. A contract's `Trigger::Timer` gained `jitter`
  (`v0.1.49`, play_launch phase85-D1); `entity_census` counts timers by
  `rate_hz` and now says `..`. No nano-ros reader takes a timer's release
  jitter yet; a contract stating one is accepted and the value is unused. `ParamValue::StrList`,
  `ScopeInfo.ns` and `NodeInstance.on_exit` broke nothing: every nano-ros
  literal already ends in `..Default::default()`.
- **W1's agreement test is repurposed**, since rlm no longer projects the
  switches it compared against: `overlay_is_the_only_home_of_the_switches`
  asserts that rlm's own `apply_to_launch` writes NO `execution.features` for
  any tracked `system.toml`, that the overlay still states the switches of
  the bringups that have them, and that the lifecycle default agrees with
  rlm's `lifecycle_autostart()` while it exists (W8 deletes it).
- **Byte-identical to W4's, measured:** W4's CLI with its resolver vs this
  CLI with the new one, each resolving its OWN models (91 per side, all
  paired; every new model is schema 2 and none carries `features`):
  `entity-facts` 126/126, `entity-inventory` 91/91, `sizing-descriptor
  --from-model` 91/91, typed C entries 89/89, typed C++ entries 89/89; the
  rest error identically on both sides (4 facts, 2+2 entries). The lifecycle
  autostart those entries emit now comes from the overlay alone: the new
  resolver projects `[lifecycle]` onto no node (play_launch phase 86 W0).

## W8 — retirement

Corrected 2026-10-13 (revision above). Three steps, in this order, because
nano-ros must stop naming an rlm item before the tag that deletes it reaches
it:

- **W8a, nano-ros (pins rlm `v0.1.56`):**
  - Own the deprecated `[deploy.*]` type.
  - Refuse a placement-bearing `[deploy.*]` block by name, naming `[host.*]`.
  - Stop calling rlm's `lifecycle_autostart()`.
  - Move the overlay migration guard off `Execution.features` and onto the
    raw YAML. The question it asks is "did an older writer put switches
    here", which survives the field's deletion.
  - Add the shared-key agreement test.
- **W8b, rlm:** delete the rows below and tag.
- **W8c, play_launch, then nano-ros:** bump to that tag. In nano-ros the rlm
  tag and the `play_launch` pin move in ONE commit, and the gate's allowlist
  goes to empty.

| What | Where | Replaced by | Refusal |
| --- | --- | --- | --- |
| `SystemConfigToml.lifecycle`, `.param_services`, `SystemDefaults.features`, `lifecycle_autostart()` | rlm | nano-ros `SystemToml` (its own `[lifecycle]`, `[param_services]`, `[system] features`) | none in rlm: it ignores tables it does not declare |
| `[deploy.*]`: `DeployBlock`, its placement path, `[deploy.<n>.nros]` | rlm | placement → `[host.*]` | none in rlm (ignored) |
| `[deploy.*]` re-export | nano-ros | nano-ros's own deprecated type until 0.6.0 (issue 1809) | placement-bearing block: refused by name, naming `[host.*]` (revision item 2); build fields: warned until 0.6.0 |
| `[param_services]` typed block | nano-ros | `features = ["param_services"]` | warned until 0.6.0 (issue 1809), refused after |
| `Execution.features`, `Deploy.extra`, `Deploy.target` / `Target` | rlm model | the nano-ros overlay; nothing for `target` | none: old models still load (serde ignores the keys); nano-ros's guard reads the raw YAML |
| the issue-0293 double parse | — | replaced by the shared-key agreement test (revision item 3) | unknown key refused by name, as today |
| the PR #1830 store-only special case | nano-ros sizing descriptor | the general rule (W4, landed) | — |
| the W3 gate's allowlist | nano-ros | empty | the gate itself |

**Acceptance:**
- `check-launch-model-boundary` passes with an empty allowlist.
- rlm's crate contains no realizer vocabulary.
- Every moved key in a fixture `system.toml` either resolves through the
  overlay or is refused or warned by name. Nothing is silently dropped.
- W0 images are byte-identical across W8c, measured like W7.

### W8a result (2026-10-13)

- **`[deploy.*]` is nano-ros's own deprecated type** (`DeployTarget` in
  `cargo_metadata_schema.rs`), no longer a re-export of rlm's `DeployBlock`.
  - The parse refuses, by name, every field that reached an image only
    through rlm's placement: `nodes`, `launch`, `domain_id`, `locator`,
    `rmw`. Each refusal names its new home.
  - **Why `domain_id`, `locator` and `rmw` are refused too:** placement is
    what put them into the model's `execution.deploy`, which
    `codegen entry` bakes into the session, so they would also have been
    dropped silently.
  - **Readers deleted with the fields:** the deploy rungs of
    `resolved_{rmw,domain_id,locator}`, `codegen-system`'s `[deploy.<t>]
    launch` fallback, and doctor's deploy launch check.
  - **Doctor reports a file it cannot parse** instead of skipping it. A
    refused `[deploy.*]` used to make doctor call the file healthy.
  - Build fields still parse with the warning until 0.6.0. That lint no
    longer tells users "`[deploy.*]` keeps PLACEMENT". Issue 1809 is
    narrowed to match.
- **The overlay's migration guard reads the raw YAML.**
  `legacy_projected_features` replaces `model.execution.features`, using
  `serde_yaml_ng`, which was already in all 11 locks: one line each.
  - `LoadedModel::from_legacy_projection(SystemModel)` became
    `from_legacy_yaml(&str)`. Its ~130 test call sites carried no switches
    and became `bare`.
  - The three helpers that write switches as model YAML now return a
    `LoadedModel`.
  - The trap: a test closure typed `&SystemModel` that wraps
    `bare(m.clone())` drops the overlay through deref coercion and still
    compiles. Four tests went red on exactly that, and each closure was
    retyped to take the `LoadedModel`. A sweep for the pattern found no
    others that receive switches.
- **`check-launch-model-boundary`'s allowlist is EMPTY.**
  - `overlay.rs` names no retired field in code.
  - The test that wrote `Deploy.target`/`extra` into a model to prove EDF
    slicing ignores them is deleted with the fields.
- **The agreement test (revision item 3) is
  `tests/system_toml_readers_agree.rs`**, which replaces W7's test.
  - It compares every tracked `system.toml` through both readers, on
    `[system]`, `[[component]]`, `[[bridge]]`, `[host.*]` and `[tiers.*]`.
    They agree on all of them.
  - Lower bounds keep it from comparing empty sets.
  - Mutation-tested: swapping two tier fields in one view goes red.
  - It also keeps W7's "the model carries no switch" check, asked of the
    serialised execution layer so it holds on either rlm.
- **No rlm call removed by W8b remains.** Measured, not argued: nano-ros's
  CLI, orchestration IR and proc-macro were built and tested against rlm
  W8b through a local `[patch]`.

### W8c result (2026-10-13)

- **One commit, both pins:** play_launch `15258108` → `465dba4` (its rlm
  `v0.1.57` bump) and rlm `v0.1.56` → `v0.1.57` in the four manifests, both
  workspace locks, the nine board leaf locks and `nros-launch-resolve`'s.
  Each lock moves only rlm's packages (`0.1.10` → `0.1.11`, tag and hash).
  The two NuttX FFI locks cannot resolve without a NuttX libc checkout, so
  their four rlm entries were edited to match the sibling NuttX board lock
  exactly (checked entry for entry).
- **Byte-identical, measured like W7:** every example workspace synced with
  main's CLI and resolver and with this commit's. 78 resolved models differ
  ONLY in the resolver's version stamp (`meta` … `version`, the play_launch
  commit); 74 entity-facts outputs and 78 entity inventories are identical.
- **Acceptance:** `check-launch-model-boundary` passes with an EMPTY
  allowlist (1314 Rust files). rlm's crate names no realizer (rlm
  `v0.1.56`/`v0.1.57`). nros-cli-core 1873/1873, nros-orchestration-ir +
  nros-macros 280/280.

### The 0.6.0 step (issue 1809) — ready, held for the bump (2026-10-13)

W8a kept the build fields of `[deploy.*]` and the typed `[param_services]`
parsing with a warning until 0.6.0. The deletion that follows is on branch
`feat/1809-deploy-removal`, on top of W8a, and is HELD for the 0.6.0 version
bump (maintainer decision 2026-10-12). Issue 1809 lists what it deletes. Both
tables are then refused by name through one helper,
`nros_orchestration_ir::retired_tables`, which every nano-ros reader of a
`system.toml` calls. It does not depend on W8c: rlm `v0.1.57` already reads
neither table.

## W9 — the carriers the old rule kept (follow-up, measured per road)

`NROS_DECLARED_NODES` and `NROS_DECLARED_SUBSCRIPTION_BUFFER_SIZE` were kept
"by design" because a model-only fact had no file (RFC-0100 2026-10-03).
Under the 2026-10-10 rule the parameter-service node count is
overlay-plus-model, so the descriptor can state it. Retire each carrier only
where the descriptor is NAMED on that road (CLAUDE.md: retirement is a
question about the road, never the field). Today a multi-entry configure and
a Zephyr west entry name none, so W9 starts with those roads or does not
start.

(That last sentence was stale when written: a Zephyr west entry has named its
descriptor since issue 1407, and a multi-entry configure its shared runtime's
since issue 1649. Every road that sent either carrier names one.)

### W9 result (2026-10-13)

- **Where the facts live now.** `[image] model_nodes` (new, schema-additive)
  is the model's `structure.nodes`: the MAX over a shared runtime's models,
  and a standalone leaf's `[[component]]` count. It is NOT `[image]
  node_count`, which is the node TABLE and holds a slot for the lifecycle
  family's executor node (issue 1740). Stating one for the other is wrong both
  ways: it under-sizes the table or over-counts the parameter services.
  `[types] max_wire_bound_bytes` was already the take buffer's field (issue
  1595).
- **The unwired image states both.** `Wiring::Unwired` gained `model_nodes`
  (`Some` wherever the carrier was sent, `None` for a standalone leaf that
  declares no entities, whose declaration is the opt-in). With bound tables
  handed (the cmake roads) it states the closure maximum through the same
  `closure_wire_bound` the wired road calls. Zero nodes is stated as absent,
  because every consumer floors the count at one, so an empty model still
  writes no file.
- **Readers.** `nros-zpico-build` (queryable table, liveliness pool) and
  `nros-rmw-xrce-cffi` (service-server slots) read `model_nodes`; `nros-node`
  reads `RX_BUF` from the descriptor and the crate default only. Producers
  deleted: the cmake fold and emission (`NROS_ENTITY_NODES_MAX`,
  `_nros_take_buffer_env`), `entity_facts::facts_from_model` /
  `facts_from_leaf`, the leaf env's node row (`with_fact_infra` now takes the
  count from the model it reads once). `check-knob-single-reader` has a
  RETIRED entry, wave "phase-486 W9", forbidding both spellings in cmake,
  zephyr and Rust.
- **Measured, 1649's method:** every build-script `out/` file and generated
  config header, main (A, carriers on) vs this branch (B, carriers retired),
  normalised for checkout paths and cargo hashes, with a negative control C
  (main with the node carrier dropped and no descriptor field):

  | road | image | A vs B | A vs C |
  | --- | --- | --- | --- |
  | cmake multi-entry | `workspaces/cpp` `native`, 7 entries, `param_services` temporarily on (reverted) | 0 of 96 | `ZPICO_MAX_QUERYABLES` 22→16, `ZPICO_MAX_LIVELINESS` 29→23 |
  | cargo workspace | `workspaces/features` `native_rust_custom_msg` (unwired), `native_rust_qos` (wired), `native_rust_params_xrce` | 0 of 91 (one cargo hash inside a path in XRCE's vendor record) | queryables 25→19 and 18→12, liveliness 38→31 and 26→20 |
  | cargo leaf | `native/rust/talker` | 0 | not sensitive (1 node, no service family) |
  | standalone cmake leaf | `native/cpp/talker` | 0 | not sensitive (as above) |

  The XRCE image has one node, so it cannot move; no tracked XRCE image has
  two. The take-buffer carrier was emitted by NONE of these configures: every
  closure measured carries unbounded types, so cmake's own guard refused it.
  Its retirement rests on issue 1595's reader measurement (a descriptor
  stating X sizes `RX_BUF` exactly as the carrier X did) plus
  `an_unwired_image_states_the_retired_carriers_facts_as_the_wired_road_does`
  (the unwired road answers the same maximum over the same tables as the
  wired one).
- **Not measured: Zephyr west.** It carried neither carrier, but its
  descriptors now state `model_nodes`. By reading, no west consumer uses it:
  the queryable table returns before the node count when no infrastructure
  fact arrives (west sends none), west states `ZPICO_MAX_LIVELINESS` and
  `ZPICO_MAX_QUERYABLES` itself, and XRCE's slot rule returns on the same
  absent fact. The build was attempted and failed on main and here alike:
  the shared Zephyr workspace registers no nano-ros module for these
  worktrees (`undefined symbol NROS`).
- **Left for later:** `NROS_DECLARED_INFRA_QUERYABLES`. Its KEPT reason was
  that a contract-less image has no file. Every resolved model's image now
  has one, so it can follow onto a descriptor field the same way.

## Migration

**nano-ros users: nothing to rewrite.** Every key a user writes today keeps its
meaning:

| You write | Before | After |
| --- | --- | --- |
| `[system] features = [...]` | model `execution.features` | overlay, as the default for every image |
| `[image.<id>] features = [...]` | (refused: unknown key) | overlay, for that image only (W2) |
| `[lifecycle] autostart = "..."` | projected onto every lifecycle node in the model | overlay default; a node's own launch or contract value wins |
| `[param_services]` | deprecated sugar | still parsed and warned; refused at 0.6.0 (issue 1809), naming `features = ["param_services"]` |
| `[deploy.<n>]` | build fields deprecated, placement live | build fields warned until 0.6.0; a placement-bearing block (`nodes`, `launch`, or several non-embedded blocks) is refused, naming `[host.*]` |
| `<lifecycle_node autostart="true">` | not parsed | per-node `Active` in the model (W5) |

What a user must do: **re-run `nros sync`** after upgrading. Models are build
artifacts (phase-330), so a model written by an older `nros` still carries
`features`; the new readers ignore it and read the overlay, which only a sync
writes. A missing overlay where the model carries switches is a named error
("re-run `nros sync`"), never a silent "no switches".

**play_launch users on Linux: nothing to rewrite.** None of the moved keys
had a Linux meaning. A `system.toml` passed through the legacy `--sched`
bridge still parses, and its `features` / `[lifecycle]` were ignored before
and are ignored now. New capability: `<lifecycle_node autostart="true">` and
`LifecycleNode(autostart=True)` now reach the model and `up` (W5).

**Downstream readers of the model (anyone else):** `execution.features`,
`Deploy.extra` and `Target::Mcu` stop appearing in models written after W6.
Old models still load.

## Out of scope

- Reconciling `[host.*]` with play_launch's `<arg>` + `if=` + `host:=`
  placement (both are topology).
- Moving nano-ros's `[tiers.<n>.<rtos>]` onto per-target platform files
  (play_launch's Phase 41.6).
