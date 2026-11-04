# Phase 531 — Workspace-first project layout: bringup-only `system.toml`, one build road, the leaf shape retired

**Status (2026-11-05). OPEN — W1 (the measurement) and W2 (the gates and the
sync warning) done; W3 next.**
Implements [RFC-0121](../design/0121-workspace-first-project-layout.md) D1 to
D8, with its amendments to RFC-0098, RFC-0116 D6, RFC-0026/0066, RFC-0100,
RFC-0118 D2 and RFC-0120.

Depends on:

- [phase-528](phase-528-run-on-host-and-emulator.md) W4, for `nros run`, whose
  target argument W4 here extends;
- [phase-529](archived/phase-529-flash-monitor-bench.md) W3, for `nros flash`, the same;
- [phase-530](phase-530-nros-configure-single-pass.md), whose `nros configure`
  becomes the place W3's launch generation runs. W3 can land on today's `nros
  sync` first and move with phase-530;
- [phase-525](archived/phase-525-adoption-front-doors.md) W4 (the timed first chapter):
  it scaffolds a single package today. Its chapter text and probe move with W8
  here, so the two phases agree on the scaffold before either rewrites the
  chapter.

---

## Why

A standalone package is built by its own road, and that road, not the launch
file, is what every later feature had to be written for twice: the selection
crate, the `--from-leaf` sizing producer, `write_for_leaf`, three bringup
synthesisers and the shape checks in `cmd/build.rs`. RFC-0121 makes every
project a workspace with `system.toml` only in a bringup, and keeps the
single-node experience by a scaffold and a node-name target rather than by a
second shape.

## Work items

### W1 — Measure what retires

Before anything is removed, list every function, file and gate the standalone
shape reaches, and say for each whether a workspace image also reaches it (some
`leaf_*` code serves workspace entries too). Inputs: the call graph from
`leaf_system`, `leaf_settings`, `leaf_kconfig`, `leaf_entity_env`,
`west_leaf_sizing`, `sizing_descriptor` (`write_for_leaf`, `--from-leaf`), the
three synthesisers, and the shape checks in `cmd/build.rs` and `cmd/ws.rs`.

**Acceptance:** a table in this doc with each item, its line count, whether the
workspace road reaches it, and the W7 step that deletes it. It also records how
many tracked `system.toml` sit beside code (223 on 2026-11-03), split by tree
(`examples/`, `packages/testing/`, templates).

**Status: DONE (2026-11-05, PR #3119, measured at `03473878d6`).** The result
follows.

#### How reach was measured

- **Call graph.** Every function in the files below was listed with its span
  (non-test lines only; `#[cfg(test)] mod tests` excluded), and its callers
  were found by name across `packages/{cli,core,testing}`.
- **Builds, with coverage.** A coverage-instrumented `nros`
  (`cargo llvm-cov show-env`, with `nros_orchestration_ir` added to the
  instrumented crates) was driven down both roads, and each function's
  execution count was read per road:
  - workspace road: `nros sync` and `nros build --dry-run --all` in
    `examples/workspaces/{rust,c,cpp,mixed}`, a real `nros build linux` in
    `rust` (cargo) and `c` (CMake), and the Zephyr module hook's own call
    (`nros dev image leaf-system <bringup> --image zephyr --kconfig-out …`,
    as `zephyr/cmake/nros_image_kconfig.cmake` issues it for a generated west
    application). 532 profiles in total;
  - leaf road: `nros sync`, `nros build --dry-run` and `nros build` in
    `examples/linux/{rust,c,cpp}/talker`. 27 profiles.
- **Reading the results.**
  - A function that ran only on the leaf road is **leaf-only**.
  - A function that ran on the workspace road but only to answer "this is
    not a leaf" (it returns `None` for a directory with no `system.toml`
    beside code) is a **probe**. A probe goes when its call sites go.
  - A function the runs did not reach is classified by its callers. The row
    names the caller that decides it.
  - The proc-macro (`nros-macros`) is not in the instrumented binary, so its
    rows rest on the call graph and on the generated entry's manifest (below).

#### Tracked `system.toml` beside code

262 tracked `system.toml`; **223 beside a tracked `Cargo.toml` or
`CMakeLists.txt`** (unchanged from 2026-11-03) and 39 in bringup-only
directories.

| Tree | Beside code | of which `Cargo.toml` / `CMakeLists.txt` / both | Migrated by |
| --- | --- | --- | --- |
| `examples/` (excluding templates) | 162 | — | 23 in W6 (the 22 `<plat>/<lang>/talker` witnesses and `linux/c/custom-platform`); 139 in phase-535 (RFC-0122) |
| `examples/templates/` | 11 | 2 / 9 / 0 | W6 |
| `packages/testing/` | 50 | — | W6 |
| **total** | **223** | 101 / 109 / 13 | |

By platform directory, `examples/` holds: linux 43, mps2-an385-baremetal
25, rv-virt-threadx 20, qemu-armv7a-nuttx 19, mps2-an385-freertos 18,
threadx-linux 18, zephyr 18, rv-virt-nuttx 1. The 13 "both" sites are Zephyr
and ThreadX-RISC-V Rust leaves, which are west/CMake applications with a cargo
package inside.

Node-package manifests that name a selection or board crate (W2's second
gate, same scope): **100 rows**, of which 94 name `<pkg>_nros_selection` and 6
name `nros-board-threadx-qemu-riscv64` (the `rv-virt-threadx/rust/*` leaves).
Hand-written entry packages (`*_entry`, 8 of them under `packages/testing/`)
name their selection crate too. They are out of that gate's scope: an entry
is the platform seam, and it retires with `nros new entry` in W7 step 8.

#### What retires, and what the workspace road still needs

Lines are non-test lines. "WS reach" uses the classes defined above.

| Item | Lines | WS reach | Evidence | W7 step |
| --- | --- | --- | --- | --- |
| `orchestration/nros_config.rs` `synthesise_self_bringup` + the `is_self_bringup_eligible` loop in `from_cargo_metadata`; `[package.metadata.nros.deploy.*]` schema | 198 + ~25 | none | not run on either road; no example declares `[deploy.*]` | 1 |
| `orchestration/launch_synth.rs` `synthesise_self_model`, `enumerate_launch_files`, `is_self_bringup_eligible`; the `cmd/plan.rs` self-bringup block (lines 189–225) | 42 + ~37 | none | reached only from `nros plan <pkg-dir>` | 2 |
| `launch_synth.rs` `discover_pkg_name`, `discover_exec_target` and their four readers | 147 | leaf-only | ran on the leaf road only, via `synthesise_leaf_launch` | 2 (W3's bringup generator reads `[[component]]` and needs none of them) |
| `cmd/ws.rs` `synthesise_leaf_launch`, `carry_leaf_contract` | 65 + 28 | probe / leaf-only | `synthesise_leaf_launch` ran 26 times on the workspace road, each time returning `None` (no `system.toml` beside code) | **kept, moved**: W3 rewrites it to read bringups (RFC-0121 D3) |
| `orchestration/facade.rs` `write_leaf_selection`; `leaf_system::LEAF_SELECTION_DIR` | 168 | leaf-only | the workspace road writes its facade through `write_facade` | 3 |
| `cmd/leaf_settings.rs` `write_selection`, `write_selection_for`, `depends_on_selection`, `check_board_crate_dependency`, `check_lowered_edition_features`, `dependency_tables`, `system_toml`, `image_ros_edition` | 210 | probe / leaf-only | `write_selection` ran 7 times on the workspace road, from `run_sync`'s per-package loop, always returning `None` | 3 |
| `cmd/build.rs` `check_leaf_selection` | 26 | leaf-only | | 3 |
| `nros-macros` `leaf_selection_crate`, `reroot_through` (the board-path re-root) | 33 | leaf-only (call graph) | a generated entry names its board crate, so the re-root never fires for it (doc comment at `main_macro.rs:3036`) | 3 |
| **`leaf_system::selection_crate_name`** | 3 | **live** | the generated entry depends on its own selection crate (`build/posix-zenoh/linux_entry/Cargo.toml`: `linux_entry_nros_selection = { path = … }`), and `nros-macros`'s `entry_selection_crate` names that crate through this function to register the backend (RFC-0071 D11) | **kept**, see the findings below |
| `cmd/sizing_descriptor.rs` `write_from_leaf`, `leaf_declaration`, `is_cargo_leaf` (`--from-leaf`) | 215 | leaf-only | | 4 |
| `sizing_descriptor.rs` `write_for_leaf`, `descriptor_path_for_leaf`, `leaf_store_infra`, `for_leaf_declaration`, `for_leaf_census`, `is_leaf_declaration` | 172 | leaf-only, except `is_leaf_declaration`, a probe (31 runs on the workspace road, each answering false) | | 4 |
| `cmake/NanoRosSizingDescriptor.cmake` `nros_sizing_descriptor_from_leaf` and the leaf-path arm of `nros_sizing_descriptor_cargo_env` | ~120 of 1072 | leaf-only | guarded on `${LEAF}/system.toml` | 4 |
| `cmake/NanoRosLeafEntityFacts.cmake` | 229 | leaf-only | its only carrier is `--from-leaf`; `nano_rosConfig.cmake:166` calls it for `CMAKE_CURRENT_SOURCE_DIR`, which has no `system.toml` in a generated CMake root | 4 |
| `cmd/west_leaf_sizing.rs` (whole file) and `_nros_west_leaf_sizing` in `zephyr/cmake/nros_cargo_build.cmake` | 273 + ~60 | leaf-only (call graph) | the hook returns unless `${APPLICATION_SOURCE_DIR}/system.toml` exists, and a generated west application has none (`builder/west_app.rs:103`) | 5 |
| `cmd/build.rs` `plan_single_package`, `plan_leaf_image`, `plan_single_package_cmake`, `unsynced_inputs`, and the two dispatch lines in `plan_for` (615–621) | 47 + 212 + 149 + 31 + 7 | probe / leaf-only | the two `plan_single_package*` functions ran 6 times each on the workspace road, each returning `None` | 5 |
| `cmd/build.rs` `zephyr_leaf_application`, the `is_package_dir(&root)` term of `package_mode` (689) | 14 + 1 | probe | returns `None` unless the root is the bringup, which only a Zephyr leaf is | 5 |
| `cmd/build.rs` `leaf_image_net` | 19 | live (call graph) | also called by `cmd/leaf_system.rs::run` for a bringup image whose `os_config` names board network files | **kept** (rename) |
| `cmd/build_next.rs` `declared`'s leaf arm (605–630) and `cmake_leaf_executable` | ~25 + 19 | leaf-only | | 5 |
| `cmd/ws.rs` `run_sync`'s `single_pkg_mode` arms (3571–3619, 3721, 3878, 3972–3976), `scan_one_pkg_dir`, `scan_one_pkg_dir_inner`, `generated_edition`, `cargo_dependency_names`, `emitted_msg_dep_closure`, `is_generated_path`; the `nros ws model-dims` twin at 6817–6830 | ~70 + 50 + 49 + ~20 | leaf-only | | 5 |
| `cmd/leaf_settings.rs`, the leaf-only half: `settings_path`, `target_dir`, `leaf_package`, `resolve`, `resolve_west`, `resolve_west_image`, `leaf_image_from_decl`, `build_command`, `write`, `write_shared`, `settings_include_rel`, `wire_settings_into_leaf_config`, `states_its_own_build_flags`, `baked_ros_args`, `leaf_app_rung`, `image_layers`, `transport_implications`, `may_name_rmw_file` | ~560 of 1586 | probe / leaf-only | `leaf_package`, `resolve_image` and `resolve_for` ran on the workspace road only as `write_selection`'s probe | 5 |
| `cmd/leaf_settings.rs`, the shared half: `board_fact_layer`, `board_knob_layer`, `board_rung*`, `app_rung`, `knob_env_row`, `registry_knob_of_env_row`, `refuse_retired_env_keys`, `link_build_implications`, `session_link_build_implications`, `transport_implications_of`, `image_block_layers*`, `image_layers_over`, `is_settings_include`, `resolve_claimed_entry`, `write_entry_selection` | ~620 | live | ran on every workspace `plan_for` (`cmake_image_rows`, `image_config_digest`) and in the Zephyr hook. The last two serve hand-written entries | **kept** (the file should be renamed for what it is: the image's settings layers); `resolve_claimed_entry` and `write_entry_selection` go with W7 step 8 |
| `orchestration-ir` `leaf_system.rs`: `read`, `from_system_toml`, `declared_entities`, `board`, and the "a `system.toml` beside the entry answers first" arm of `for_entry` / `claiming_image` | 8 + 49 + 11 + 3 + ~6 | probe / leaf-only | `read` ran 613 times on the workspace road, always returning `None` | 5 |
| `orchestration-ir` `leaf_system.rs`: `read_image`, `image_system`, `for_entry`, `claiming_image`, `entry_package_name`, `flatten_knobs`, `is_package_dir`, the key readers, `link_kind_of_locator`, `is_byte_stream_link` | ~560 | live | `for_entry` ran 5164 times and `image_system` 3444 times on the workspace road. `is_package_dir` is also the predicate RFC-0121 D7's refusal needs | **kept** (the module is the `system.toml` reader for bringups) |
| `orchestration-ir` `refuse_retired_manifest_keys`, `retired_deployment_keys` | 40 | live | refuses `[package.metadata.nros.deploy.*]` in any package | 7, with `check-leaf-deployment-spelling` R1, once no reader of those keys remains (step 1) |
| `cmd/leaf_system.rs` `run` / `rows` / the `board_*` and `kconfig` arms (`nros dev image leaf-system`) | ~470 of 832 | live | the Zephyr hook calls it with the bringup directory for every generated west application (measured: 473 lines ran) | **kept** |
| `cmd/leaf_system.rs`, the `--write-settings` and `resolve_image` arms of `run` (616–639), `qemu_nic`, and the `find_package(nano_ros)` consumer `nano_ros_read_leaf_system` (`cmake/NanoRosPackageXml.cmake` 309–530) | ~24 + 43 + 222 | leaf-only | the CMake function returns unless `${DIR}/system.toml` exists; `qemu_nic` is called by `just/freertos.just` on a leaf | 5 |
| `cmd/leaf_kconfig.rs` | 665 (whole) | live | the Zephyr hook on a bringup ran 426 lines of it (`render_over`, `defaults_text`, `fragment_text`, `write_fragment`) | **kept** |
| `leaf_entity_env.rs`, the leaf-only half: `declared_entities`, `inventory_for_leaf`, `manifest_infra`, `leaf_infra`, `leaf_model`, `leaf_model_path`, `leaf_env`, `with_fact_infra`, `with_leaf_monitor_rows`, `env_rows` | ~386 | leaf-only | | 5 |
| `leaf_entity_env.rs`, the shared half: `declaration_from_probe`, `source_name`, `qualified_type`, `render_env_sidecar` | ~241 | live | `render_env_sidecar` is `cmd/build.rs::derived_pool_env`'s renderer; `declaration_from_probe` ran 1019 times on the workspace road | **kept** |
| `leaf_entity_env.rs` census readers (`census_*`) and the census arm of `leaf_kconfig::render_over` | ~60 + ~20 | none (13 leaves state `entities = "census"`, no bringup does) | | **kept, moved**: the declaration moves into the bringup's `[[component]]` row in W6 |
| `leaf_payload_classes.rs` `leaf_bound_inventory`, `leaf_bound_rows`, `payload_classes_for_leaf`, `join`; `leaf_take_buffer.rs` `take_buffer_for_leaf` | ~104 | leaf-only | the bound-table readers (`generated_bound_tables`, `bound_rows_from_tables`, `project_bound_rows`, `take_buffer::derive`) ran on the workspace road and stay | 5 (the derivation itself moves to the model road, or W5 keeps the crate defaults there as today; see the findings below) |
| `cmake/NanoRosLeafCache.cmake`, `cmake/NanoRosLeafCensus.cmake`, `nano_rosConfig.cmake`'s leaf-facts block | 38 + 48 + ~40 | leaf-only (call graph) | | 5 (census: moved with the census readers above) |
| `scripts/build/leaf-settings.sh`, `scripts/build/leaf-config-includes.py`, `scripts/lib/leaf_target.py` | 252 + 249 + 126 | leaf-only | the fixture builder's leaf rows | 5, with W6's fixture rewrite |
| `provider_overrides.rs` `leaf_system_toml_fallback` | ~32 | leaf-only | | 6 |
| gates: `check-leaf-deployment-spelling` (R1, R3), `check-msg-dep-is-path`'s two leaf arms, `check-image-locator-bake`'s leaf-row scope, `check-image-panic-policy`'s selection lookup, the single-package exemptions in `check-workspace-root-build-files` and `check-no-tracked-workspace-roots`, `check-bringup-workspace-root`'s leaf arm | — | guard the leaf shape only | | 7 (R2 of `check-leaf-deployment-spelling`, the `package.xml` tuple, guards every package and stays) |
| gates kept: `check-leaf-conf-nros-knobs` (covers workspace-image `.conf` files too), `check-west-leaf-vocabulary` (Zephyr build-dir names, any shape), `check-leaf-lockfiles`, `check-example-leaf-target-dirs`, `check-example-leaf-build-dirs` (in these four, "leaf" means any example directory) | — | live | | **kept** |

#### Findings for later work items

1. **`selection_crate_name` does not retire.** The generated workspace entry
   has a selection crate of its own, `<entry>_nros_selection`, through which
   `nros::main!` registers the backend (RFC-0071 D11). So W7 step 3 can
   delete the leaf's selection crate (`LEAF_SELECTION_DIR`,
   `write_leaf_selection`, `write_selection`, the macro's board-path re-root),
   but not the naming function. W5's acceptance text "`rg
   selection_crate_name` finds only the code W7 deletes" cannot hold as
   written. One related fact: `facade::write_facade` spells the same name
   inline (`format!("{entry_name}_nros_selection")`) rather than calling the
   function.
2. **Most `leaf_*` code is the workspace road's.** About 2,550 lines of
   `leaf_settings` + `leaf_system` (both) + `leaf_kconfig` + `leaf_entity_env`
   run for workspace images, the Zephyr hook among them. W7 step 5 is a
   deletion of the leaf-only halves (≈1,300 lines in those files) and a
   rename of the rest, not a deletion of the modules.
3. **The leaf road derives two sizes the workspace road does not.** These are
   the payload classes and the take buffer (`cmd/build.rs::derived_pool_env`
   passes `Refused` for both). Retiring the leaf road without moving that
   derivation would return 162 migrated examples to the crate defaults
   (issue 1125 measured 131,072 B of `.bss` from the payload-class default
   alone). W5's section-size tolerance check is where this would show up.
4. **Census and `entities = "census"` are leaf-only today** (13 leaves), and
   the declaration has to keep working after W6 moves those rows into
   bringups.
5. Measured total, all items above: ≈4,700 lines leaf-only or probe (about
   3,300 Rust in `nros-cli-core` / `nros-orchestration-ir` / `nros-macros`,
   about 760 CMake, about 630 build scripts), against ≈2,600 lines in the same
   files that the workspace road uses and keeps.

### W2 — The rule and its gates

- `check-system-toml-in-bringup` (fast line) refuses a tracked `system.toml`
  beside `Cargo.toml` or `CMakeLists.txt`. It is a ratchet whose allowlist is
  generated from W1's list and may only shrink.
- `check-node-package-selection-free` refuses a selection crate or board crate
  named in a node package's manifest, with the same ratchet.
- `nros configure` / `nros sync` warn on a `system.toml` beside code, naming
  `nros dev migrate-bringup`. The warning becomes a refusal in W7.

**Acceptance:** both gates in `just check fast` with mutation cases (a new
`system.toml` beside a `Cargo.toml` fails; removing one allowlisted site
without deleting its row fails the ratchet).

**Status: DONE (2026-11-05).**

- `just check system-toml-in-bringup` (`scripts/check-system-toml-in-bringup.py`)
  reads the tracked set.
  - Its allowlist, `.config/system-toml-in-bringup-baseline.txt`, holds W1's
    223 paths.
  - `--write-baseline` refuses to add a path.
  - The self-test runs on every invocation, 8 cases. Among them: a new
    `system.toml` beside a `Cargo.toml` fails, and so does one beside a
    `CMakeLists.txt` or at the root. A migrated site, or one whose build file
    went, fails until its row is deleted.
- `just check node-package-selection-free`
  (`scripts/check-node-package-selection-free.py`) covers every tracked
  `Cargo.toml` under `examples/` and `packages/testing/` that has a
  `package.xml` or `system.toml` beside it.
  - Exempt: entry packages, and board, runner and command providers.
  - It refuses a dependency key or `package =` target that is a
    `<pkg>_nros_selection` crate or an `nros-board-*` crate. Every dependency
    table counts, `[target.*]` included.
  - Its allowlist, `.config/node-package-selection-free-baseline.txt`, holds
    W1's 100 rows.
  - The self-test has 5 cases: a renamed board crate in a target table, a
    selection crate, a stale row, and the two exemptions.
- Both are fast-line gates: `just/check/cargo.just`, listed in
  `.config/gate-registry-baseline.txt`.
- `nros sync` warns once per scanned package whose directory holds
  `system.toml` beside a build file, in either layout. The warning names the
  file and `nros dev migrate-bringup <dir>` (W6), and says a later release
  refuses the layout (RFC-0121 D7). It carries no internal ids, because
  `check-user-surface-ids` keeps those off the user surface. The helper is
  `system_toml_beside_code_warnings` in `cmd/ws.rs`, and a unit test covers
  it.
  - It sits at sync's package scan, the one place that sees every package
    before configuration is resolved. `nros configure` (phase-530) has not
    landed, so it is not there yet.
  - `nros build` does not warn yet. D7's refusal in `nros build` is W7 step 9.

### W3 — A missing launch file is generated, in any bringup

Move `synthesise_leaf_launch` to read bringup packages: a bringup with no
`launch/` gets `<ws>/build/nros/launch/<bringup>/system.launch.xml` from its
`[[component]]` rows. `nros plan explain` marks it generated. A bringup with
neither is refused, naming both remedies.

**Acceptance:** a fixture workspace whose bringup has no `launch/` resolves the
same SystemModel as the same workspace with the generated file copied into
`launch/` (models compared byte for byte); deleting `launch/` returns to
generation.

### W4 — `nros new` writes a workspace; `nros run <node>`

- `nros new <ws> --board <b> --template <t>` writes `<ws>/src/<node>/` (a
  library node package) and `<ws>/src/<node>_bringup/` (`package.xml`,
  `system.toml`, no `launch/`). Inside an existing workspace, `nros new <node>
  [--to <bringup>]` adds a package and, with `--to`, a `[[component]]` row
  and an `<exec_depend>` on the package in the bringup's `package.xml`;
  without `--to` it touches no bringup.
- The `run` / `flash` / `monitor` / `build --packages-up-to` target resolver
  gains RFC-0121 D5's rungs, with `--why` naming the rung and every ambiguous
  case refused with the spellings that would resolve.

**Acceptance:** `nros new sensor_ws --board mps2-an385-baremetal --template
talker && cd sensor_ws && nros run sensor_node --expect 'Publishing: 0'`
passes on QEMU. A workspace with two bringups naming one node refuses `nros run
<node>` and lists both.

### W5 — One build road

Every image builds through the generated entry. A node package is a library;
its `main` is generated. The hooked roads of RFC-0120 D8 (west, CMake) build
the generated entry, which records its bringup.

**Acceptance:** each platform family's single-node example builds and runs
through the workspace road, with images whose section sizes match the
standalone build's within the phase-445 tolerance. `rg selection_crate_name`
finds only the code W7 deletes.

### W6 — Migrate the tree

> **Scope amended 2026-11-04 ([RFC-0122](../design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md),
> [phase-535](phase-535-example-reorg-and-cadence-ci.md)).** Under `examples/`,
> W6 migrates only the single-node witnesses (`<plat>/<lang>/talker`) and
> `custom-platform` in place. The pairs, variants, configurations and feature
> demos move into the language workspaces in phase-535 instead.

- `nros dev migrate-bringup <dir>`, the one-shot of RFC-0121 D8. It is tested
  on a copy of each shape: Rust, C, C++, Zephyr west, a hand-written `main`, and
  a fixture row.
- Run it over `examples/`, `packages/testing/` and the templates, in batches by
  platform directory so each PR's fixture rebuild stays one lane. Each batch
  rebuilds its lane's fixtures and runs the lane (`just test <platform>`).
- `fixtures.toml` rows keep their coordinates (`row_coord()` unchanged), so
  lane membership does not move; `matrix_fixture_coverage.rs` and
  `check-west-leaf-vocabulary` stay green per batch.
- The ratchet allowlist from W2 shrinks with each batch.

**Acceptance:** the W2 allowlists are empty. `just ci` (tier 1) and `just ci
matrix` (tier 2) are green on the migrated tree, with fixture counts unchanged
from before W6.

### W7 — Retirement

Delete what W1 listed as reachable only from the standalone shape, in this
order, each step with its own PR and gate run:

1. the `[deploy.*]` manifest metadata and `synthesise_self_bringup`;
2. `synthesise_self_model` and the launchless-package detection in
   `launch_synth.rs`;
3. the `<pkg>_nros_selection` crate (`leaf_system::selection_crate_name`,
   `LEAF_SELECTION_DIR`) and the macro path that emits it;
4. `--from-leaf`, `write_for_leaf` and the leaf rows of
   `NanoRosSizingDescriptor.cmake`; RFC-0100's road table loses its leaf rows;
5. the leaf-only halves of `leaf_settings`, `leaf_system`, `leaf_kconfig`,
   `leaf_entity_env`, `west_leaf_sizing`, and the shape checks in `cmd/build.rs`
   and `cmd/ws.rs`;
6. the `[overrides]` fallback to a root `system.toml` (RFC-0119 D4 as
   amended); `nros.toml [workspace.overrides]` is the only home;
7. the gates that police the retired shape (`check-leaf-deployment-spelling`,
   the leaf arm of `check-msg-dep-is-path`, and the like), each confirmed by W1
   to guard nothing that remains;
8. `nros new entry` (`cmd/new_entry.rs`, its `cmd/new.rs` dispatch and its
   entry packs) retires (RFC-0121 retirements, RFC-0085 D13 as amended). First
   prove the replacement: `nros new --image zephyr --board zephyr`, then
   `nros materialize <bringup>:zephyr`, builds and boots on native_sim in a
   workspace-first tree, as D13 proved the scaffold it replaces. The book
   pages that name it (`integration-zephyr.md`, `workspace-cpp.md`,
   `workspace-entry-pkg.md`, `user-guide/component-and-entry-pkg.md`) move to
   that flow in W8;
9. W2's warning becomes a refusal.

Each deletion is listed in `book/src/reference/retired-names.md` with its
replacement.

**Acceptance:** W1's table has every row marked deleted or kept-with-reason;
`just ci gate` and `just ci matrix` green after the last step.

### W8 — Book rewrite

The book teaches one shape, the workspace, from the first page.

| Page | Change |
| --- | --- |
| `getting-started/first-project.md` | `nros new <ws>` → `nros run <node>`; shows the workspace tree with its two packages and says the reader need not open the bringup yet (RFC-0116 D6 as amended) |
| `getting-started/anatomy.md` | explains node package, bringup and the generated launch file (`nros plan explain`) |
| `getting-started/first-node-{rust,c,cpp}.md` | node packages as libraries; no hand-written `main` |
| `getting-started/workspace-from-app-node.md`, `workspace-node-pkgs.md`, `workspace-bringup.md`, `workspace-entry-pkg.md` | merged into one "Growing your workspace" path: add a node package, add a component row, add `launch/` when a launch argument is needed, add an image |
| `getting-started/workspace-cpp.md`, `workspace-mixed-language.md`, `workspace-languages.md` | updated to the bringup-only `system.toml` and the node rung |
| platform chapters (`freertos.md`, `integration-zephyr.md`, `integration-nuttx.md`, `threadx.md`, `bare-metal.md`, `arm-fvp.md`, `px4.md`) | every example path and command becomes a workspace; `nros run <node>` / `nros flash <node>` |
| `porting/*.md`, `getting-started/build-as-subdirectory.md`, `vendored-tree.md` | a custom board is a board package under `src/` (RFC-0115 D3, RFC-0087 D6); projects that own their build stay as they are (RFC-0121 D6 scope) |
| `internals/creating-examples.md` | an example is a copy-out workspace (RFC-0026 as amended) |
| `reference/cli.md`, `build-commands.md`, `retired-names.md` | the target rungs, the retired flows and names |
| `troubleshooting-first-10-min.md` | the `system.toml`-beside-code refusal and its remedy |

- The `probe=NN` blocks of every rewritten page are re-run (`just probe
  bootstrap`, `just probe installed`, `just probe <platform>`), so the book's
  commands are executed, not just edited.
- Coordinate the first chapter with phase-525 W4 (the timed chapter), so one
  rewrite lands. Phase-525 W4 shipped the single-package chapter in PR #3076;
  phase-532 (PR #3093, RFC-0122) rewrites it to this phase's workspace shape
  ahead of the CLI, by the maintainer's ruling of 2026-11-04. Its `[[probe]]`
  rows carry `awaits = ["phase-530 W1", "phase-531 W4"]` and fail as an unmet
  precondition until then; when W4 lands, the phase-532 owner removes the
  `awaits` lines and replays each track.

**Acceptance:** `rg 'system.toml' book/src` finds no page that places it beside
code; `just book` builds; every touched page's probe track is green.

### W9 — Docs and pointers

- CLAUDE.md: the lines describing standalone leaves (RFC-0098 single-package
  shapes, the `<pkg>_nros_selection` crate, the leaf `.cargo/config.toml`
  include) are replaced by one-line pointers to RFC-0121.
- AGENTS.md, `examples/README.md` and `examples/workspaces/README-layout.md`:
  the workspace shape, and how to add an example.
- `docs/reference/canonical-build-path.md`: the leaf road leaves the map.

**Acceptance:** `just check fast` green (the doc-reference and identifier gates
cover these files).

## Acceptance for the phase

- No tracked `system.toml` sits beside code, enforced on the fast line, with no
  allowlist.
- `nros new <ws> --board <b> --template talker && nros run <node>` is the first
  chapter, and it passes its probe on QEMU.
- The standalone road's code is deleted (W1's table, all rows resolved).
- Tier 1 and tier 2 are green on the migrated tree.

## Not in scope

- Projects that own their build and have no `system.toml` (guest Zephyr
  applications, CMake `add_subdirectory` consumers, vendor-SDK projects).
- The open questions of RFC-0121 (bringup naming in the scaffold;
  rust-analyzer with the generated entry as the cargo root). W4 picks the name
  and records the choice in RFC-0121.
