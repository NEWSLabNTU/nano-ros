# Phase 531 — Workspace-first project layout: bringup-only `system.toml`, one build road, the leaf shape retired

**Status (2026-11-05). OPEN — W1 (the measurement), W2 (the gates and the
sync warning), W3 (the generated launch file) and W4 (the workspace scaffold
and the node rung) done; W5 step 1 (the design: the generated entry drops its
selection package, an owned entry keeps a renamed adapter) done; W5 step 2
(its implementation, with section sizes measured identical) in review; W5
step 3 measured (finding 3 already holds on the workspace road; finding 4
open).**
Implements [RFC-0121](../design/0121-workspace-first-project-layout.md) D1 to
D8, with its amendments to RFC-0098, RFC-0116 D6, RFC-0026/0066, RFC-0100,
RFC-0118 D2 and RFC-0120.

Depends on:

- [phase-528](archived/phase-528-run-on-host-and-emulator.md) W4, for `nros run`, whose
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
| `orchestration/nros_config.rs` `synthesise_self_bringup` + the `is_self_bringup_eligible` loop in `from_cargo_metadata`; `[package.metadata.nros.deploy.*]` schema | 198 + ~25 | none | not run on either road; no example declares `[deploy.*]` | 1 — **deleted** (W7 step 1). The schema keeps a `RetiredTable` slot so discovery refuses the key by name |
| `orchestration/launch_synth.rs` `synthesise_self_model`, `enumerate_launch_files`, `is_self_bringup_eligible`; the `cmd/plan.rs` self-bringup block (lines 189–225) | 42 + ~37 | none | reached only from `nros plan <pkg-dir>` | 2 — **deleted** (W7 step 2), with `nros plan`'s `--exec` / `--file` flags, which only that block read |
| `launch_synth.rs` `discover_pkg_name`, `discover_exec_target` and their four readers | 147 | leaf-only | ran on the leaf road only, via `synthesise_leaf_launch` | **moved to 5** (measured 2026-11-07): `synthesise_leaf_launch` in `cmd/ws.rs` still reads them, so they go with the leaf road, not with step 2 |
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
| `orchestration-ir` `refuse_retired_manifest_keys`, `retired_deployment_keys` | 40 | live | refuses `[package.metadata.nros.deploy.*]` in any package | 7, with `check-leaf-deployment-spelling` R1. Step 1 removed the last reader of those keys (2026-11-07) |
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

**Status: DONE (2026-11-05, PR #3128).**

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

**Status: DONE with W4 (2026-11-05, PR #3143).** W4's scaffold needs it: its
bringup has no `launch/`. `synthesise_bringup_launch` (`cmd/ws.rs`) writes the
file from the `[[component]]` rows under a mirror root,
`<ws>/build/nros/launch/<bringup>/` holding `launch/system.launch.xml` and a
copy of `system.toml`, and the resolve is given that root as its
`--bringup-root`. So `meta.inputs` records `launch/system.launch.xml` and
`system.toml` exactly as for an authored file, and the model is portable. A
recorded input is read back through `model_location::recorded_input_path`,
which reads `system.toml` from the bringup itself, so an edit to it is still
seen. Every reader of a bringup's launch file asks
`model_location::launch_dir` (`ensure_model`, `validate_image_launch`,
`launch_node_pkgs`, `workspace_model_missing`, the model gate, the plan's
`launch_file`). A bringup with neither `launch/` nor a row gets a warning
naming both remedies and resolves no model, as before: the refusal waits for
W7 step 8, because a bringup whose image boots a hand-written entry package
(`tests/cmake-workspace-system-required-tests.sh`'s west workspace) names
neither today. No tracked bringup is in that state.

Measured on the W4 acceptance workspace: the generated model, the model
resolved with the generated file copied into `launch/`, and the model after
`launch/` was deleted again are byte-identical. `nros plan explain` is not a
verb (`nros plan <bringup>` writes `nros-plan.json`, `nros explain` renders
one), so "marks it generated" is not done here.

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

  Follow-up (the Rust tutorials' shape): a ROS message package beside a Rust
  node no longer sends the image down the cmake road. `routing::cmake_node`
  (`route` less interface packages) is the one predicate the driver choice
  and the cmake root share. An image with no `launch` now reads its node
  packages from the bringup's default launch, or from the `[[component]]`
  rows when that launch is generated.

  Follow-up (the C++ starters, issue 2339): the metadata probe takes a C++
  component's shape from the verb that declared it. One `rclcpp` fallback
  sent every scaffolded `nano_ros_add_node` class down the
  construct-with-handle arm. `check-scaffold-builds` now fails a scaffold
  whose probe fails: before, the image built, so the gate stayed green.

**Acceptance:** `nros new sensor_ws --board mps2-an385-baremetal --template
talker && cd sensor_ws && nros run sensor_node --expect 'Publishing: 0'`
passes on QEMU. A workspace with two bringups naming one node refuses `nros run
<node>` and lists both.

**Status: DONE (2026-11-05, PR #3143).**

Follow-up (Zephyr, issue 2290): a scaffolded Zephyr image names no `os_config`
fragment, so its network stack comes only from the board package. The
`nros-nsos` snippet (board `zephyr`, native_sim) and
`nros-board-mps2-an385-zephyr/prj.conf` now state `CONFIG_NETWORKING`,
`CONFIG_NET_IPV4` and `CONFIG_NET_SOCKETS`. Without them, native_sim failed to
link on `z_impl_sys_rand_get`, and mps2-an385 booted into `ConnectionFailed`.
`qemu-cortex-a53`'s package now states its own network, randomness and
stacks, and `nucleo-f767zi-zephyr` and `rpi-pico-w-zephyr` state their
`host_serial`, so the scaffold writes a serial locator for them. Issue 2290 is
resolved.

- `nros new <ws> --board <b>` (`cmd/new_workspace.rs`) writes `src/<node>/`
  (the component scaffold with `workspace_member`: no `[workspace]` table, no
  `nros.toml`) and `src/<node>_bringup/`. The names, the image id and the
  serial locator (`[board.net] host_serial`, new, stated by
  `mps2-an385-baremetal`) are recorded in RFC-0121 D4. The single-package
  project branch of `nros new` is gone; `--workspace`, `--from` and
  `--component` are unchanged.
- Inside a workspace (`new_workspace::enclosing_workspace`), `nros new <node>
  [--to <bringup>]` adds the package, and with `--to` its row and
  `<exec_depend>`.
- Follow-up (the Zephyr Rust chapter): the scaffold writes the
  `.colcon_workspace` marker every in-tree workspace tracks, and the west
  handoff passes `-DNROS_WORKSPACE_ROOT`, which the Zephyr module appends to
  the Rust lane's env, because zephyr-lang-rust passes cargo no settings file
  of ours. Without either, `nros::main!` stopped at the entry's own
  `[workspace]` and found no bringup outside a git checkout (issue 2314).
- The rungs are `builder::plan::name_target`; `run_plan::name_target` rewrites
  the target word before configure, planning and the implicit build, so
  `run`, `flash` and `monitor` all take a node. `nros run --why` names the
  rung (the `image` provenance row); `nros build --packages-up-to <node>` uses
  rung 3 only where the bare build would have refused.
- `nros new` writes a board's `[board.net] host_serial` into the image as a
  `serial/` locator, and the generated entry's facade now takes the board's
  link feature from that locator (`serial` in place of `ethernet` on the
  mps2 bare-metal crate), as a leaf's selection package already did.
- Follow-up: an emulator image that states no `locator` now gets a run-plan
  `link` from the board, on the `board` rung, so `nros run` starts its router.
  This covers the scaffolded freertos, ThreadX riscv64 and NuttX images. The
  link is `nros_board_common::board_net::default_locator`, the one rule the
  board crates, `nros board net` (and therefore the cmake entry rung and the
  ThreadX cmake modules) and the planner share; see RFC-0109's implementation
  notes. Issue 1793's leftover copies are gone: two Rust constants and two
  cmake literals. `nros run` now refuses to start a router on a port that is
  already taken, naming `--router tcp/…` and `--router none`. Zephyr boards
  get no board default yet, because Kconfig owns theirs (issue 2301).
- The acceptance's `--expect 'Publishing: 0'` cannot match the talker, which
  prints `Publishing: 'Hello World: N'` from 1. It was run with `--expect
  'Publishing:'`, as the first chapter (PR #3093) does: on QEMU (ROS 2 Humble
  sourced, `nros setup --tool qemu`), `nros run sensor_node` matched
  `Publishing: 'Hello World: 1'` 3.3 s after boot. The chapter's host track
  (`nros new host_ws --board linux --lang cpp --rmw cyclonedds`, `nros run
  host_node`) matched after 1.3 s. With a second bringup
  naming `sensor_node`, `nros run sensor_node` exits 3 listing
  `other_bringup:host` and `sensor_node_bringup:baremetal`.
- `scripts/check-template-copy-out.sh` (the scaffold arm) and
  `scripts/ci/scaffold-journey-check.sh` build and resolve the scaffolded
  workspace instead of the retired single package.

**W4 follow-ups** (found by replaying the target chapters on the installed
track):

- **A C/C++ node package names no `DEPLOY`.** The component scaffold wrote
  `nano_ros_add_node(... DEPLOY linux)`, the standalone package's carrier ELF.
  `nros build` reads a `DEPLOY` token as the board a package is for
  (`entries_for_other_boards`), so on every non-Linux board the node was left
  out and the cmake root refused: "no cmake packages in this workspace … A
  pure-Rust workspace builds through the cargo root". Reproduced with
  `--board qemu-armv7a-nuttx --lang c` and `--board mps2-an385-freertos
  --lang c`. A workspace member is now registered carrier-less (RFC-0121 D4:
  a node package names no board), and when every cmake package was left out
  the refusal names them instead of calling the workspace pure Rust.
- **One workspace detector.** `nros new <x> --kind runner|command` places the
  package through `new_workspace::enclosing_workspace`, the node rung's
  detector, so from any directory below a workspace it lands in
  `<ws>/src/<x>`.

### W5 — One build road

Every image builds through the generated entry. A node package is a library;
its `main` is generated. The hooked roads of RFC-0120 D8 (west, CMake) build
the generated entry, which records its bringup.

The generated entry has no selection package (maintainer decision,
2026-11-05; RFC-0071 D13, RFC-0121 D6 as amended). Its `Cargo.toml` names the
board crate and the backend crate(s) with the features the package carried
(`facade::derive`), and the backend registration of RFC-0071 D11 is written
into its source above `nros::main!`. An entry a person owns (materialised,
`nros new entry`, the seven fixture `*_entry` packages) keeps a generated
adapter crate, renamed `<entry>_nros_derived`, so its derivation stays live
(RFC-0065 D5). C and C++ images are unaffected: they register through the
generated `nros_app_register_backends()` and never used the Rust package.

Steps, each its own pull request:

1. **Design** (docs only): RFC-0071 D13, RFC-0098 D6 and RFC-0121 D6
   amended; this item and W7 step 3 rewritten.
2. **The generated entry states its own selection; the owned entry's
   adapter is renamed.** `builder::entry` renders the derived features on its
   dependency lines and the two registration macros into its source.
   `nros::main!` has one call shape, `__nros_register_backends!(…)` by bare
   name, and emits a `use` of the adapter's macros when the manifest names
   one. `EntrySpec::facade_dir` and the facade heal and refresh in
   `cmd/build.rs` are deleted. `facade::write_facade` writes
   `<entry>_nros_derived` under `generated/nros-derived/`, named by
   `entry_names::adapter_crate_name`. `nros materialize` turns the generated
   entry's stated features back into an adapter dependency, and `nros new
   entry` names the adapter.
3. **W1 findings 3 and 4 move to the model road.** The payload classes and
   the take buffer are derived for a bringup image (today
   `cmd/build.rs::derived_pool_env` passes `Refused`, and issue 1125 measured
   131,072 B of `.bss` from the payload-class default), and `entities =
   "census"` is read from a bringup's `[[component]]` row.
4. **One road per platform family.** Each family's single-node example builds
   and runs through the workspace road, with section sizes compared against
   the standalone build (`just mem-report`).

**Acceptance:**

- each platform family's single-node example builds and runs through the
  workspace road, with images whose section sizes match the standalone build's
  within the phase-445 tolerance (numbers recorded here);
- a generated entry names no adapter crate (no `*_nros_derived` and no
  `*_nros_selection` dependency), and nothing writes one for it;
- an owned entry's adapter has one name, from one function
  (`entry_names::adapter_crate_name`), and a materialised entry still follows
  an RMW or edition change in `system.toml` on its next build;
- `selection_crate_name` survives only for the leaf road W7 step 3 deletes.

**Status: step 1 done (PR #3147); step 2 in review (PR #3149, 2026-11-05).**
Step 2 does the following:

- `builder::entry` states `facade::entry_selection` on the entry's own lines.
  Each derived feature is marked `# nros-derived:`, and the two macros sit
  between `// nros-derived: begin` / `end` markers.
- `nros::main!` has one call shape, through `registration_source`. Its two
  definers are the entry's own source and a `use` from an adapter the
  manifest names.
- The facade heal and refresh are gone from `cmd/build.rs`.
- `write_facade` writes `<entry>_nros_derived` under `generated/nros-derived/`,
  named by `nros_orchestration_ir::entry_names::adapter_crate_name`. That
  module also holds `entry_package_name`, re-exported from `leaf_system`. A
  manifest still naming `<entry>_nros_selection` keeps that spelling and gets
  a warning.
- `nros materialize` hands the marked features and the macros back to the
  adapter (`builder::materialize::owned_manifest` / `owned_source`).
- `nros build` writes an owned entry's adapter before cargo runs. This was
  measured to be needed: `nros configure` finds no materialised entry, because
  `generate_facade_crates` reads cargo workspace members and a workspace has
  no root `Cargo.toml` (RFC-0098 D9). A materialised entry used to build only
  because the generated entry's facade had the same directory.
- `nros new entry` names the adapter.
- `check-node-package-selection-free` refuses the adapter spelling in a node
  package.

Section sizes were compared before and after, with the CLI at main against
this change (`size -A`, debug profile, `nros build <image>`). The images are
from `examples/workspaces/rust` unless noted:

| image | .text | .rodata | .data | .bss | before = after |
| --- | --- | --- | --- | --- | --- |
| `linux` (zenoh) | 958,147 | 158,932 | 95,768 | 260,090 | identical |
| `linux_cyclonedds` | 1,289,859 | 198,964 | 113,056 | 53,642 | identical |
| `linux_xrce` | 666,931 | 100,484 | 94,000 | 35,442 | identical |
| `freertos` (mps2-an385, thumbv7m) | 572,520 | — | 5,064 | 1,058,096 | identical |
| `threadx` (threadx-linux) | 540,019 | 90,315 | 3,608 | 4,462,160 | identical |
| `bridge-cyclonedds:linux` (zenoh + cyclonedds, registration from the entry's own macros) | 1,654,227 | 271,452 | 38,832 | 302,018 | identical |
| `zephyr` (native_sim/native/64, Zephyr 3.7 west; `(gated)` registration; berkeley `size`) | 962,710 text | — | 51,351 | 1,844,656 | identical |
| `linux`, materialised (`nros materialize linux`, then `nros build linux` through `linux_entry_nros_derived`) | 958,147 | 158,932 | 95,768 | 260,090 | identical to the generated entry |

NuttX cannot build on the measuring host: the NuttX kernel export fails
before any change is involved. Its comparison is left to the tier-2 matrix
run on CI, and nothing is claimed for it here.

**Step 3, measured (2026-11-05).** W1 finding 3 described the leaf road's
`[env]` sidecar: the workspace road passes `Refused` for the payload classes
and the take buffer there. That sidecar is not the workspace road's carrier.
A workspace image with a composed inventory writes a sizing descriptor
(`write_for_model`, phase-457-payload W2), and `nros-rmw-zenoh` sizes its
pools from it. The descriptor states `subscriber_count` and
`subscription_entities`; `max_wire_bound_bytes` is derived, or refused with
its reason. Derivation was measured through the `[env]` rows as well, with
the leaf road's `join` and `take_buffer::derive` over the same inventory and
the `<ws>/generated/` bound tables:

- `linux_service_server` (subscribes to nothing) now states
  `NROS_ZENOH_MAX_LARGE_SUBSCRIBERS = "0"`. Its `.bss` stays 110,514 B with
  and without that row, because the descriptor had already sized
  `LARGE_PAYLOADS` to zero;
- every image subscribing to `std_msgs/msg/String` refuses on both carriers
  (an unbounded member);
- the take buffer refuses on both, over the workspace's whole `generated/`
  closure (38 unbounded types).

So that change was dropped: a second carrier for a fact the descriptor
already delivers is what CLAUDE.md's "never re-carry a fact through an env
knob" forbids. What remains is a precision gap, not a regression. The take
buffer's closure is the workspace's `generated/` tree, not the image's own,
so an image can refuse because a type it does not link is unbounded. W1
finding 4 (`entities = "census"` from a bringup row) is open. Its 13 sites
are NuttX C/C++ leaves (12) and one `packages/testing` bin, which move in
W6 / phase-535, and NuttX cannot build on this host.

### W6 — Migrate the tree

> **Scope amended 2026-11-04 ([RFC-0122](../design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md),
> [phase-535](phase-535-example-reorg-and-cadence-ci.md)).** Under `examples/`,
> W6 migrates only the single-node witnesses (`<plat>/<lang>/talker`) and
> `custom-platform` in place. The pairs, variants, configurations and feature
> demos move into the language workspaces in phase-535 instead.
>
> **Who migrates what (agreed with phase-535, 2026-11-11).** Under the
> maintainer ruling of 2026-11-10 an example kept in its own directory is a
> one-package workspace (RFC-0122 D3), so every remaining leaf migrates; the
> split is only about who does it:
> - **phase-531 W6:** the 22 `<plat>/<lang>/talker` witnesses (the linux Rust
>   talker's `link-tls` and peer rows included), `linux/c/custom-platform`, the
>   templates, and the nros-tests bins `action-server-concurrent` and
>   `message-info-observer` (issue 2406).
> - **phase-535 W6:** the linux Rust listener, `service-*` and `action-*` leaves
>   (the per-edition ones per issue 2409, the listener's `link-tls` row and its
>   docker image), the feature demos (linux Rust logging, lifecycle-node,
>   custom-msg; C/C++ logging, parameters, safety-listener; C custom-msg; NuttX
>   and ThreadX typed-carrier; ThreadX errno-isolation), the linux Rust
>   custom-transport pair, the mps2-an385 C/C++ bare-metal leaves, and any other
>   non-talker, non-template leaf under `examples/`.
> - **W7 step-3 prep:** the other 25 nros-tests bins and the 3 nros-bench
>   programs.

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
- Before the first batch, fix [issue 2298](../issues/archived/2298-workspace-image-take-buffer-not-derived.md) (fixed 2026-11-06):
  the workspace road states neither the take buffer nor the payload classes, so
  every image a batch moves would lose the sizing it had as a cargo leaf.

**Acceptance:** the W2 allowlists are empty. `just ci` (tier 1) and `just ci
matrix` (tier 2) are green on the migrated tree, with fixture counts unchanged
from before W6.

**Status: in progress (2026-11-06).**

- Issue 2298 fixed (PR #3208, merged 2026-11-06): a workspace image derives its payload classes and take
  buffer over its own message closure, with the leaf road's functions
  (`image_closure_bound_tables`, `payload_classes_over`, `take_buffer_over`).
  The rows equal the leaf builds of the same nodes; the issue records the
  measurement.
- `nros dev migrate-bringup <dir> [--fixtures <manifest>] [--dry-run]`
  (`cmd/migrate_bringup.rs`). Unit tests on a copy of each shape: Rust,
  C, C++, Zephyr west (the Rust leaf with a nested node package, and the C
  leaf whose `CMakeLists.txt` registers the component), a hand-written
  `main`, a `main` that only names a panic policy, and fixture rows.
  - The node package gets the code and manifest, with the selection-crate
    dependency, the `[[bin]]` of a deleted `main` and the `[workspace]`
    table removed and relative `path =` dependencies re-rooted. The bringup
    gets `system.toml`, every file it names, `launch/`, `boards/`, the
    contract and board-net files, and on Zephyr every Kconfig fragment.
  - The crate takes its `package.xml` name and the `[[component]] class`
    paths follow: the generated entry depends on a node package by that
    name, so `linux/rust/talker` (`native-rs-talker` against
    `native_talker`) failed `no matching package` until it did. Measured:
    the migrated `linux/rust/talker` then built `native_talker_bringup:linux`
    through the workspace road (trial only; the tree is unchanged here).
  - A `main.rs` that only calls `nros::main!` is deleted; a `panic` argument
    moves into `[image_defaults] panic`. Any other `main` is moved and
    reported (RFC-0122 D10).
  - A fixture row becomes a `[[workspace_fixture]]` row on the image that
    builds its RMW, keeping `platform`, `lang` and `rmw`. A row carrying a
    key a workspace row does not read (`features`, `cmake_defs`, `builder`,
    `target`, the `west_*` keys) is left alone and reported.
- Two gaps the first Zephyr trial migration found, fixed in the tool:
  - It wrote no `.colcon_workspace` marker at the new workspace root, so the
    generated entry's `nros::main!` walked up to `.git/` and reported
    `pkg not found ... Known pkgs: []`. The root now gets the marker
    (an existing one stays where it is).
  - It moved a west application's base `prj.conf` to the bringup root and
    suggested naming it in `os_config`. That does nothing: the generated
    application has a placeholder `prj.conf` of its own, which
    `resolve_fragment` finds first. The file now goes to
    `<bringup>/boards/<west board>/prj.conf`, the image's
    APPLICATION_CONFIG_DIR, once per west board the images build for (the
    board package through the catalog, `west_build_board`). A board that
    does not resolve keeps the old move and is reported.
- `fixtures-manifest.py validate-workspaces` accepts a bringup with no
  `launch/` and at least one `[[component]]` row: its launch file is
  generated (RFC-0121 D3).
- Run over a copy of all 23 W6 sites and `examples/fixtures.toml`. Every site
  migrated, and 7 of its fixture rows were rewritten; the 7 validate. The
  reports are the batches' work list:
  - 14 sites have a hand-written `main` to turn into a component: every C
    and C++ talker outside Zephyr (the baremetal pair through a Rust carrier
    `main.rs` as well), and `custom-platform`
    (RFC-0122 D10 keeps a manual entry there; how is a batch decision);
  - the C/C++ leaves build xrce and cyclonedds through `-DNROS_RMW` and
    declare one image, so those rows need an image per RMW;
  - the rows carrying platform knobs (`cmake_defs`, `builder`, `target`,
    `skip_probe`, the `west_*` keys) and the Rust talker's `link-tls`
    feature row.
- The verb merged as PR #3230 (2026-11-07).
- The shared test-side fallback for a migrated leaf (PR #3250):
  `nros_tests::fixtures::migrated_leaf` and `migrated_leaf_binary`.
  - `build_example`, `build_example_rmw` and `build_cmake_leaf_rmw` take it
    first.
  - A leaf with no `[[fixture]]` row left resolves through its
    `[[workspace_fixture]]` rows, selected by RMW and a variant (plain, an
    `env`, or an image).
- A leaf row's cargo `features` has a workspace home now: RFC-0121 D9,
  `[image.<id>] node_features` (maintainer decision 2026-11-07, over a TLS
  knob implied by a `tls/` endpoint).
  - The generated entry states the features on the node package's
    dependency line, so every road that builds a Rust entry carries them; a
    C/C++ entry refuses the key by name.
  - A `[[workspace_fixture]]` row restates the list as `node_features`, and
    the manifest reader checks it against the image.
  - Measured on a copy of the migrated `linux/rust/talker` with
    `[image.linux_tls] node_features = ["link-tls"]`: `nros build linux_tls`
    built, its entry manifest reads `native_talker = { …, features =
    ["link-tls"] }` (the `linux` entry's line has none), and the binary
    links `_z_f_link_*_tls`.
  - It unblocks the three configured rows W6 moves: `link-tls` on the Rust
    talker, `zenoh-reply-slots` on `bins/action-server-concurrent`, and
    `unstable-zenoh-api` on `bins/message-info-observer`.
- **Batch 8, Zephyr (merged, #3406).** `examples/zephyr/{c,cpp,rust}/talker`
  and `packages/testing/nros-tests/bins/idle-wake-zephyr-mps2` are single-node
  workspaces; their 17 `[[fixture]]` rows (c 6, cpp 4, rust 5, idle-wake 2)
  are `[[workspace_fixture]]` rows on a bringup image, each keeping its
  coordinate, `west_role`, build-dir name and leaf id. Measured: `coords`
  515 lines before and after with an identical coordinate multiset, and
  `west-leaves` 98 records before and after whose identity fields (board,
  lang, role, build dir, id, isolation, coordinate) are unchanged; only the
  conf list and image columns moved to `ws_dir` / `nros_image`.
  - The mps2/an385 rows build their own images (`mps2_zenoh`,
    `mps2_boot_report`, `mps2_serial`; the idle-wake images name the board
    package). A workspace image merges its board package's Kconfig BEFORE its
    `os_config`, where the standalone leaf merged the board file last. The
    talkers' `prj-zenoh.conf` disagreed with the mps2 board on 5 symbols
    (`HEAP_MEM_POOL_SIZE` and the four NET_PKT/NET_BUF counts;
    `MAIN_STACK_SIZE` is the module's per-image default since issue 2365),
    so on this road it would have overridden the board's measured
    budget. Those values are native_sim's, so they moved to
    `prj-zenoh-native-sim.conf`, which only the native_sim image names; the
    board's budget wins on mps2 as it did before. Listing the board file last
    was the other option and was not taken: it is a file of another package,
    reachable only by a repo-relative path, and it would restate the board's
    own merge.
  - `check-kconfig-overridden-values` reached only the `[[fixture]]` rows'
    `conf_files`, so the move took four overrides out of its sight (the
    boot-report overlay's `NET_LOG`/`LOG`/`EARLY_CONSOLE`, the serial
    image's `NETWORKING`). It now also composes every west
    `[[workspace_fixture]]` row's fragments in `nros build`'s merge order.
    Its baseline went from 0 to 6: the four are back, and two pre-existing
    workspace overrides (the features C++ image's `RUST=n`, the Nucleo
    board's `NETWORKING=n`) are recorded for the first time.
  - The C++ leaf's vendored `msg/String.msg` (unused since phase-277; the
    code publishes `std_msgs/String`) made its node package an interface
    package to the generated entry, which then wanted a Cargo crate there.
    Deleted.
  - **Parked: the boot-report row** ([issue 2362](../issues/2362-boot-report-cell-refused-by-arena-oracle-on-workspace-road.md)).
    On this road the image's entities are declared, so phase-412's
    compile-time arena oracle refuses the 32 B arena the cell needs. The
    row builds with a derived arena and its test SKIPS naming the issue.
    **W6 acceptance (empty W2 allowlists) is blocked on that decision.**
  - The Nucleo-F767ZI row is migrated but not verified on this host: the
    Zephyr workspace has no `hal_stm32`, which is deliberately outside the
    west allowlist.
  - Shared edits: `fixtures-manifest.py` reads Nucleo-F767ZI workspace rows
    as west rows (`ZEPHYR_WEST_PLATFORMS`) and `west-overrides` reads
    workspace rows; `check-zephyr-fixture-rows.py` reads a workspace row's
    non-entry `west_role`; `zephyr/module.yml` drops the three talker
    samples; `just zephyr build-one` and `check-copy-out.sh` build a
    workspace witness through `nros build`.
- Two linux sites waited on a decision (2026-11-10). Neither is a gap in the
  migration tool; each is a road the workspace does not have yet. The
  maintainer ruled on both on 2026-11-11; the order is the 2409 PR, then the
  2406 design and code, then the migrations.
  - `examples/linux/rust/talker` migrates and builds, `linux_tls` included
    (the entry's dependency line carries `link-tls` and the binary links
    `_z_f_link_*_tls`). Its six edition images do not: a workspace generates
    its messages for one edition, so `linux_jazzy` builds with `ros-jazzy` on
    and humble type hashes, and `nros build` rewrites a `nros configure
    --ros-edition jazzy` back before compiling.
    [Issue 2409](../issues/archived/2409-workspace-image-ros-edition-builds-workspace-edition-messages.md).
    Ruling: refuse an image whose edition differs from the workspace's
    generated messages, and move the edition images into their own
    per-edition workspace rows. **Done:** the refusal is in `plan_for`, the
    talker is a one-node workspace (`src/native_talker`,
    `src/native_talker_bringup`, images `linux`, `linux_tls`, `linux_xrce`,
    `linux_cyclonedds`), its five rows are `[[workspace_fixture]]` rows, and
    `just ros_editions build-e2e-fixtures` builds the talker from a
    per-edition copy of the workspace. `migrate-bringup` wrote the rows no
    `target_dir`; the peer row (the `linux` image with an env) needs its own,
    or it overwrites the plain row's `linux_entry`.
  - The two bins named above, and the linux C and C++ talkers, keep a
    hand-written `main` and are built for several images. An owned entry is
    one image's program, the cargo road finds it only at
    `src/<image>_entry`, and `node_features` reaches node packages, not an
    owned entry's own features. The C and C++ `main`s are also the subject of
    issue 1556's census tests, so turning them into components moves what
    those tests cover.
    [Issue 2406](../issues/archived/2406-owned-entry-cannot-serve-several-images.md).
    Ruling: one owned entry may serve several images (RFC-0071 D13 and
    `claiming_image` amended, the cargo road honours `[image.<id>] entry`),
    and the census tests keep running the same `main()`s.
  - **Issue 2406, core support landed (RFC-0071 D19).** The cargo road
    builds the package `[image.<id>] entry` names; an entry several images
    name has one adapter with an `image-<id>` feature per image, turned on by
    `nros build`, so neither the adapter nor the entry's lock moves between
    images; `node_features` reaches the entry's own features. Proved on
    `bins/message-info-observer` (images `linux`, `linux_zero_copy`).
    Merged as #3496.
  - **Issue 2406 resolved.** `bins/action-server-concurrent` and the linux C
    and C++ talkers are migrated as well: owned entries with one image per
    RMW, the C/C++ ones on the CMake road, which takes the same claim rule
    (RFC-0071 D19, "The CMake road"). Issue 1556's census tests still run
    the same `main()`s.
- **Batch, the link-checked Rust talkers.** `examples/{threadx-linux,mps2-an385-freertos,qemu-armv7a-nuttx}/rust/talker`
  are one-package workspaces; their `[[fixture]]` rows are
  `[[workspace_fixture]]` rows with the same coordinates (`coords`: identical
  `(platform, lang, rmw)` multiset before and after), each with its own
  `target-fixtures` dir. Every `main.rs` only called `nros::main!`, so each
  image boots a generated entry (`threadx_linux_entry`,
  `mps2_an385_freertos_entry`, `qemu_armv7a_nuttx_entry`).
  - `rust-rtos-link-check` builds the three through one helper,
    `link_check_build`: a leaf as before, a witness workspace with
    `nros build --board <board> -- <profile> --target-dir <dir>`; L3 reads the
    generated entries' ELFs.
  - Test side: `migrated_leaf_binary` resolves a Rust row at the profile the
    workspace builder used (NuttX's carve-out, else ambient), and a row whose
    `env` only pins `RUSTUP_TOOLCHAIN` (the NuttX row, for `build-std`) is
    still the plain build.
  - Measured on rebuilt fixtures: `entry_build` (nuttx, threadx-linux),
    `freertos_run_plan_runtime`, the shape/coverage suites pass; the
    threadx-linux and NuttX (QEMU) images publish to a router.
  - The C/C++ talkers on cross boards wait on
    [issue 2433](../issues/2433-owned-cross-entry-has-no-census.md): a cross
    C/C++ leaf is census-sized (RFC-0120 D4), and an owned entry on the
    workspace road has no model for a census to count, nor (until a CLI fix
    the issue records) its image's deploy, domain and locator.

### W7 — Retirement

Delete what W1 listed as reachable only from the standalone shape, in this
order, each step with its own PR and gate run:

1. the `[deploy.*]` manifest metadata and `synthesise_self_bringup`;
2. `synthesise_self_model` and the launchless-package detection in
   `launch_synth.rs`;
3. the leaf's `<pkg>_nros_selection` crate: `LEAF_SELECTION_DIR`,
   `facade::write_leaf_selection`, `leaf_settings::write_selection` and its
   helpers (`write_entry_selection` among them),
   `cmd/build.rs::check_leaf_selection`, `leaf_system::selection_crate_name`,
   and the macro's `leaf_selection_crate` / `reroot_through` (the board-path
   re-root). The generated entry left the package in W5. The seven fixture
   `*_entry` packages reach their board through that re-root today, so they
   move onto the owned-entry adapter here (they name their board crate and
   depend on `<entry>_nros_derived`) and stay as owned-entry coverage. The
   adapter itself (`write_facade`, `adapter_crate_name`) is not the leaf's and
   stays (RFC-0071 D13);

   **Precondition (maintainer ruling 2026-11-10).** Step 3 runs only once no
   leaf consumes the selection crate, which takes three things first:
   - [ ] phase-535's moves: the pairs, variants, configurations and feature
     demos are `[image.*]` rows in the language workspaces;
   - [ ] W6's witnesses (`<plat>/<lang>/talker`, `custom-platform`) are
     one-package workspaces;
   - [ ] step-3 prep: 25 of the `packages/testing/nros-tests/bins` and the 3
     `nros-bench` programs are one-package workspaces, migrated in slices, each
     slice ticking its count here. The other two bins,
     `action-server-concurrent` and `message-info-observer`, migrate with
     issue 2406 (a multi-image owned entry). — 3/28 done (#3485, #3501)

   A leaf that is still one after that is a true leaf. It moves in place onto
   the owned-entry adapter (`<entry>_nros_derived`), as the fixture entries
   above do; the selection crate is not kept for it. RFC-0122's "its own
   directory" means a one-package workspace (RFC-0122 D3, as clarified).
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
   that flow in W8. The adapter crate stays: a materialised entry
   uses it too (RFC-0071 D13);
9. W2's warning becomes a refusal.

Each deletion is listed in `book/src/migrating/retired-names.md` with its
replacement.

**Acceptance:** W1's table has every row marked deleted or kept-with-reason;
`just ci gate` and `just ci matrix` green after the last step.

**Status (2026-11-07): steps 1 and 2 done.**

- **Step 1 (done).** Re-measured before deleting: `git grep
  'metadata\.nros\.deploy' -- '*Cargo.toml'` finds no tracked manifest, and
  the only consumers were `synthesise_self_bringup` and its tests. Deleted:
  `synthesise_self_bringup`, the eligibility loop in `from_cargo_metadata`,
  `PackageMetadataNros::is_self_bringup_eligible`, `DeployTargetMetadata`,
  `NrosConfigError::SelfBringupSessionDisagrees`, and `BringupSource` (its
  only other variant is every bringup, so the field and its one filter in
  `scaffold_deploy` went too). `[package.metadata.nros.deploy.<t>]` is now
  refused by name at discovery (`RETIRED_MANIFEST_DEPLOY`), naming the
  bringup `[image.<id>]` that replaces it. The two codegen-system alias tests
  that staged their bringup through `[deploy.*]` now stage a `system.toml`
  plus a model, so the alias resolver keeps its coverage; the unknown-RMW
  load test moved onto an on-disk bringup. Doc comments that still called
  the table live were corrected in the same sweep (`image.rs`,
  `main_macro.rs`, `nros-platform` `board/entry.rs`, `nros-orchestration-ir`
  `lib.rs`, `cmd/config.rs`).

- **Step 2 (done).** Re-measured: `synthesise_self_model`,
  `launch_synth::is_self_bringup_eligible` and `enumerate_launch_files` had
  one caller, the self-bringup block in `cmd/plan.rs`, and no tracked script,
  cmake module or recipe runs `nros plan` on a package directory or passes
  `--exec` / `--file` (the one in-tree caller,
  `cmake/nano_ros_workspace_metadata.cmake`, passes a bringup's launch
  file). Deleted: that
  block, the three functions, `nros plan`'s `--exec` and `--file` (read by
  nothing else), the three `LaunchResolveError` variants only launch
  resolution raised, and `nros-tests/tests/self_bringup.rs` with its
  `matrix.rs` and lane-admission rows. W1 put `discover_pkg_name` /
  `discover_exec_target` in this step; they are still read by
  `synthesise_leaf_launch`, so they go in step 5. `nros codegen-system`
  carries the same two dead flags (`--exec`, `--file`, read by nothing since
  the launch-XML path went in phase-296); they are left for step 5's sweep of
  the leaf-only arguments.

### W8 — Book rewrite

The book teaches one shape, the workspace, from the first page.

Page paths below are today's. [Phase-532](phase-532-two-books-by-audience.md)
moves pages into part directories by `book/page-map.toml` (RFC-0123). The four
workspace pages this item merges are already mapped to one target,
`build/growing-your-workspace.md`. A branch rebases over a move with
`scripts/book-move.py --rewrite`.

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
  phase-532 (PR #3093, RFC-0123) rewrites it to this phase's workspace shape
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
- `book-internals/src/pipeline/canonical-build-path.md`: the leaf road leaves the map.

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
