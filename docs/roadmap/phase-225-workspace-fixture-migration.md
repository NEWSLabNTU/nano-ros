# Phase 225 — Workspace Fixture Migration to Product Examples

**Goal.** Move product-shaped multi-node workspace fixtures out of
`packages/testing/nros-tests/fixtures/` and into `examples/` so the
examples tree demonstrates the real target workflow: Node packages for
node logic, Bringup packages for launch/config only, and one Entry
package per target platform.

**Status.** IN PROGRESS. Created 2026-06-06 after review of the book's
multi-node workflow pages:
`workspace-from-app-node.md`, `workspace-node-pkgs.md`,
`workspace-bringup.md`, `workspace-entry-pkg.md`,
`workspace-cpp.md`, and `workspace-mixed-language.md`. First
implementation pass landed the promoted native Rust, C, C++, and mixed
workspaces, plus product-path fixes discovered by building them through
the documented workflow. Follow-up review found a testing workflow gap:
workspace tests still build in the test stage instead of following the
single-node fixture convention where `build-fixtures` prepares binaries
and E2E tests only run prebuilt fixtures.

**Priority.** P1. These examples are the user-facing proof of the
Phase 212/219/222 workflow. The current split leaves realistic
workspaces hidden in tests, while some visible templates still use
placeholder node content.

**Depends on.**

- Phase 212 Node/Bringup/Entry role model.
- Phase 219 C/C++ Entry package support.
- Phase 222 CLI surface decision: `nros` provisions, checks, and
  generates; platform tools build and run.

---

## 1. Target Workflow

The examples must model the workflow documented in the book, not the
internal test matrix.

### 1.1 Package Roles

Every promoted workspace uses the same roles:

- **Node pkg**: reusable node component code only. No `main()`, no board
  selection, no deploy config. Contains real publishers, subscriptions,
  timers, services, or actions.
- **Bringup pkg**: config only. Contains `package.xml`, `system.toml`,
  `launch/*.launch.xml`, and optional `config/`. No `Cargo.toml`, no
  `CMakeLists.txt`, no `src/`.
- **Entry pkg**: one runnable `main()` per target platform. Owns board
  selection and links the Node packages named by the Bringup launch file.

### 1.2 Build Boundary

The command flow is:

```sh
source ./activate.sh
nros setup <platform> --rmw <rmw>
eval "$(nros ws env)"
nros ws sync
nros codegen-system --bringup src/<system>_bringup --target <target>

# Then build with the native tool:
cargo build ...
cmake -S . -B build && cmake --build build
west build ...
idf.py build ...
```

`nros` does environment setup, dependency provisioning, message codegen,
system codegen, planning, and checking. It does not replace `cargo`,
`cmake`, `west`, `idf.py`, `probe-rs`, or board-specific build tools.

These workspaces are verification targets for the designed user
workflow. If a workspace cannot be built by following the documented
workflow, that is a product issue to fix in the CLI, codegen, CMake
glue, package metadata, or docs. Do not paper over it with fixture-only
shortcuts or test-only build paths.

---

## 2. Target Example Set

Do not create one example per platform. Create a small set of
workspace-shaped examples, each with multiple Entry packages where the
workflow needs platform variation.

Proposed layout:

```text
examples/workspaces/
├── rust/
│   ├── Cargo.toml
│   └── src/
│       ├── talker_pkg/
│       ├── listener_pkg/
│       ├── demo_bringup/
│       ├── native_entry/
│       └── qemu_freertos_entry/        # first embedded Rust entry
├── c/
│   ├── CMakeLists.txt
│   └── src/
│       ├── c_talker_pkg/
│       ├── c_listener_pkg/
│       ├── demo_bringup/
│       └── native_entry/
├── cpp/
│   ├── CMakeLists.txt
│   └── src/
│       ├── talker_pkg/
│       ├── listener_pkg/
│       ├── demo_bringup/
│       └── native_entry/
└── mixed/
    ├── CMakeLists.txt
    └── src/
        ├── c_talker_pkg/
        ├── cpp_listener_pkg/
        ├── rust_filter_pkg/            # if Corrosion path is ready
        ├── demo_bringup/
        └── native_entry/
```

Additional platform Entry packages are added only when they teach a
different build integration:

- Zephyr: `west build` + `nros codegen-system` integration.
- ESP-IDF / PlatformIO: vendor tool drives the build.
- NuttX / ThreadX / FreeRTOS: only one representative at first unless
  the Entry package shape truly differs.

The node and bringup packages should be shared across those entries
inside the same workspace.

---

## 3. Tracks

### 225.A — Inventory and Classification

- [x] List every directory under `packages/testing/nros-tests/fixtures/`.
- [x] Classify each as one of:
  - product-shaped workspace example;
  - parser/planner fixture;
  - diagnostic/error fixture;
  - platform smoke fixture;
  - obsolete fixture candidate.
- [x] Keep diagnostic and edge-case fixtures in `nros-tests`.
- [x] Promote only product-shaped workspaces to `examples/workspaces/`.

Acceptance:

- A table in this phase doc maps each moved fixture to its new
  `examples/workspaces/` home.
- No parser/diagnostic-only fixture is exposed as a user example.

### 225.B — Canonical Rust Workspace Example

- [x] Replace or migrate `examples/templates/multi-node-workspace/` into
  `examples/workspaces/rust/`.
- [x] Keep exactly the book shape: Node pkgs, Bringup pkg, Entry pkgs.
- [x] Add at least two Entry packages sharing the same Node + Bringup
  packages. The Rust workspace now has `native_entry` and
  `native_default_entry`; embedded target Entries remain follow-up
  platform coverage.
- [x] Update tests that validate real workspace flow to stage this
  example path instead of a hidden `nros-tests` fixture.

Acceptance:

- `nros ws sync` succeeds from the workspace root.
- `nros check --bringup src/demo_bringup` succeeds.
- `nros check --workspace .` succeeds.
- `cargo build` succeeds for both native Rust entries.
- The commands above are run exactly as the user-facing workflow, not
  through test-only staging shortcuts. Any failure becomes a tracked bug.

### 225.C — C and C++ Workspace Examples

- [x] Migrate the current pure-C and pure-C++ templates into
  `examples/workspaces/c/` and `examples/workspaces/cpp/`.
- [x] Keep CMake as the build driver.
- [x] Entry packages must use `nano_ros_entry(...)` / `NROS_MAIN(...)`.
- [x] Node packages must use the same conceptual API as the Rust Node
  packages: register node entities, expose a component registration
  symbol, and contain no `main()`.

Acceptance:

- `cmake -S . -B build -DNANO_ROS_ROOT=<repo>` configures.
- `cmake --build build` builds the native Entry binary.
- The C/C++ examples use the same Bringup package shape as Rust.
- Configure/build failures under the documented workflow are fixed in
  the product path, not hidden by example-specific workarounds.

### 225.D — Mixed-Language Workspace Example

- [x] Create one mixed workspace, not one mixed workspace per platform.
- [x] Include at least C and C++ Node packages.
- [x] Add Rust Node package only if the Corrosion path is stable enough
  to be a user-facing example rather than a test-only experiment.
- [x] Keep one Bringup package and one native Entry package initially.

Acceptance:

- Mixed Node packages are linked into one Entry binary.
- Tests skip cleanly when optional Corrosion prerequisites are absent.
- The README explains exactly which language owns which package role.
- The non-skipped path builds through the documented user workflow.

### 225.E — Replace Placeholder Node Content

Current visible Rust template nodes use `PlaceholderInt32` instead of
generated `std_msgs/Int32`. That is acceptable for an internal compile
fixture, but not for the primary workspace example.

- [x] Remove placeholder message types from promoted product examples.
- [x] Add real message dependencies to `package.xml` and generated
  bindings through `nros ws sync` / `nros generate-rust`.
- [x] Ensure each Node package has meaningful behavior:
  - talker publishes a real typed message;
  - listener deserializes/observes it;
  - optional service/action examples use real request/response paths.
- [ ] Leave placeholder-only code only in explicitly internal test
  fixtures, with comments saying why runtime behavior is not under test.

Audit note: promoted product examples no longer contain
`PlaceholderInt32` or equivalent stand-ins. Remaining placeholder/stub
content is limited to non-promoted surfaces:
`examples/templates/multi-node-workspace/` still carries
`PlaceholderInt32` as a copy-out skeleton that is superseded by
`examples/workspaces/rust/`, and several platform smoke fixtures under
`packages/testing/nros-tests/fixtures/` use stub nodes to test
build/link/planning behavior rather than runtime pub/sub. The checklist
item stays open until the old template is either archived/marked
internal or converted to generated `std_msgs/Int32`.

Acceptance:

- `rg -n "PlaceholderInt32|placeholder message|stand-in" examples/workspaces`
  returns zero matches.
- The primary Rust workspace builds after a clean `nros ws sync`.

### 225.F — Align Node Component API with ROS Composable Nodes

The current nano-ros term is "Node pkg", but the API should feel as
close as practical to ROS 2 composable nodes. This track is design-first:
do not rename APIs blindly until the mapping is explicit.

Questions to answer:

- Does `nros::node!(T)` map cleanly to the ROS concept of registering a
  loadable node class?
- Should the user-facing metadata say `class`, `plugin`, or another
  ROS-familiar term?
- Can C++ `NROS_NODE(...)` / `nano_ros_node_register(...)` be shaped
  closer to `rclcpp_components_register_node(...)` while preserving the
  embedded static-link model?
- Should Node construction accept a `NodeOptions` shape that better
  mirrors ROS 2 `rclcpp::NodeOptions`?
- Which ROS composable-node concepts are intentionally absent because
  nano-ros composes at link time, not by dynamic loading?

Work items:

- [x] Write a short API comparison table:
  `rclcpp_components` concept -> nano-ros Rust/C/C++ equivalent.
- [x] Identify naming mismatches that can be fixed without breaking ABI.
- [x] Identify breaking API changes that must wait for a minor release.
- [ ] Update the promoted examples to use the best current API names and
  avoid legacy "component" wording except where compatibility requires it.

API comparison:

| ROS 2 composable-node concept | Rust nano-ros today | C/C++ nano-ros today | Compatibility decision |
|---|---|---|---|
| Component class registered with `RCLCPP_COMPONENTS_REGISTER_NODE(T)` / `rclcpp_components_register_node()` | `impl Node for T` plus `nros::node!(T)` exports static registration symbols | Node package exports a register symbol through `NROS_NODE(...)`, `nano_ros_node_register(...)`, or CMake-side entry glue | Keep static-link registration. Add ROS-familiar docs/aliases where cheap; do not imply dynamic plugin loading. |
| Plugin/class name in launch metadata | `system.toml` `[[component]].class` names a Rust type path | Launch/CMake metadata names package/class/entry symbols | Keep `class` in metadata because it matches ROS vocabulary. Prefer "Node pkg" in prose and reserve "component" for compatibility internals. |
| Composable container process | Entry pkg links all selected Node packages and owns `main()` | C/C++ Entry binary links static Node package libraries and generated entry glue | Keep Entry pkg as the container analogue. Dynamic load/unload is intentionally absent for embedded determinism. |
| `rclcpp::NodeOptions` | `NodeOptions::new("name")` exists but is narrower | C/C++ node creation APIs expose only the current static options surface | Non-breaking follow-up: add builder-style options and docs mapping names, namespace, remaps, params, and allocator/runtime limits. |
| Runtime composition and dynamic plugin discovery | Not supported; packages are resolved at build/codegen time | Not supported; symbols are kept/link-checked by the platform build | Document as an intentional difference. Breaking rename to hide old "component" identifiers waits for a minor release. |
| Lifecycle/component manager services | Not part of current workspace examples | Not part of current workspace examples | Defer. Add only when lifecycle support exists; do not fake it in examples. |

Compatibility plan:

- Near-term, non-breaking: keep current Rust/C/C++ APIs, keep metadata
  `class`, and update docs/examples to call user packages "Node pkgs"
  while explaining that exported symbols still use legacy
  `component` names for ABI compatibility.
- Near-term, non-breaking: add aliases only where they reduce user
  friction without duplicating semantics, for example CMake or macro
  names that read as `node` while forwarding to current component
  registration.
- Minor-release candidate: rename public macros/symbol helpers that
  currently expose "component" as the primary user-facing name. Keep
  compatibility aliases for at least one release cycle.
- Deferred: dynamic plugin loading, component-manager services, and
  runtime load/unload. nano-ros composition remains static-link-first.

Acceptance:

- The phase doc records a reviewed decision: keep current API, add
  aliases, or schedule a breaking rename.
- Book examples and promoted workspace examples use consistent role
  names: Node pkg, Bringup pkg, Entry pkg.

### 225.G — Workspace Fixture Manifest Refactor

Workspace fixtures must join the same source-of-truth manifest as
single-node fixtures, but they need a distinct schema because their
build unit is a workspace root, not a single package directory.

- [x] Extend `examples/fixtures.toml` with `[[workspace_fixture]]`
  rows for the promoted native Rust, C, C++, and mixed workspaces.
- [x] Include explicit workflow inputs in each row: workspace root,
  Bringup package path, Entry package/target, RMW, codegen output, and
  deterministic build or target directory.
- [x] Extend `scripts/build/fixtures-manifest.py` with a
  `list-workspaces` command so build/test helpers can consume the new
  rows mechanically.
- [x] Add manifest validation for workspace rows:
  - `dir` exists and contains a workspace root file;
  - `bringup/package.xml`, `bringup/system.toml`, and the default launch
    file exist;
  - `entry/package.xml` exists;
  - CMake rows name a valid CMake target or Entry package;
  - Rust rows name a Cargo package present in the workspace.
- [x] Decide whether workspace stale-checking reuses the existing
  fixture input signature machinery or gets a workspace-specific
  signature helper. Decision: use a workspace-specific signature helper
  because workspace rows span Node, Bringup, Entry, codegen, and CMake
  inputs rather than one package directory.

Acceptance:

- `python3 scripts/build/fixtures-manifest.py list-workspaces --platform native`
  emits all promoted workspace rows.
- Existing `fixtures-manifest.py list ...` output for single-node
  fixtures is unchanged.
- Invalid workspace manifest rows fail fast in a focused validation test
  or script before the fixture build starts.

### 225.H — Workspace Build-Fixtures Refactor

Workspace examples must be built in the build-fixtures stage, not inside
Rust integration tests. The recipe should execute the same commands a
user would run.

- [x] Add `scripts/build/workspace-fixtures-build.sh` or equivalent
  shared helper that consumes `[[workspace_fixture]]` rows.
- [x] Add `just native build-workspace-fixtures`.
- [x] Wire `build-workspace-fixtures` into `just native build-fixtures`
  and root `build-test-fixtures-leaves`.
- [x] For every workspace row, run the target workflow exactly:
  `nros ws sync`, `nros codegen-system --bringup <bringup> --out <out>`,
  then the native build tool.
- [x] Rust rows build with `cargo build -p <entry>` and a deterministic
  target dir from the manifest.
- [x] C/C++/mixed rows configure and build with CMake using the manifest
  build dir, target, RMW, and normal in-tree CLI resolver.
- [x] Write a per-workspace build signature or stamp that lets
  `test-all` detect missing or stale workspace fixtures with one clear
  precondition message.

Implementation note: `workspace-fixtures-build.sh` writes one
`.nros-workspace-fixture.<id>.inputsig` per manifest row after the
workflow build succeeds. `scripts/check-fixtures-stale.sh` compares those
signatures during preflight; `nros-tests` also checks the matching stamp
before returning a prebuilt workspace binary path.

Acceptance:

- `just native build-workspace-fixtures` builds all native workspace
  Entry binaries from a clean checkout after bootstrap/setup.
- `just native build-fixtures` includes workspace fixture builds.
- `just build-test-fixtures` includes workspace fixture builds and still
  writes `target/nextest/.fixtures-built`.
- Workspace fixture output paths are deterministic and gitignored.
- A failed `nros ws sync`, `nros codegen-system`, Cargo build, or CMake
  build is treated as a product workflow bug, not papered over by
  fixture-specific shortcuts.

### 225.I — Test-Stage Workspace E2E Refactor

The test stage should run prebuilt workspace Entry binaries directly,
matching the single-node app convention.

- [x] Add `nros-tests` helpers for resolving promoted workspace Entry
  binaries from the manifest.
- [x] Convert current workspace build tests so they no longer invoke
  Cargo or CMake.
- [x] Add runtime E2E coverage for the native Rust workspace Entry
  binary.
- [x] Add runtime E2E coverage for at least one native CMake workspace
  Entry binary, then expand to C, C++, and mixed as observability allows.
- [x] Add deterministic app observability if needed: bounded run mode,
  received-message counter, success log line, or an exit-on-success path
  owned by the example code rather than by test-only shims.
- [x] Keep configure/build metadata checks only where they verify CLI or
  CMake diagnostics, not as the primary product workflow test.

Implementation note: Rust hosted Entry packages now support an opt-in
bounded spin via `NROS_ENTRY_SPIN_MS`, with
`NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS` asserting message-bearing callback
dispatch before clean exit. The Rust workspace E2E uses this to verify a
real generated `std_msgs/Int32` pub/sub callback from the prebuilt
`native_entry` binary, with the existing prebuilt native Rust talker as
the external publisher because Zenoh does not loop one session's own
publish back to its same-process subscriber. The first CMake runtime
test starts the prebuilt C++ Entry binary directly and verifies it
enters the native spin loop;
full C/C++ pub/sub assertions remain blocked until the native C/C++
NodeContext runtime moves beyond its current recording-only adapter.

Acceptance:

- Focused workspace E2E tests fail with a clear "build workspace
  fixtures first" hint when prebuilt Entry binaries are absent.
- Workspace E2E tests start the prebuilt Entry binary directly and do
  not run `cargo build`, `cmake`, or `nros codegen-system`.
- At least one workspace E2E asserts a real pub/sub observation through
  generated `std_msgs/Int32`, not just process startup.
- The old hidden fixture path is not the only tested source of truth for
  product-shaped workspaces.

### 225.J — Product Workflow Gap Closure

The fixture refactor must keep pressure on the target workflow rather
than drifting into a test-only build path.

- [x] Ensure the workspace fixture helper sources the activated in-tree
  CLI or passes the same CLI path used by existing CMake fixture builds.
- [x] Clarify the non-Rust behavior of `nros ws sync`: it is still part
  of the workflow, even when no Rust patch table is written.
- [x] Decide whether `nros codegen-system` output is an actual build
  input for native Rust/CMake entries or a workflow validation artifact;
  update examples/docs accordingly.
- [x] Add at least one multiple-Entry workspace fixture once a second
  Entry package is present.
- [x] Keep CMake-generated interface binding side effects separate from
  `nros codegen-system` validation in the fixture logs.

Workflow decision: `nros ws sync` remains a required workspace command
for all languages. For pure C/C++/mixed workspaces it may be a no-op for
Rust patch-table content, but it still validates/discovers workspace
state through the same CLI workflow. The native fixture
`nros codegen-system --out ...` output is currently a workflow
validation artifact for native Rust/CMake Entries; embedded/platform
Entries may consume the baked output directly. CMake-generated interface
bindings remain produced by the CMake configure/build path, not by this
validation artifact.

Acceptance:

- The build-fixtures log for each workspace shows the documented command
  sequence before the platform-native build.
- Any divergence from the book workflow is recorded in this phase doc
  with an owner and a reason.
- Book docs, README command snippets, fixture recipes, and tests agree
  on the same workspace workflow.

### 225.K — Existing Test Wiring

- [x] Add a helper in `nros-tests` for resolving
  `examples/workspaces/<name>`.
- [x] Update tests that validate product workflow to stage from
  `examples/workspaces/`.
- [x] Keep tests that validate invalid input under
  `packages/testing/nros-tests/fixtures/`.
- [x] Update canonical-shape walkers so `examples/workspaces/` is
  validated as a workspace root, not skipped as a template carve-out.

Acceptance:

- `cargo test -p nros-tests --no-run` passes.
- Focused workspace tests pass for Rust and C/C++ examples.
- The old hidden fixture path is not the only tested source of truth for
  product-shaped workspaces.

### 225.L — Documentation

- [x] Update `book/src/getting-started/workspace-*.md` to point at
  `examples/workspaces/` instead of hidden test fixtures or template-only
  paths.
- [x] Add `examples/workspaces/README.md` explaining the small example
  set and the command flow.
- [x] Update `examples/templates/README.md` to clarify whether templates
  remain copy-out skeletons or are superseded by `examples/workspaces/`.

Acceptance:

- Book pages describe the same command flow:
  `nros setup` + `nros ws sync` / codegen, then platform-native build.
- No page implies `nros build`, `nros run`, `nros deploy`, or
  `nros launch` is the build/run path.

---

## 4. Non-Goals

- Do not move every `nros-tests` fixture into `examples/`.
- Do not add one workspace example per board or per RMW backend.
- Do not reintroduce `nros build` or `nros launch`.
- Do not make Bringup packages compile code.
- Do not make Node packages choose boards or contain `main()`.
- Do not keep placeholder nodes in user-facing promoted examples.

---

## 5. Fixture Classification

### 5.1 Promotion Sources

| current path | target | note |
|---|---|---|
| `examples/templates/multi-node-workspace/` | `examples/workspaces/rust/` | promoted product example uses generated `std_msgs/Int32`; old template still needs archive/internal decision |
| `examples/templates/pure-c-workspace/` | `examples/workspaces/c/` | keep CMake/native first |
| `examples/templates/multi-node-workspace-cpp/` | `examples/workspaces/cpp/` | keep CMake/native first |
| `examples/templates/c-and-cpp-mixed-workspace/` | `examples/workspaces/mixed/` | C + C++ first; Rust only when Corrosion is ready |

### 5.2 `nros-tests` Fixture Inventory

| fixture directory | classification | phase-225 action |
|---|---|---|
| `board_import_fvp` | platform smoke fixture | stay in tests; validates `nano_ros_use_board()`/FVP board import path |
| `diagnostic_cmake_fixture` | diagnostic/error fixture | stay in tests; negative CMake diagnostic surface |
| `diagnostic_rustc_fixture` | diagnostic/error fixture | stay in tests; negative Rust diagnostic surface |
| `multi_pkg_workspace_cpp` | obsolete fixture candidate | promoted equivalent is `examples/workspaces/cpp/`; keep only until build-fixtures/E2E refactor no longer needs hidden source |
| `multi_pkg_workspace_esp_idf` | platform smoke fixture | stay in tests; ESP-IDF/IDF component integration is platform-specific, not a primary native example |
| `multi_pkg_workspace_freertos` | platform smoke fixture | stay in tests for FreeRTOS run-plan/link coverage; possible source for future second Rust Entry package |
| `multi_pkg_workspace_mixed` | obsolete fixture candidate | promoted equivalent is `examples/workspaces/mixed/`; keep only while old tests still reference hidden fixture behavior |
| `multi_pkg_workspace_nuttx` | platform smoke fixture | stay in tests; NuttX-specific packaging/link behavior |
| `multi_pkg_workspace_platformio` | platform smoke fixture | stay in tests; PlatformIO-specific packaging/build integration |
| `multi_pkg_workspace_px4` | platform smoke fixture | stay in tests; PX4/gated platform smoke, not a user-facing generic workspace |
| `multi_pkg_workspace_threadx` | platform smoke fixture | stay in tests; ThreadX-specific Entry/platform integration |
| `multi_pkg_workspace_zephyr` | platform smoke fixture | stay in tests; Zephyr/west-specific integration and stub-node link behavior |
| `n9_workspace` | parser/planner fixture | stay in tests; macro/main-form and launch/codegen regression fixture |
| `n_board_agnostic_run_plan` | parser/planner fixture | stay in tests; board-agnostic run-plan behavior with multiple Entry packages |
| `o4_pkg_index_workspace` | parser/planner fixture | stay in tests; package-index and workspace-discovery regression coverage |
| `o5_nav2_compat_smoke` | parser/planner fixture | stay in tests; Nav2 compatibility/planning smoke with placeholder fallback paths |
| `one_dep_component_pkg` | parser/planner fixture | stay in tests; focused macro/dependency regression fixture |
| `orchestration_composable` | parser/planner fixture | stay in tests; launch parser/composable-node planning records |
| `orchestration_conditionals` | parser/planner fixture | stay in tests; launch condition evaluation records |
| `orchestration_e2e` | parser/planner fixture | stay in tests; orchestration record-generation E2E, not product build workflow |
| `orchestration_includes` | parser/planner fixture | stay in tests; launch include/deep/cycle parser cases |
| `orchestration_set_remap_env` | parser/planner fixture | stay in tests; remap/environment/set launch behavior |

No parser, diagnostic, or platform-smoke fixture should be exposed as a
primary user example. Product-shaped native Rust/C/C++/mixed examples now
live under `examples/workspaces/`; hidden test fixtures are retained only
for platform-specific smoke coverage, parser/planner edge cases, or
temporary backward compatibility during the fixture-refactor work.

---

## 6. Acceptance

- [x] `examples/workspaces/` contains a small product-shaped set:
  Rust, C, C++, and mixed.
- [x] Each promoted workspace follows Node / Bringup / Entry roles.
- [x] At least one workspace demonstrates multiple Entry packages that
  share the same Node and Bringup packages.
- [x] User-facing workspace examples contain real node behavior and real
  generated message usage, not placeholder stand-ins.
- [x] Tests use promoted examples as the source of truth for product
  workflow, while diagnostic fixtures stay in `nros-tests`.
- [x] Book links and README files point to the promoted workspaces.
- [x] Workspace fixtures are declared in `examples/fixtures.toml`.
- [x] Workspace fixtures are consumed through a build-fixtures lane, not
  bespoke test code.
- [x] Workspace E2E tests run prebuilt Entry binaries directly and do
  not build workspaces during the test stage.
- [x] The workspace fixture build path executes `nros ws sync` and
  `nros codegen-system` before invoking Cargo, CMake, or a platform
  build tool.
- [x] Node component API alignment with ROS composable-node concepts is
  either implemented or captured as a reviewed follow-up with a concrete
  compatibility plan.
- [x] Every promoted workspace is verified by following the documented
  user workflow. Failures discovered this way are fixed or recorded as
  product bugs with owners; they are not waived as "just examples."
