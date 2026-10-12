# Phase 486 — the launch model carries what every realizer reads; nano-ros's own facts move to an overlay

**Status (2026-10-12). W0–W3 LANDED; W4 in review; W5 landed in play_launch (`ca7eb1a`, its phase 86 W1–W4) and W6 in rlm (`v0.1.55`); W7–W9 open. Design revised 2026-10-12 (below) after re-reading rlm and play_launch `main`.** Implements RFC-0060's
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

Deleted, each with a named refusal where a user could still write it:

| What | Where | Replaced by | Refusal |
| --- | --- | --- | --- |
| `[param_services]` typed block | nano-ros `SystemToml`, rlm `SystemConfigToml` | `features = ["param_services"]` | nano-ros: parse error naming the replacement (already deprecated via `deprecated_typed_capability_blocks`) |
| `[deploy.<n>]`, the whole table (revised 2026-10-12: its placement half too) | rlm `DeployBlock`, nano-ros's re-export | build fields → `[image.<id>]`, `[board_config.<board>]`; placement → `[host.<n>]` | nano-ros: parse error naming `[image.*]` and `[host.*]` |
| `Execution.features`, `Deploy.extra`, `Deploy.target` / `Target` in NEW models | rlm model | the nano-ros overlay; nothing for `target` (no reader) | none needed (writers gone; readers tolerant) |
| `SystemConfigToml.lifecycle`, `.param_services`, `SystemDefaults.features`, `lifecycle_autostart()` | rlm | nano-ros `SystemToml` | none: rlm ignores tables it does not declare |
| nano-ros's second parse of rlm's keys (issue 0293) | nano-ros `SystemToml` | shared keys through rlm's struct; strict on the union via `serde_ignored` | unknown key refused by name, as today |
| realizer names in rlm doc comments | rlm | the CHANGELOG and the realizers' docs | — |
| the PR #1830 store-only special case | nano-ros sizing descriptor | the general rule (W4) | — |
| the W3 gate's allowlist | nano-ros | empty | the gate itself |

A read-tolerant model field is deleted outright one rlm minor release after
W7, when no tracked model or fixture carries it. That is checked, not assumed:
rlm's golden fixture is the only file allowed to keep it.

**Acceptance:** `check-launch-model-boundary` passes with an empty allowlist.
Every moved key in a fixture `system.toml` either resolves through the overlay
or is refused by name. Nothing is silently dropped.

## W9 — the carriers the old rule kept (follow-up, measured per road)

`NROS_DECLARED_NODES` and `NROS_DECLARED_SUBSCRIPTION_BUFFER_SIZE` were kept
"by design" because a model-only fact had no file (RFC-0100 2026-10-03).
Under the 2026-10-10 rule the parameter-service node count is
overlay-plus-model, so the descriptor can state it. Retire each carrier only
where the descriptor is NAMED on that road (CLAUDE.md: retirement is a
question about the road, never the field). Today a multi-entry configure and
a Zephyr west entry name none, so W9 starts with those roads or does not
start.

## Migration

**nano-ros users: nothing to rewrite.** Every key a user writes today keeps its
meaning:

| You write | Before | After |
| --- | --- | --- |
| `[system] features = [...]` | model `execution.features` | overlay, as the default for every image |
| `[image.<id>] features = [...]` | (refused: unknown key) | overlay, for that image only (W2) |
| `[lifecycle] autostart = "..."` | projected onto every lifecycle node in the model | overlay default; a node's own launch or contract value wins |
| `[param_services]` | deprecated sugar | refused at W8, naming `features = ["param_services"]` |
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
