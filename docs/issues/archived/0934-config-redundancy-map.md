---
id: 934
title: "The same fact is authored in up to five config surfaces; no SSoT is declared for rmw, board or domain"
status: resolved
resolved: 2026-10-14
area: build, api
severity: medium
found: 2026-08-30
related: [0931, RFC-0065, RFC-0048, RFC-0046, RFC-0014, RFC-0033, phase-405, phase-484]
---

**Home: [phase-484](../../roadmap/phase-484-one-owner-per-fact.md)** — re-homed 2026-10-12 from phase-405, which is archived.

# Config redundancy map

A survey of every AUTHORED configuration surface, done to answer "where is the
single source of truth for X". For several important X there is no answer.

Method: inventory each authored file, then for every field find the code that
READS it. A field read by two independent resolution paths, with no precedence
stated between them, is the defect this issue names. Findings below carry the
confidence they were established with — several are already known and gated
rather than removed, and two could not be settled.

## The surfaces

| file | authored by | schema |
| --- | --- | --- |
| `<bringup>/system.toml` | user | `cargo_metadata_schema.rs:541` (`deny_unknown_fields`) |
| `<bringup>/launch/*.launch.xml` | user | ROS 2 XML, resolved by `nros-launch-resolve` |
| `nros-codegen.toml` | user | `rosidl-lower/src/config.rs:239` |
| `nros-sdk-index.toml` | repo | `orchestration/sdk_index.rs:22` |
| `examples/fixtures.toml` | repo | `scripts/build/fixtures-manifest.py` |
| `config/<platform>/nros-platform.toml` | repo | `platform_config.rs:66` |
| `package.xml` | user (nodes) / generated (bringup) | `cmake/NanoRosPackageXml.cmake:67` |
| leaf `.cargo/config.toml` | MIXED author + `nros sync` | `cmd/ws.rs:4068` |
| `build/nros/models/<bringup>/*.yaml` | GENERATED | `model_location.rs:38` |

## The redundancies that matter

**R1 — `rmw` is authored in five places.** `[system].rmw`, `[deploy.<t>].rmw`,
`[image.<id>].rmw`, `package.xml`'s `<nano_ros rmw=…>`, and CMake
`-DNROS_RMW`. The first three have a documented ladder (`resolved_rmw`,
`cargo_metadata_schema.rs:915`, plus the image fold at `facade.rs:111`). The
`package.xml` tuple and the CMake cache var are a SEPARATE resolution path with
**no stated precedence against the TOML ones** — `resolved_rmw` never sees the
`package.xml` value and the CMake path never sees `resolved_rmw`. RFC-0065's
Consequences section already names this class: "Two duplicate-declaration
hazards must be resolved, not inherited."

**R2 — `board` in four.** `[deploy.<t>].board`, `[image.<id>].board`,
`package.xml`, and the fixture rows. The duplication is live enough to have
grown arbitration code: `board_facts.rs:66-101` guards the case where two
`[deploy.*]` blocks name one board and resolve differently.

**R3 — `domain_id` in three**, plus runtime `$ROS_DOMAIN_ID`. First two have a
ladder; `[workspace.metadata.nros].domain_id_override` is documented as
"propagate instead of" with no shared helper.

**R13 — three ways to bind an entry to a deploy.**
`[package.metadata.nros.entry].deploy`, CMake `nano_ros_entry(DEPLOY …)`, and
`[image.<id>].entry`. RFC-0065 already flags `freertos_entry/Cargo.toml` as
carrying "the same rmw/domain_id/locator facts as the bringup's block, with no
stated precedence".

## Known, declared, not yet executed

* **R12** — `[deploy.<t>]`'s BUILD fields duplicate `[image.<id>]`. RFC-0065 D6
  rules this a split and opens a window on the build fields, "never the table".
  **Measured (phase-405 W5): the window cannot be executed as written.**
  `profile` and `features` have zero uses; `rmw` has two, both in fixtures with
  no `[image.*]` at all; and `board` is not merely a build field but the JOIN
  KEY `board-facts` uses to reach `[deploy.*.nros]` site config, with no image
  fallback. Four candidate blocks were probed and all four break softly.
  D6's "become deletable once their `[image.*]` lands" should read "once the
  RESOLVER READS the image" — this needs a code wave before a data wave.
  This entry also named `examples/workspaces/mixed`'s `[deploy.freertos]` as
  carrying a duplicate `board`; it does not — it carries `kind` and a `.nros`
  block and NO board, which is [issue 0940](0940-deploy-site-config-unreachable-without-board.md).
  The duplicated pair there is `[deploy.mps2-an385-freertos]` vs
  `[image.freertos]`.
* **R11** — `[system].features` vs the typed `[safety]`/`[param_services]`
  blocks. **Already closed in the data** (phase-405 W5): zero of 33 in-tree
  `system.toml` carry either. `[lifecycle]` is NOT part of this window —
  `deprecated_typed_capability_blocks()` names only the other two — and it is
  not convertible, because `features = ["lifecycle"]` can only say ON while
  `autostart` is read from the typed block alone. This entry originally listed
  `[lifecycle]` among the deprecated spellings; that was wrong. The real defect
  in `examples/workspaces/features` was `"lifecycle"` appearing in the
  `features` LIST *and* as a block, which `capability_enabled` ORs — fixed by
  dropping the list entry.
* **R4** — `[image.<id>].{board,conf}` vs `fixtures.toml`'s
  `board`/`conf_files`. Partly migrated: a row with `image =` derives, a row
  with `entry =` restates, and `validate_workspace_fixture` refuses both
  together with the comment "one fact in two places, which is this repository's
  named defect class".

## Duplications that are GATED rather than removed

These are honest about themselves and worth listing separately — the gate is
the mitigation, and the question is whether the gate is the end state.

* **R5** — SDK roots exist four times: `[deploy.<t>.nros].sdk`,
  `just/sdk-env.just` exports, `[build.zenoh].required_env`, and a `BOARDS`
  table in `scripts/check-site-config.py`. That gate's docstring says the
  duplication is transitional and it asserts agreement. The same file records a
  redundancy that WAS removed (netstack moved to the board descriptor) — the
  pattern to follow.
* **R15** — `[knobs.zenoh.tx]` vs Zephyr `NROS_ZENOH_TX_*`.
  `config/zephyr/nros-platform.toml` states the mirror requirement outright, and
  two gates enforce it.

## Two defects that are not redundancy but cost the user the same

* **R9** — `[[model]]` is TYPED (`cargo_metadata_schema.rs:741`) and has **no
  reader**; the same table is re-parsed as raw toml in `model_location.rs:165`,
  and `[system].default_launch` is read three ways (typed, raw-toml, and by
  `fixtures-manifest.py:758`). Two parsers of one table is the drift shape this
  repo repeatedly names.
* **R10** — `[wcet]` parses and validates, and **nothing in production reads
  it**; `wcet_profile_for`'s only callers are its own unit tests. A surface that
  costs authoring effort and affects nothing.

## Not settled — do not act on these without more reading

* **R8** — `platform` appears in `nros-sdk-index.toml [board.*]`,
  `nros-board.toml`, and `board-support.toml`. The KEY SPACES differ
  (`mps2-an385-freertos` vs `nros-board-mps2-an385-freertos` vs `Linux`), so these
  may be three keyings of a board that each carry a platform column rather than
  one fact three times.
* Whether `[deploy.<t>].{framework,optimize,target}` have any nano-ros reader
  at all — they come from the upstream `DeployBlock` and no nano-ros call site
  was found, but the rlm crate's own consumers were not exhaustively checked.
* The precedence when `package.xml`'s `<nano_ros rmw=>` disagrees with
  `[image.<id>].rmw`. No reconciling code was found; not every cmake→CLI
  invocation was traced.

## The knob surfaces multiply it further

A second survey covered Kconfig, env knobs, cmake arguments and cargo features.
It found the same facts settable in more places than the file survey saw.

| fact | authorities | reconciler |
| --- | ---: | --- |
| **locator / agent endpoint** | **9** | precedence covers 3 of 9; Zephyr carved out entirely |
| **rmw backend** | **8** | a cmake WARNING for one pair; nothing compares `BACKEND` to `[system].rmw` |
| **domain id** | **7** | one derivation edge (`CONFIG_NROS_CYCLONE_DOMAIN_ID` defaults from `NROS_DOMAIN_ID`); the rest unlinked |
| **ros edition** | **6** | three independent defaulting sites, all `humble` |
| **platform / board** | **6** | `NanoRosEntry.cmake:488-528` exists solely to reconcile three spellings |
| **executor callback capacity** | **4** | real precedence code, but it lives in the CLI where no cmake or Kconfig user sees it |

**A concrete, ungated duplication in a shipped template.**
`examples/templates/pure-c-workspace/CMakeLists.txt:19` says `BACKEND zenoh`;
line 22 names `SYSTEM demo_bringup`, whose `system.toml:3` says
`rmw = "zenoh"`. Two authored copies of one fact in one call, and **nothing
compares them** — `NanoRosWorkspace.cmake` builds the path to that `system.toml`
(`:351`) and never parses it. Same in the c-and-cpp-mixed template. A user who
edits one and not the other gets no diagnostic.

## Two live defects found on the way, neither about redundancy

**A 0460 instance the gate cannot see.** `nros_cargo_build.cmake:201-206`
forwards `ZPICO_SUBSCRIBER_RING_DEPTH`, `ZPICO_MAX_LARGE_SUBSCRIBERS` and
`ZPICO_SUBSCRIBER_LARGE_SIZE` from Kconfig, but their reader
(`nros-rmw-zenoh/build.rs:37,43,45`) resolves them through `env_usize`, whose
`KCONFIG_KNOBS` lookup misses and falls through to a bare `std::env::var`. On the
Zephyr RUST lane that yields the crate default whatever `.config` says — issue
0460 verbatim, on three pool-sizing knobs. `check-kconfig-knob-forwarding.sh`
passes because its match arm is "the literal name appears in the file"
(satisfied by the `rerun-if-env-changed` lines) and its anti-pattern arm greps
for `env::var("&lt;NAME&gt;")` spelled literally, which this file never writes because
the name arrives as a parameter. The gate's own comment warns about this shape
(issue 0751) and the check it added is still name-literal. **Code path is
confident; not measured on a built image.**

**`nros new-entry` emits a Kconfig symbol that does not exist.**
`cmd/new_entry.rs:606` writes `CONFIG_NROS_XRCE_AGENT_LOCATOR=…` into a
generated conf. No such symbol is declared in `zephyr/Kconfig` — the real pair is
`NROS_XRCE_AGENT_ADDR` + `_PORT`. Zephyr's merge is silent about unknown
symbols, so every XRCE scaffold ships an inert line.

## Dead surfaces

* **`CONFIG_NROS_TRANSPORT_SERIAL`** (`zephyr/Kconfig:720`) — zero references
  anywhere in the tree.
* **`CONFIG_NROS_INIT_DELAY_MS`** (`:935`, default 2000) — zero source readers,
  but `docs/guides/zephyr-setup.md:220` and `book/src/getting-started/zephyr.md:249`
  both document it as live, and `docs/guides/cpp-api.md:451` shows a code sample
  using it. Same shape as archived issue 0377.
* **`nros_feature_set(BOARD …)`** — its own comment says "accepted but UNUSED
  since phase-338 W5.a".
* **`nano_ros_entry`'s `ARGS` / `LAUNCH_ARGS`** — no users, and `LAUNCH_ARGS`
  has no path at all through the `nano_ros_add_executable` verb.

## `nano_ros_add_executable` and `nano_ros_entry` agree by accident

`NanoRosVerbs.cmake:85` parses no `BRINGUP`, no `PANIC`, no `LAUNCH_ARGS`, no
`NAME`. Three in-tree zephyr entries pass `BRINGUP` and `PANIC` to it anyway.
They work because those land in `_NRE_UNPARSED_ARGUMENTS`, get spliced into
`_srcs`, and are forwarded as `SOURCES`, where `nano_ros_entry`'s own
`cmake_parse_arguments` **re-tokenizes them back into keywords** and leaves
`SOURCES` empty. Correct outcome, entirely by accident of flat-list re-parsing.
**Add one positional source to any of those three calls and it breaks.**

## The documentation is a redundancy surface too

`book/src/reference/static-pool-inventory.md` is generated, lists 47 knobs, and
says "Set them as environment variables at BUILD time". About 30 of them are
ALSO settable as `CONFIG_NROS_*` on Zephyr, and the tx trio also as
`[knobs.zenoh.tx]`. A reader tuning a Zephyr image from that page cannot learn
the Kconfig spelling exists, or which wins. The hand-authored Kconfig tables in
`docs/guides/zephyr-setup.md` have already drifted — they document
`NROS_INIT_DELAY_MS` as live.

## The clearest statement of the problem is already in the tree

`scripts/check-zephyr-knob-agreement.py`, on the zenoh tx trio:

> They agree today … but only by coincidence: nothing derives one from the
> other … Two spellings of one fact is the drift this repo keeps paying for …
> **Not a substitute for merging the sources.**

That gate is the model for where most of this stands: detected, asserted, not
merged.

## What a fix looks like

Not "delete the duplicates". For each fact, name the SSoT and make every other
site a PROJECTION with a gate, or delete it:

1. **`rmw`, `board`, `domain_id`** — the bringup's resolved model is already the
   place all three end up. The `package.xml` tuple and the CMake cache var
   should read from it rather than race it. This is the one with real
   consequences, because the two paths can currently disagree silently.
2. **R9/R10** — one parser per table; delete `[wcet]` or wire it.
3. **R12/R11/R4** — execute the deprecations already declared.

Related: [issue 0931](0931-retire-model-and-default-launch.md) is the same
question one layer up, for `nano_ros_entry`'s arguments.

## Status (2026-10-14): most rows are gone; what is left

Re-measured against the tree:

| row | now |
| --- | --- |
| R1 `rmw`, R2 `board` | The `[deploy.<t>]` table is refused (`refuse_retired_deploy`, `cargo_metadata_schema.rs:1044`, phase-486 W8). A `package.xml` `<nano_ros deploy= …/>` element is refused too (`cmake/NanoRosPackageXml.cmake:19`); cmake reads the leaf's `system.toml` instead. That leaves `[system]` / `[image.<id>]` plus `-DNROS_RMW`. |
| R12 | Gone with `[deploy.*]`. |
| R13 | `[package.metadata.nros.entry] deploy` was retired by RFC-0098 D5 (`builder/entry.rs:784`). `nano_ros_entry(DEPLOY …)` and `[image.<id>].entry` remain. |
| R10 `[wcet]` | It has a production reader now: `cmd/build.rs:482` calls `wcet_profile_for_image`. |
| R3 `domain_id_override` | Still a field (`cargo_metadata_schema.rs:73`). Not re-ruled. |
| R4, R5, R15 | As recorded above: partly migrated, or gated. |
| R8, R9 | Not re-measured. |

The headline, "five surfaces and no SSoT for rmw/board/domain", no longer
holds for rmw and board. Each is authored in `system.toml` (`[system]` /
`[image]`), which phase-484 / RFC-0103 owns. What remains:

- R3;
- R13's cmake `DEPLOY` vs `[image].entry`;
- R4's `entry =` rows;
- the gated R5 / R15;
- the unsettled R8 / R9.

Close this once phase-484 rules on those, or split them into their own
issues.

## Resolution (2026-10-14)

This was a survey, and RFC-0103 / phase-484 ("one owner per fact") is the
program it asked for. The status above asked to close it "once phase-484 rules
on those, or split them into their own issues". Each remaining row is now
fixed here, recorded as settled, or split into its own issue.

| item | now | where |
| --- | --- | --- |
| R1 `rmw` | **fixed here.** `[deploy.*]` is refused by the schema (phase-486 W8). `resolved_rmw` is one ladder, cli > image > system. The `package.xml <nano_ros …/>` tuple is refused. The generated cmake root takes `BACKEND` from the resolved rmw. The residue: `nano_rosConfig.cmake` cached the leaf's `NROS_RMW` / `NROS_PLATFORM` / `NROS_BOARD` only when the cache was EMPTY, so editing `rmw` in `system.toml` (a CONFIGURE_DEPENDS input) reconfigured to the stale value silently. | `cmake/NanoRosLeafCache.cmake`: a value nano-ros cached (recognised by its helpstring, which also covers build dirs configured before) follows the file and says so. A person's `-D` or a project's own `set()` still wins. Gate `cmake-leaf-cache` (4 cases; dropping the follow branch fails case 2). |
| R2 `board` | resolved | `[image.*]` is the only board table. The tuple is refused. Fixture rows restate it under R4's gate. |
| R3 `domain_id` | resolved | `[host.*] > [system]`; the `[deploy]` rung is retired, and `domain_id_override` has no CLI reader left. |
| R4 fixture rows | gated, not removed | 0 `entry =` rows, 142 `image =`; `_require_image_rmw` checks the restated rmw. |
| R5 SDK roots | resolved | Derived (phase-484 W3); the site-config rule is inverted. |
| R8 `platform` | unsettled, as filed | Stated in the index `[board.*]`, `nros-board.toml` and `board-support.toml`. Still in "Not settled". |
| R9 `[[model]]` / `default_launch` | **split** | [issue 1895](1895-system-toml-models-read-raw-not-typed.md): three parsers, and the typed field has no reader. |
| R10 wcet | resolved | Has a production reader (`build.rs`). |
| R11 `[param_services]` | resolved | Refused by the schema. |
| R12 | moot | `[deploy.*]` is retired. |
| R13 entry ↔ deploy | **split** | `[package.metadata.nros.entry].deploy` is refused. The CMake `DEPLOY`/`BOARD` keyword still wins with no comparison: [issue 1894](1894-entry-deploy-board-keyword-beside-system-toml.md). |
| R15 knob chain | resolved | Generated: platform TOML → `knobs.toml` → Kconfig. |
| "agree by accident" (Verbs) | resolved | `nano_ros_add_executable` parses `BRINGUP` / `PANIC` / `LAUNCH_ARGS` itself (issue 1136). |
| `CONFIG_NROS_INIT_DELAY_MS` | **fixed here** | Deliberately user-facing (`gen-config-surface.py` `PROVIDED`), not dead. But the guide and the `zpico-zephyr` README taught it through `zpico_zephyr_wait_network`, which no longer exists. Both now call `nros_platform_zephyr_wait_network`. |

