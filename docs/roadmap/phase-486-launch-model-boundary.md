# Phase 486 — the launch model carries what every realizer reads; nano-ros's own facts move to an overlay

**Status (2026-10-11). W0 LANDED (census below, two design corrections); W1–W9 not started.** Implements RFC-0060's
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
| `execution.deploy.<n>.extra` (W0: `target = mcu:<board>` stays, it is placement) | `[deploy.<n>]` framework/profile/optimize/features/kind, `[deploy.<n>.nros]` | none. These describe a build; nano-ros itself moved them to `[image.*]` and `[board_config.*]` |

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

## Ordering — why the waves run in this order

nano-ros pins rlm by tag, so nothing breaks until nano-ros bumps the tag. The
one ordering constraint that matters is: **nano-ros must stop reading the
fields (W3) before it bumps to an rlm that no longer writes them (W7).**
Everything before W7 is additive and must leave every image byte-identical.

```
W0 census ─► W1 overlay (additive) ─► W2 per-image switches ─► W3 readers move ─► W4 descriptor rule
                                                                        │
             play_launch phase 86 (W5: lifecycle_node autostart) ───────┤
             rlm (W6: stop projecting; tag) ────────────────────────────┤
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

**Correction 1: `Target::Mcu{board}` stays in the model.** The board-slice
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

## W2 — per-image switches

- `[image.<id>] features` and `[image_defaults] features` in `SystemToml`.
  `[system] features` stays as the default.
- Unknown switch names are refused by name, at sync, naming the image.
- `examples/workspaces/features` states its switches on the native image only,
  as its own comment asks.

**Acceptance:** a two-image bringup with `param_services` on one image builds
the store in that image only (`IMPLIED_STORE_SLOTS` 32 vs 0), measured. Every
existing bringup is byte-identical, since none states per-image switches yet.

## W3 — every reader moves to the overlay

- The six readers, and any W0 adds, read the overlay. The model's
  `execution.features` is read by nothing in nano-ros.
- The lifecycle default is applied by nano-ros: for each lifecycle node whose
  model `lifecycle_autostart` is `None`, the overlay default applies. A
  per-node value from the launch file or contract wins.
- `nros::main!` Form 1 (PR #1843's `leaf_capabilities`) reads the leaf's
  overlay through the same reader as every other road.
- **Gate:** `check-launch-model-boundary` — no nano-ros source outside a
  named allowlist reads `execution.features`, `Execution::features`, a
  `Deploy` `extra` key, or projects a system-wide lifecycle default from the
  model. The allowlist is empty at W8.

**Acceptance:** every W0 image is byte-identical: knobs diffed per consumer,
plus the sizes headers and `OUT_DIR` outputs (the PR #1830 method). Mutation:
pointing one reader back at an empty model field turns a test red.

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

## W5 — play_launch: the per-node lifecycle fact from the launch file (play_launch phase 86)

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

- `apply_to_launch` no longer writes `execution.features`, no longer projects
  `[lifecycle]` onto nodes, and no longer writes `Deploy.target = mcu:<board>`
  or `Deploy.extra`.
- `SystemConfigToml` stays LAX: the keys still parse (into nothing), so a
  valid nano-ros `system.toml` still resolves. They are deleted from the
  struct only after W8, together with nano-ros's re-exports of them.
- The model types keep deserialising old models: `Execution.features` and
  `Deploy.extra` become read-tolerant. They load and are ignored, the
  `jitter_ms` precedent (rlm phase 68), and the golden fixture keeps them on
  disk. `SCHEMA_VERSION` stays 1.
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
   `nros sync` every bringup.

**Acceptance:** every W0 image is byte-identical to W4's. The models no longer
carry `features`, and nothing in nano-ros noticed. Tier 2 run
(`just build-test-fixtures lane=tier2` + `just ci matrix`), since codegen and
cmake inputs moved.

## W8 — retirement

Deleted, each with a named refusal where a user could still write it:

| What | Where | Replaced by | Refusal |
| --- | --- | --- | --- |
| `[param_services]` typed block | nano-ros `SystemToml`, rlm `SystemConfigToml` | `features = ["param_services"]` | nano-ros: parse error naming the replacement (already deprecated via `deprecated_typed_capability_blocks`) |
| `[deploy.<n>]` build fields: `board`, `framework`, `profile`, `optimize`, `features`, `[deploy.<n>.nros]` | rlm `DeployBlock`, nano-ros's re-export | `[image.<id>]`, `[board_config.<board>]` | nano-ros: parse error naming `[image.*]` |
| `Execution.features`, `Deploy.extra`, `Target::Mcu` in NEW models | rlm model | the nano-ros overlay | none needed (writers gone; readers tolerant) |
| `SystemConfigToml.lifecycle`, `.param_services`, `SystemDefaults.features` | rlm | nano-ros `SystemToml` | none: rlm ignores keys it does not own |
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
