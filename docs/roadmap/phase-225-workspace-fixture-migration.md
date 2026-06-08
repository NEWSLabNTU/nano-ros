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

- **Node pkg**: reusable node code only. No `main()`, no board
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
        ├── rust_heartbeat_pkg/
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
- [x] Add at least two platform Entry packages sharing the same Node +
  Bringup packages. The prior `native_default_entry` package was removed
  because default launch selection is not a target platform; embedded
  target Entries remain follow-up platform coverage. The Rust workspace now
  has native, QEMU FreeRTOS, and ThreadX Linux Entry packages sharing the
  same Talker/Listener Node pkgs and `demo_bringup`.
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

Visible Rust workspace and template nodes use generated
`std_msgs/Int32`. Placeholder-only nodes are allowed only in internal
parser, planner, diagnostic, or platform-smoke fixtures whose comments
make clear that runtime pub/sub behavior is not under test.

- [x] Remove placeholder message types from promoted product examples.
- [x] Add real message dependencies to `package.xml` and generated
  bindings through `nros ws sync` / `nros generate-rust`.
- [x] Ensure each Node package has meaningful behavior:
  - talker publishes a real typed message;
  - listener deserializes/observes it;
  - optional service/action examples use real request/response paths.
- [x] Leave placeholder-only code only in explicitly internal test
  fixtures, with comments saying why runtime behavior is not under test.

Audit note: promoted product examples no longer contain
`PlaceholderInt32` or equivalent stand-ins. The old Rust template was
converted to generated `std_msgs::msg::Int32` and now follows the same
`nros ws sync` workflow as the promoted Rust workspace. Remaining
placeholder/stub content is limited to internal platform smoke,
parser/planner, and diagnostic fixtures under
`packages/testing/nros-tests/fixtures/`, where those fixtures test
build/link/planning behavior rather than runtime pub/sub.

Acceptance:

- `rg -n "PlaceholderInt32|placeholder message|stand-in" examples/workspaces examples/templates/multi-node-workspace`
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
- [x] Update the promoted examples to use the best current API names and
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

Wording cleanup note: promoted workspace README/CMake comments now use
"Node pkg" in prose. Remaining `component` spellings in promoted C/C++
workspaces are compatibility target names or generated ABI symbols such
as `<pkg>_<exec>_component` and `__nros_component_<pkg>_register`; book
pages label `nros new --component` as the current compatibility
scaffold flag for Node pkgs.

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

### 225.M - Node Component API Alignment Design

Follow-up review compared the promoted workspace Node-pkg APIs against
real upstream client-library shapes:

- `external/ros2-demos/composition/` - real `rclcpp_components`
  talker/listener/client/server examples.
- `external/rclcpp/` - `rclcpp::Node`, `rclcpp::NodeOptions`,
  publisher, subscription, timer, and component registration surfaces.
- `external/rclc/` - C node/publisher/subscription/executor handle
  APIs.
- `external/ros2_rust/` - `rclrs` `Executor::create_node`,
  `NodeOptions`, `Node::create_publisher`, and
  `Node::create_subscription` surfaces.

Reference conclusions:

- **rclcpp / rclcpp_components**: the reusable unit is a class with
  `explicit T(const rclcpp::NodeOptions& options)`, usually deriving
  from `rclcpp::Node`. The node name is passed to the base `Node`
  constructor, entities are created by topic/QoS/callback, and
  `RCLCPP_COMPONENTS_REGISTER_NODE(T)` registers the class, not an
  internal node ID.
- **rclc**: the API is explicit and embedded-friendly, but identity is
  still handle-based: `rclc_node_init_default(&node, name, ns,
  &support)`, `rclc_publisher_init_default(&pub, &node, ts, topic)`,
  `rclc_subscription_init_default(&sub, &node, ts, topic)`, and
  executor registration with concrete handles and callback/context
  pointers.
- **rclrs**: the Rust API takes a bare node name or `NodeOptions`,
  supports builder-style options such as `.namespace(...)`, and creates
  publishers/subscriptions from topic/options plus callbacks.

Design decision:

- User-authored Node APIs describe ROS graph semantics only: source
  default node name, namespace/options, topic/service/action names,
  message/service/action types, QoS, callbacks, and callback effects.
- Users must not author nano-ros internal IDs in normal source code.
  Legacy explicit ID constructors and raw descriptor fields are
  generated/build internals, not product API.
- Metadata extraction records declarations as structural slots local to
  the Node pkg, for example `NodeSlot(0)`, `EntitySlot(2)`, and
  `CallbackSlot(1)`. These slots are assigned by declaration order and
  are referenced through returned handles.
- `nros codegen-system` expands launch instances and assigns
  workspace-scoped generated IDs. Generated IDs are deterministic for a
  fixed launch/metadata input, but they are not user-facing names.
- ROS graph names stay separate from generated IDs. Source code may
  provide a default node name, matching ROS 2 `rclcpp::Node("talker")`;
  launch `name=` overrides it. If launch omits `name=`, codegen uses the
  source default. If neither is statically known, workspace codegen
  fails and asks the user to set `name=`.
- Build-time audit validates the effective graph names after launch
  expansion. `(domain_id, namespace, effective_node_name)` must be
  unique in one generated system. Duplicate fallback names are build
  errors with launch/source locations, not runtime surprises.
- Single-node apps may rely on the source default name. Workspaces may
  omit `name=` for ROS 2 compatibility, but generated systems must pass
  the same uniqueness audit.

Implications:

- The source default name is not an ID. Renaming a node in launch must not
  affect codegen-internal identity or callback dispatch wiring.
- Returned handles are the only way product code relates callbacks to
  entities. Product examples should call helpers such as
  `timer.publishes(&publisher)`.
- Dynamic source names are allowed only where no build-time system audit
  needs them. In workspace mode, a dynamic source default requires
  launch `name=`.
- Old explicit-ID paths remain temporarily for generated code,
  compatibility tests, and migration, then move behind internal/generated
  namespaces or feature gates.

Acceptance:

- Phase 225 records that internal IDs are generated codegen artifacts,
  not user-authored stable strings.
- The implementation work below is organized around metadata extraction,
  codegen identity assignment, public API migration, and removal of old
  manual-ID paths.

### 225.N - Generated-ID Node API Redesign

Metadata extraction and planning:

- [x] Add structural slots to metadata records beside the compatibility
  string IDs: node slot, entity slot, callback slot, and source
  location/provenance fields.
- [x] Record source default node name as metadata, separate from the
  generated node slot.
- [x] Represent callback effects with slot references while preserving
  compatibility string ID relations.
- [x] Teach source metadata JSON and plan schemas to preserve source
  defaults, declaration slots, source locations, and launch instance
  provenance.
- [x] Update planner/codegen to assign workspace-scoped generated IDs
  after launch expansion.
- [x] Add build-time effective-node-name resolution:
  `launch.name` > static source default > error.
- [x] Add build-time uniqueness audit for `(domain_id, namespace,
  effective_node_name)`; follow-up diagnostic polish should attach
  precise source declarations once source locations are populated.
- [x] Reject dynamic/unknown source default names in workspace mode when
  launch omits `name=`.

Rust public API:

- [x] Replace `create_node_with_options` with the normal spelling
  `create_node(options)` once the old explicit-ID overload is retired or
  moved.
- [x] Add publisher/subscription/timer helpers that avoid public manual
  entity IDs.
- [x] Add service/action helpers that avoid public manual entity and
  callback IDs.
- [x] Add callback-effect helpers that accept returned handles.
- [x] Replace callback dispatch APIs that expose manual callback IDs to
  product code with generated callback registration/typed callback hooks.
  Wave D adds product-facing `nros::Callback`, changes
  `ExecutableNode::on_callback` to receive that opaque event, and keeps
  `CallbackId` construction at runtime/codegen boundaries.
- [x] Keep legacy explicit ID types only in internal or generated modules.
  Wave D moved the root `NodeId`/`EntityId`/`CallbackId` re-export behind
  `doc(hidden)` and removed `CallbackId` from callback bodies. Wave 7 added
  name/topic/handle wrappers for the remaining parameter, service-client, and
  action-client paths, migrated product examples and CLI fixtures, and hid the
  ID-taking Rust declaration/runtime helpers from product docs so generated and
  internal compatibility code remain the only expected callers.
- [x] Migrate promoted Rust workspace and templates to zero manual IDs.
- [x] Migrate hidden Rust fixtures and orchestration E2E workspaces away
  from manual IDs where they are not explicitly testing legacy behavior.
  Wave 5A audited `packages/testing/nros-tests/fixtures/**` and migrated
  the product/workflow Rust fixtures (`n9_workspace`,
  `o4_pkg_index_workspace`, `o5_nav2_compat_smoke`,
  `one_dep_component_pkg`, `multi_pkg_workspace_freertos`, and
  `n_board_agnostic_run_plan`) from explicit node-ID declarations to
  source-name node creation. No fixture under that tree currently retains
  legacy explicit ID constructors or raw descriptor fields outside
  compatibility paths.

C public API:

- [x] Add rclc-shaped declared-node init helpers.
- [x] Add C entity helpers that return declaration handles and avoid
  product-authored stable-ID strings, for example
  `nros_declared_node_create_publisher_for_name(&node, &publisher,
  topic, type, hash)`.
- [x] Add callback/effect helpers that accept C handles, not string IDs.
- [x] Move raw C entity descriptor types and string-ID helpers to
  generated/internal headers.
  Wave C keeps the descriptor ABI layout in `<nros/node_pkg.h>` only as
  `_nros_node_entity_descriptor_t` for inline handle helpers and generated
  runtime callbacks. The old product spellings
  `nros_node_entity_descriptor_t`, `nros_declared_node_create(...)`,
  `nros_node_create_entity(...)`, string-ID entity helpers, and
  `nros_node_record_callback_effect(...)` are hidden unless
  `NROS_NODE_PKG_ENABLE_LEGACY_RAW_DESCRIPTOR_API` is defined.
- [x] Migrate promoted C workspace and templates to zero manual IDs.

C++ public API:

- [x] Add `NodeContext::create_node(out, const NodeOptions&)`.
- [x] Add typed `DeclaredNode::create_publisher<T>(topic, qos)` and
  `create_subscription<T>(topic, callback, qos)` helpers.
- [x] Add `rclcpp::NodeOptions` support to `rclcpp_compat.hpp`.
- [x] Update `rclcpp_components_register_node()` generated entry to
  instantiate `T(rclcpp::NodeOptions{})` when that constructor exists.
- [x] Replace declared-node helpers that take stable-ID strings with
  handle-returning helpers for publisher/subscription/timer/service/action.
- [x] Add declared callback/effect helpers that accept handles or
  callables, not string IDs.
- [x] Move raw C++ entity descriptor string-ID APIs to generated/internal
  headers.
  Wave C moves the raw descriptor layout to `nros::detail`, changes the
  component ABI callback to an opaque descriptor pointer, and hides the
  public `NodeEntityDescriptor`, explicit stable-ID overloads, string
  callback-effect overloads, and string callback subscription overload
  behind `NROS_CPP_ENABLE_LEGACY_RAW_DESCRIPTOR_API`.
- [x] Migrate promoted C++ workspace and mixed C++ Node pkg to zero
  manual IDs.
- [x] Migrate C++ templates to zero manual IDs.

C++ implementation note: `DeclaredEntity` and `DeclaredCallback` now
cover the promoted C++ workspace path, including handle-based callback
effects. The raw C++ descriptor remains a header-internal ABI detail
because the inline handle helpers still construct it before handing an
opaque pointer to generated/internal runtime callbacks.

Old path removal:

- [x] Mark public manual-ID constructors/APIs deprecated once the
  generated-ID path is available for all promoted examples.
  Wave 6A marks the legacy C public wrappers that accept product-authored
  stable IDs or raw entity descriptors deprecated while keeping the
  handle-returning `*_for_name` product path warning-free. The matching
  C++ compatibility overloads for explicit stable node/entity IDs, raw
  entity descriptors, string callback effects, and string callback
  subscriptions are also deprecated while handle-returning helpers route
  through internal raw implementations without warning.
- [x] Move legacy Rust ID imports out of the public prelude/docs for
  product authoring.
- [x] Move C/C++ raw descriptor headers or members behind generated-only
  include paths, keeping ABI shims only where codegen still needs them.
  Wave C default-hides the C/C++ public raw descriptor spellings and
  string-ID helpers while retaining opt-in legacy macros for
  compatibility. A separate generated-only physical header is no longer
  required for product API hygiene; the remaining internal structs are
  ABI payloads for generated/runtime callbacks.
- [x] Delete migration-only tests that assert user-authored duplicate ID
  behavior; replace them with generated-slot and duplicate graph-name
  audit tests. Wave 7 scan found no remaining product/workflow tests that
  depend on user-authored duplicate IDs; the remaining `DuplicateId`
  assertions are internal recorder/runtime invariants or generated dispatch
  compatibility tests.
- [x] Update book and README examples so no product path teaches manual
  IDs.
  Wave 5B audited `book/src/**` and `examples/**/README.md`; no product
  workflow doc teaches legacy explicit IDs or raw descriptors.

Implementation order:

1. Add generated slot metadata beside the current string IDs.
2. Teach codegen/planner to consume slots and emit generated IDs.
3. Migrate public APIs and examples to handles only.
4. Flip diagnostics/tests to graph-name and slot-based validation.
5. Deprecate, hide, then remove old manual-ID paths.

Acceptance:

- Product examples contain no legacy explicit ID constructors or raw
  descriptor fields.
- Launch `name=` is optional, but omitting it requires a statically known
  source default node name.
- Duplicate effective node names are diagnosed during build/codegen with
  launch and source locations.
- Real upstream `composition::Talker(const rclcpp::NodeOptions&)`-style
  constructors are accepted by the hosted compatibility path.
- Static-link composition remains explicit and deterministic, with
  generated IDs hidden inside generated code.

### 225.O - Workspace Topology and Mixed-Language Fixes

The review found example topology issues separate from the API surface.

- [x] Remove `native_default_entry` from
  `examples/workspaces/rust/`; default launch selection belongs in the
  one native Entry package.
- [x] Replace the second native Entry with real platform Entry packages
  for the first green platform targets. `examples/workspaces/rust/`
  now has `native_entry`, `qemu_freertos_entry`, and
  `threadx_linux_entry` sharing the same Talker/Listener Node pkgs and
  `demo_bringup`.
- [ ] Add `qemu_nuttx_entry` once the NuttX workspace build blockers
  below are resolved.
- [x] Add the Zephyr Entry package — DONE (Phase 225.P). `src/zephyr_entry`
  builds via `west build` on native_sim through the workspace fixture lane;
  the `nros setup` + `nros ws sync`/codegen + west workflow is wired. Its
  runtime E2E is gated only by an environmental native_sim↔zenoh
  connectivity issue that also fails the pre-existing single-node reference
  (`test_zephyr_to_native_e2e`) — tracked in the 225.P Status note, not an
  Entry defect.
- [ ] Add the ESP32 Entry package once its bare-metal runtime
  (`NullNodeRuntime` → real `ExecutorNodeRuntime`, shared 212.N track) and a
  CI-runnable OpenETH board land.
- [x] Update `examples/fixtures.toml`, fixture builders, and E2E lookup
  helpers after Entry topology changes.
- [x] Add generic native CMake/Corrosion support for Rust Node pkgs in
  mixed workspaces, or document the exact product-path blocker.
- [x] Add one Rust Node pkg to `examples/workspaces/mixed/`.
- [x] Update mixed Bringup launch to include C, C++, and Rust Node pkgs.
- [x] Build the mixed workspace through `nros ws sync`,
  `nros codegen-system`, and the normal CMake build.

Wave B implementation note:

- Rust `nros::node!()` now emits the C/C++ entry-compatible
  `extern "C" __nros_component_<pkg>_register` symbol by adapting the
  C++ `NodeContext` ABI to the Rust `NodeRuntime` trait.
- `nano_ros_node_register(LANGUAGE RUST ...)` imports the Rust Node pkg
  through Corrosion as a staticlib and exposes the normal
  `<pkg>_<exec>_component` target consumed by generated C/C++ entry
  link sidecars.
- `examples/workspaces/mixed/` includes `rust_heartbeat_pkg`, a Rust
  timer Node pkg, and the Bringup launch now instantiates C, C++, and
  Rust Node pkgs in one native Entry binary.

Wave A platform Entry update:

- `examples/fixtures.toml` has `workspace-rust-qemu-freertos` and
  `workspace-rust-threadx-linux` rows. `just freertos build-examples`
  and `just threadx_linux build-examples` both invoke
  `scripts/build/workspace-fixtures-build.sh <platform> rust`, so these
  entries build through the same workspace workflow as the native Entry:
  `nros ws sync`, `nros codegen-system`, then Cargo.
- The shared Rust Node pkgs now select their platform through package
  features. Entry pkgs depend on the same `talker_pkg` and
  `listener_pkg` sources with `native`, `freertos`, or `threadx-linux`
  features instead of copying Node code per platform.

Remaining blockers:

- NuttX cannot be added as a green workspace Entry row yet. Three
  compounding problems, narrowed 2026-06-08:
  - **Named libc gap — now wired.** The workspace path did not receive
    the NuttX-only patched `libc` override that
    `scripts/build/nuttx-libc-patch.sh` applies to single-node fixtures
    (sourced + called in `fixtures-build.sh`, absent in
    `workspace-fixtures-build.sh`). `workspace-fixtures-build.sh` now
    sources the helper and calls `nros_nuttx_libc_patch` right after
    `nros ws sync`, mirroring the single-node lane. Idempotent and a
    no-op for non-NuttX rows (verified: native rust workspace build
    unaffected). This is necessary but only fires once the rendered
    config carries the global `armv7a-nuttx-eabi` target line — i.e.
    once the row below exists.
  - **ws-sync merged-config poisoning (deeper blocker, unresolved).**
    `nros ws sync` renders one merged root `.cargo/config.toml` for the
    whole workspace. The NuttX board template
    (`nros-board-nuttx-qemu-arm/nros-board.toml`) declares
    workspace-global `[build] target = "armv7a-nuttx-eabihf"` +
    `[unstable] build-std`, which would poison `native_entry` (no
    `--target`) and force build-std on every row. freertos/threadx work
    because their templates contribute only a target-scoped
    `[target.<triple>]` block + per-row `--target`. A green NuttX row
    needs either ws-sync per-target config emission or a
    workspace-friendly NuttX board `cargo_config` variant (omit global
    `[build] target`/`[unstable]`, rely on per-row `--target` +
    `CARGO_UNSTABLE_BUILD_STD` env). This sits in the CLI/board-template
    layer.
  - **Unverified deploy shape (unresolved).** All existing NuttX Rust
    examples are staticlib Components linked into the kernel image via
    `libapps.a`; none use `nros::main!`. A `[[bin]]` + `nros::main!`
    `qemu_nuttx_entry` would be the first standalone NuttX Rust binary,
    and `nros-board-nuttx-qemu-arm`'s `BoardEntry::run` path is
    unverified. The separate `libapps.a` stale-object link failure
    (unresolved `nros_*`/`nros_cpp_*`) is orthogonal to the cargo
    workspace build but still blocks `just nuttx build-examples` as a
    whole.
- Zephyr's Phase 226 scheduler is still a single-node fixture scheduler:
  `just zephyr build-fixtures` emits leaves from
  `scripts/build/zephyr-fixture-leaves.sh` and runs them through
  `scripts/build/zephyr-fixture-run-one.sh`; it does not consume
  `[[workspace_fixture]]` rows or route Zephyr workspace examples
  through `scripts/build/workspace-fixtures-build.sh`. Scoped
  2026-06-08: tractable but multi-day. `workspace-fixtures-build.sh`
  has only two build branches (cargo / cmake-target) — neither is
  `west`. A Zephyr Entry is a Corrosion staticlib that links into a
  `west build` ELF, and the codegen contract differs (native's
  `nros codegen-system --out` emits Rust; the Zephyr lane uses
  CMake-time `nros_system_generate`/`rust_cargo_application`).
  **Recommended path (Approach A):** teach
  `scripts/build/zephyr-fixture-leaves.sh` to also emit a
  workspace-Entry leaf from the `[[workspace_fixture]] platform="zephyr"`
  row so the proven `zephyr-fixture-run-one.sh` west path builds it
  unchanged, rather than re-implementing west orchestration in the
  lightweight workspace script. Also needs a `zephyr_entry` package
  (CMakeLists + prj overlays + staticlib `lib.rs`), new `fixtures.toml`
  schema fields (`board`, `conf_files`), and a `fixtures-manifest.py`
  Zephyr validation branch (`_cmake_has_entry_target` rejects the
  `project()` + `rust_cargo_application()` shape).
- ESP32 has no workspace fixture row/build lane yet; add it only after
  the platform recipe has a workspace-aware `nros setup` + codegen +
  platform build path. Scoped 2026-06-08: not tractable in one pass.
  - **Latent macro bug — fixed.** `nros-macros` `main_macro.rs` mapped
    `"esp32" => "::nros_board_esp32::Esp32"`, but the crate exports
    `Esp32C3` (no `Esp32` type exists). Form-3 `nros::main!(launch=…)`
    would have failed to compile. Fixed to `Esp32C3` (verified
    `cargo check -p nros-macros` clean). No example exercised this path,
    which is why it went unnoticed.
  - **Runtime stubs (unresolved).** `Esp32C3::run` routes through
    `nros-board-bare-metal::run_entry`, which builds the closure context
    with `NullNodeRuntime` — every `register()` errors loud (awaiting
    the deferred 212.N.4 codegen runtime). Contrast freertos, whose
    driver opens a real `Executor` + `ExecutorNodeRuntime`; that is why
    freertos/threadx workspace rows work and bare-metal/esp32 cannot.
    `Esp32C3::init_hardware` also only installs the clock — no
    WiFi/radio/smoltcp bringup.
  - **No CI-runnable board (unresolved).** `nros-board-esp32` is
    WiFi-only (needs SSID/PASSWORD + AP). The CI/QEMU-runnable OpenETH
    board `nros-board-esp32-qemu` has no `BoardEntry`/board-ZST and is
    not in the macro table, so `nros::main!` cannot drive it.
  - **Build-lane plumbing (unresolved).** The lane runs bare `cargo`;
    `riscv32imc-unknown-none-elf` needs nightly (`RUSTUP_TOOLCHAIN`) +
    scoped `-Z build-std` (global `[unstable] build-std` in the shared
    workspace config would break native/freertos/threadx rows).
- Mixed C/C++/Rust registration now builds through the static CMake
  entry path, but the C/C++ native board adapter still records node
  declarations with no-op operations. Full mixed-language runtime
  pub/sub assertions remain blocked on the native C/C++ `NodeContext`
  runtime moving beyond its recording-only adapter.

Acceptance:

- Multiple Entry packages mean multiple target platforms, not multiple
  native launch spellings.
- The mixed workspace is actually mixed C/C++/Rust, not only C/C++.
- Missing Corrosion/codegen support is fixed in the product path, not
  hidden with fixture-only glue.

### 225.P — Zephyr Workspace Entry (design + implementation)

Resolves the 225.O Zephyr Entry blocker. Designed 2026-06-08 after a
three-agent gap-check against the real Zephyr module + macro + fixture
code. Supersedes the earlier rough framing.

**Held principle.** On Zephyr the RTOS framework *is* the workflow:
`west build` is the build verb, Kconfig selects the RMW, the Entry is a
Zephyr application, and nano-ros integrates as a Zephyr module
(`zephyr/module.yml`) + CMake extensions. No `nros build` / `just`
wrapper as the user-facing build verb; no `cargo build -p entry`; no
RMW-via-`--features` typed by the user; no board baked into a package.

**Refinement to the Phase 212 "one Entry per platform" rule.** On an
RTOS with its own board abstraction (Zephyr), it is *one Entry per
RTOS*, board chosen at build time via `west build -b <board>`. A single
`zephyr_entry` covers native_sim, nrf52, stm32, aemv8r, … Contrast
freertos/threadx, whose board crates are board-specific so the Entry
bakes the board.

**User-facing UX (decision A — explicit `nros ws sync`):**

```sh
source ./activate.sh
nros ws sync                          # provision message bindings (once, platform-agnostic)
west build -b native_sim/native/64 src/zephyr_entry \
    -- -DCONF_FILE="prj.conf;prj-zenoh.conf"
west build -t run                     # native_sim; flash for hardware
```

`nros ws sync` is the workspace-provisioning step (sibling to `west
update` / `rosdep`), not a compile step — it is platform-agnostic
message codegen (verified: no per-board/per-RMW output) and needs no
Zephyr-specific change. `west build` is the only build verb.

**Corrected mechanism (the gap-check overturned the first design).**

- `nros_system_generate()` is the **C/C++ component-ABI** glue path
  (emits C `system_main.c` referencing `nros_component_*_register` +
  undefined `nros_system_init`/`nros_system_spin`). It does **not**
  compile or link Rust node crates and must **not** be used by the Rust
  Entry — linking its C glue into a Rust app risks undefined symbols.
- The real Rust-on-Zephyr path is `zephyr-lang-rust`'s
  `rust_cargo_application()` (the Entry crate is a `staticlib` exporting
  `rust_main`), with RMW flowing Kconfig overlay → `EXTRA_CARGO_ARGS`
  exactly as `examples/zephyr/rust/talker/CMakeLists.txt` does.
- `nros::main!(launch=…)` is currently broken for Zephyr
  (`board_path_for("zephyr")` → nonexistent `Zephyr` type; emits a `fn
  main` Zephyr forbids). But `main_macro.rs` already has framework
  dispatch (`framework_for(deploy)`) emitting non-`BoardEntry` shapes for
  `rtic-stm32f4`/`embassy-stm32f4`. Adding a **`zephyr` framework
  branch** that emits `rust_main` is architecturally consistent — not a
  hack — and keeps the Entry source identical to native/freertos
  (`nros::main!(launch = "demo_bringup:system.launch.xml");`), preserving
  the launch file as the single source of truth for the node set and
  uniform cross-platform Entry UX.
- The existing `multi_pkg_workspace_zephyr` fixture is a stub (nodes
  never compile/link/register); it is not the green path and stays a
  recording test.

**Test lane = the user command.** The fixture must run the same `west
build`. Approach A: emit one workspace-Entry leaf from
`zephyr-fixture-leaves.sh` (after the matrix loop, like the
logging-smoke block — `role="entry"` bypasses the role/port helpers);
the proven `zephyr-fixture-run-one.sh` west path builds it unchanged.

Work items:

- [x] **P.1 Node-pkg feature.** Add `zephyr = ["nros/alloc",
  "nros/rmw-cffi", "nros/platform-zephyr", "nros/ros-humble"]` to
  `examples/workspaces/rust/src/{talker_pkg,listener_pkg}/Cargo.toml`
  (node bodies already board-agnostic — no body changes).
- [x] **P.2 Macro `zephyr` framework branch.** In
  `packages/core/nros-macros/src/main_macro.rs`: add `zephyr` to
  `framework_for(deploy)`; emit `#[unsafe(no_mangle)] pub extern "C" fn
  rust_main()` that waits for network, opens an `Executor`, wraps it in
  `ExecutorNodeRuntime`, `register_node::<T>()` for each launch-named
  node, then spins — bounded via the existing `NROS_ENTRY_SPIN_MS` /
  `NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS` + `observed_callback_counts()`
  when hosted (native_sim is `x86_64`-hosted), forever otherwise. Route
  `deploy="zephyr"` to the real `ZephyrBoard` (NetworkWait/config only,
  not `BoardEntry`). Reuse the single-node
  `nros::zephyr_component_main!` body (`packages/core/nros/src/lib.rs`)
  as the reference.
- [x] **P.3 Entry crate.** Create
  `examples/workspaces/rust/src/zephyr_entry/`: `Cargo.toml` (`[lib]
  crate-type=["staticlib","rlib"]`, `[package.metadata.nros.deploy.zephyr]`,
  deps `nros`+`platform-zephyr`, `nros-board-zephyr`,
  `talker_pkg`/`listener_pkg` feature `zephyr`, `zephyr`/`zephyr-build`);
  `src/lib.rs` = `nros::main!(launch = "demo_bringup:system.launch.xml");`;
  `CMakeLists.txt` (`find_package(Zephyr)` + `project()` + Kconfig
  RMW→`EXTRA_CARGO_ARGS` + `rust_cargo_application()`, mirror the talker
  example); `prj.conf` (`CONFIG_RUST`, `CONFIG_NROS_RUST_API`, executor/
  heap) + `prj-{zenoh,xrce,cyclonedds}.conf` + `boards/native_sim_native_64.conf`
  + `build.rs` + `sample.yaml` + `package.xml` + `.gitignore`. Add
  `exclude = ["src/zephyr_entry"]` to the root `Cargo.toml` (keep it out
  of `members` — built by west, not plain cargo).
- [x] **P.4 Bringup.** Add a `[deploy.zephyr]` block to
  `src/demo_bringup/system.toml` if deploy resolution needs it.
- [x] **P.5 Manifest.** `examples/fixtures.toml`: add `board` +
  `conf_files` to the schema comment and a `[[workspace_fixture]]
  id="workspace-rust-zephyr" platform="zephyr" board="native_sim/native/64"
  conf_files=["prj.conf","prj-zenoh.conf"]` row.
  `scripts/build/fixtures-manifest.py`: require `board` for zephyr; add
  `_validate_zephyr_workspace` (entry CMakeLists has `project(` +
  `rust_cargo_application()`/`target_sources(app`; `prj.conf` + each
  `conf_files` entry exist) routed by a `platform=="zephyr"` branch in
  `validate_workspace_fixture` (the existing rust/cmake branches reject a
  west app); extend `workspace_record` with `board` + `conf_files`
  columns (11→13) and the `list-workspaces` consumer reads.
- [x] **P.6 Build lane (Approach A).**
  `scripts/build/zephyr-fixture-leaves.sh`: emit a workspace-Entry leaf
  after the matrix loop (unique `build_dir`, e.g. `build-ws-rs-entry-zenoh`;
  direct record construction, not via `variant_offset_for_role`).
  `just/zephyr-ci.just` `build-fixtures`: add a workspace-root `nros ws
  sync` + `nros codegen-system --bringup src/demo_bringup` prep before
  scheduling. `zephyr-fixture-run-one.sh` + `zephyr-fixture-make-driver.sh`:
  no change (record-driven).
- [x] **P.7 E2E.** Add `get_prebuilt_zephyr_workspace_entry()` to
  `packages/testing/nros-tests/src/zephyr.rs` (resolve
  `build-ws-rs-entry-zenoh/zephyr/zephyr.exe`); add a test in
  `tests/zephyr.rs` that starts it and asserts boot banner + both nodes
  register + the talker's `Published:` line (in-process listener will not
  receive its own session's publish over zenoh — assert publish +
  registration, or run an external listener). Fail-fast with a "build
  workspace fixtures first" hint when the binary is absent.
- [x] **P.8 Docs.** Update `book/src/getting-started/workspace-entry-pkg.md`
  + `examples/workspaces/README.md` with the Zephyr `west build` flow +
  the `nros ws sync` provisioning step.

Acceptance:

- `west build -b native_sim/native/64 src/zephyr_entry --
  -DCONF_FILE="prj.conf;prj-zenoh.conf"` builds the two-node Entry ELF.
- The Entry source is `nros::main!(launch=…)`, identical to the
  native/freertos/threadx entries.
- `just zephyr build-fixtures` builds the workspace leaf through the
  same west path; the E2E test runs the prebuilt `zephyr.exe` directly.
- RMW is selected by Kconfig overlay, board by `west build -b`, and the
  user types no nano-ros-specific build verb.

Status (2026-06-08): P.1–P.8 implemented via a four-agent wave, then
**build-verified end-to-end with a provisioned Zephyr toolchain** (`just
zephyr setup`). `west build -b native_sim/native/64 src/zephyr_entry --
-DCONF_FILE="prj.conf;prj-zenoh.conf"` produces the two-node Entry
`zephyr.exe` (`just zephyr build-fixtures` with
`NROS_ZEPHYR_FIXTURE_FILTER=workspace-entry`, EXIT 0), and the Entry boots
on native_sim, brings up the network, registers the launch node set, and
attempts the zenoh session — identical runtime behavior to the proven
single-node reference.

The real `west build` surfaced **three product bugs the static checks
could not** (all fixed):
1. `build.rs` called `zephyr_build::export_kconfig_bool_options()`, renamed
   to `export_bool_kconfig()` in the pinned zephyr-lang-rust — this was
   stale in **all 8 existing zephyr rust examples** too (the whole zephyr
   rust matrix was broken against this pin); fixed repo-wide.
2. The Entry `[lib] name` must be `rustapp` (zephyr-lang-rust's
   `rust_cargo_application()` links `librustapp.a` by fixed name).
3. The `Framework::Zephyr` macro branch wrongly assumed native_sim is a
   hosted (`x86_64-unknown-linux-gnu`) target and used `ZephyrBoard::wait_link_up`
   (calls `static inline` `net_if_is_up`/`k_msleep` — no link symbol →
   undefined-reference at the native_sim final link) plus `std`-based
   bounded spin. native_sim is `x86_64-unknown-none` (`no_std`). Rewrote
   the branch to use `platform::zephyr::wait_for_network`, the `log`
   facade routed to the Zephyr logger (works `no_std`), and a forever-spin
   (the OwnedSpin `NROS_ENTRY_*` bounded path needs `std::time` and does
   not apply to Zephyr; the workspace E2E observes delivery from an
   external listener + stops the process, so bounded spin is unnecessary).

Cross-agent reconciliation: the locator was unified to the Zephyr
rust-pubsub port 7456 (leaf bake + E2E router + external-listener
`NROS_LOCATOR`), and the E2E was reshaped from a single-process self-check
to the canonical external native-listener pattern (asserts real
cross-process `/chatter` delivery), routed into the serial
`qemu-zephyr-pubsub-rust` nextest group.

**E2E data-delivery is NOT green in this checkout — but for an
environmental reason, not an Entry bug.** The Entry's session open returns
`Transport(ConnectionFailed)` reaching the host zenohd over native_sim
NSOS, and the **pre-existing single-node reference `test_zephyr_to_native_e2e`
fails identically** here (same `ConnectionFailed`, same "Listener timed
out"). So native_sim↔zenoh connectivity is broken for every zephyr-zenoh
E2E in this environment, independent of Phase 225.P. The 225.O Zephyr
Entry checkbox stays unchecked until that environmental connectivity (or
the reference test) is green; at that point the workspace Entry E2E is
expected to pass with it, since the Entry already builds and runs
identically to the reference.

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
- [x] At least one workspace demonstrates multiple platform Entry packages
  that share the same Node and Bringup packages. `native_default_entry`
  does not satisfy this criterion; `examples/workspaces/rust/` now carries
  native, QEMU FreeRTOS, and ThreadX Linux Entry packages over the same Node
  and Bringup packages.
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
  implemented or captured through the 225.M/225.N compatibility plan.
- [x] Every promoted workspace is verified by following the documented
  user workflow. Failures discovered this way are fixed or recorded as
  product bugs with owners; they are not waived as "just examples."
