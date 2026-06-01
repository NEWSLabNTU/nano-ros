# Phase 212 — Multi-Node Workspace Layout (LIVE DESIGN)

## 1. Status & Audience

**LIVE doc, WIP.** Audience = phase-212 implementers + reviewers. Expect open questions throughout, expect pushback. Decisions marked **LOCKED** are settled; **OPEN:** marks live debate.

---

## 2. Constraints (locked)

1. **ROS standard layout.** Launch files live in dedicated orchestration package (`<system>_bringup` convention). Component packages stay code-only.
2. **No colcon as primary orchestrator.** Error attribution (rustc/gcc diagnostics swallowed by `Failed <<<`), embedded targets ignored, install/ tree dead weight for MCUs, cross-language codegen invisible to colcon DAG. Colcon stays *available* for Autoware-style outer integration via `colcon-cargo-ros2` seam; never the inner workflow.
3. **cargo / cmake stay user-facing.** `cargo build` works at workspace root for Rust-majority. `cmake --build build` works for C++-majority. Rustc errors stay rustc errors.
4. **nros never a build verb.** No `nros build` / `nros test` / `nros flash`. nros = provisioner + codegen + metadata + deploy. Idf.py-shaped, not colcon-shaped.
5. **One-package workflow stays canonical for tiny fixtures.** Multi-package shape kicks in at ≥2 components. (Phase 212 already decided single-package workflow user-facing.)

---

## 3. Reference patterns

- **nav2 / Autoware / turtlebot3** — `<system>_bringup` carries `launch/`, `config/`, `package.xml` with only `<exec_depend>` lines pulling in component packages. No `<build_depend>` — orchestration role is pure runtime resolution. **Takeaway:** orchestration package is a *role*, not a build artifact. Zero compiled code.
- **Cargo workspace** — single dependency graph, unified feature DAG per (package, target), `-j N` rayon scheduler. Build-scripts are crates-first. **Takeaway:** cargo IS the orchestrator for Rust; do not wrap it.
- **CMake `add_subdirectory` vs `ExternalProject_Add`** — former shares cache + targets, latter isolates. **Takeaway:** own-code uses `add_subdirectory`; corrosion provides bidirectional cargo↔cmake bridge.
- **Zephyr west + ESP-IDF idf.py + PlatformIO** — all share shape: SSoT manifest at root (`west.yml` / `platformio.ini` / `idf` component scan) + per-component manifest (`module.yml` / `library.json` / `idf_component_register`). Tool synthesizes one CMake/SCons graph at invocation. **Takeaway:** root manifest + per-package manifest is the dominant pattern; nros.toml follows it.
- **Corrosion** — already used Phase 175.A. `corrosion_import_crate` makes Rust staticlib a normal CMake target. **Takeaway:** when graph crosses languages, CMake should be the top-level driver because cargo cannot consume CMake targets in reverse.

Citations: `docs/design/ros2-user-workflow.md`, `docs/roadmap/phase-212-ux-cargo-native-and-file-consolidation.md` (lines 473–731), `nros-cli/packages/colcon-cargo-ros2/`, `CLAUDE.md` Examples + CMake Path Convention sections.

---

## 4. The orchestration package

**LOCKED shape:**

```
demo_bringup/
├── package.xml         # <name>demo_bringup</name>
│                       # <buildtool_depend>ament_cmake</buildtool_depend>  (or absent — see OPEN below)
│                       # <exec_depend>talker_pkg</exec_depend>
│                       # <exec_depend>listener_pkg</exec_depend>
├── system.toml         # [system] launch=..., components=[...], rmw=..., domain_id=...
│                       # [deploy.native] / [deploy.qemu-mps2-an385] / ...
│                       # [[domain]] / [[bridge]]
├── launch/
│   └── system.launch.xml
├── config/             # optional — params.yaml, rviz, etc.
└── README.md
```

**No `Cargo.toml`. No `CMakeLists.txt`. No `src/`.** Pure declarative.

**Naming: `<system>_bringup`.** Aligns nav2/Autoware/turtlebot3. Accept `<system>_launch` as documented alias. Reject plain `<system>` (collides w/ ament metapackage idiom).

**Dependencies — two layers, both mandatory:**
- `package.xml` `<exec_depend>` → ament/colcon discovery + install ordering when used inside outer-colcon workspace.
- `system.toml` `[system].components` → nros planner's authoritative runtime set.

`nros check` cross-validates the two. Single source = `system.toml`; `nros emit package-xml` regenerates `<exec_depend>` block (mirrors Phase 212.C `[package.metadata.ament]` pattern).

**Lint:** `nros check` rejects orchestration pkg w/ `[[bin]]`/`[lib]`/`add_executable`. Code goes in sibling component pkg.

**OPEN: should orchestration pkg ship a stub Cargo.toml?** Two paths:
- **Path A** (recommended): no Cargo.toml. Pkg not a cargo workspace member. `cargo nros plan demo_bringup` finds it via dir walk + `package.xml`. Pro: cleaner, no fake `lib.rs`. Con: `cargo nros plan` must walk outside `[workspace] members`.
- **Path B**: stub `Cargo.toml` w/ empty lib. Pkg IS workspace member. Pro: `cargo nros plan -p demo_bringup` works via cargo's `-p` flag. Con: fake target pollutes `cargo build` output, needs `[lib] path = "src/lib.rs"` w/ empty file.

Leaning A. Need to prototype `cargo nros plan <dir>` discovery first.

**OPEN: `buildtool_depend` in `package.xml`?** ament_cmake assumes empty CMakeLists installs `share/<pkg>/launch/`. Without colcon in inner loop, who installs? `nros deploy` reads `launch/` directly from the source tree — no install step. Maybe omit `<buildtool_depend>` entirely. Need to check if `ros2 launch` (when user *does* run colcon outside) still resolves.

---

## 5. The workspace root

**Workspace root = thin pointer, not a system definition.**

```
my_ws/
├── Cargo.toml          # [workspace] members = ["talker_pkg", "listener_pkg"]
│                       #             exclude  = ["demo_bringup"]    (if Path A from §4)
│                       # [workspace.metadata.nros]
│                       #   default_system = "demo_bringup"
│                       #   # optional global overrides:
│                       #   # rmw_override = "cyclonedds"
├── CMakeLists.txt      # OPTIONAL — only for C++-majority workspaces
│                       # project(my_ws); include(nano_ros_workspace_metadata)
│                       # nano_ros_workspace_metadata(SYSTEM demo_bringup)
│                       # add_subdirectory(listener_pkg)
├── .gitignore          # /target/  /build/  /install/
└── (component pkgs + bringup pkg, siblings)
```

**No workspace-root `nros.toml`.** Retired. System definition lives in `<bringup>/system.toml`. Rationale: matches ROS muscle memory (every nav2/Autoware tutorial points users at `nav2_bringup/launch/`, not a root TOML). Decouples workspace's build graph from the system graph.

**Workspace-root metadata reduced to:**
- `default_system` — disambiguates `cargo nros plan` with no args.
- Optional global RMW / deploy-target overrides (rare; for `nros plan --override` workflows).

**Per-system definition** (`<bringup>/system.toml`):
- `[system]` — components list, launch file, default RMW, default domain.
- `[deploy.<target>]` — per-target overrides.
- `[[domain]]` / `[[bridge]]` — per-system topology.

**No duplication.** Root pointer + per-system definition is two different concerns. The temptation to mirror per-system fields at root (e.g. `[workspace.metadata.nros.system.demo]`) is rejected — re-creates colcon's monorepo-of-systems pattern + breaks per-system `<exec_depend>` hygiene.

**OPEN: multiple bringup pkgs sharing fragments?** If `sim_bringup` and `field_bringup` share 80% of `[[domain]]`/`[[bridge]]` config, where does the shared fragment live? Options:
- (a) Duplicate. Honest, traceable, no magic. Painful at scale.
- (b) `include = "../shared/domains.toml"` in `system.toml`. nros expands. Adds path resolution semantics.
- (c) Workspace-root `[workspace.metadata.nros.defaults]` table that per-system TOMLs inherit + override.

Leaning (a) until a real workspace hits the pain. Don't pre-build inheritance.

---

## 6. Build orchestration without colcon

### 6.1 Rust-majority workspace (cargo top-level)

`cargo build` at workspace root → cargo's native scheduler builds all `[workspace] members`. Each component crate has `[package.metadata.nros.component]` table + `nros-build` build-dep (Phase 212.B). `build.rs` reads sibling `*.msg` via `cargo:rerun-if-changed=` for incremental correctness.

Orchestration pkg excluded from `[workspace] members` (Path A) → never built by cargo. `cargo nros plan demo_bringup` invoked separately reads `system.toml` + workspace component manifests → emits `target/nros/demo_bringup/plan.json`. No build step.

Diagnostics path unchanged: rustc → stderr → user terminal. No colcon wrapping.

### 6.2 C++-majority workspace (cmake top-level)

`cmake -S . -B build && cmake --build build` at workspace root. Root `CMakeLists.txt`:

```cmake
project(my_ws)
find_package(NanoRos REQUIRED)   # NO — Phase 140 deleted this
                                  # actually: add_subdirectory(<nano-ros-repo-root>)
include(nano_ros_workspace_metadata)
nano_ros_workspace_metadata(SYSTEM demo_bringup)   # shells `nros plan` at configure time
add_subdirectory(talker_pkg)
add_subdirectory(listener_pkg)
```

`nano_ros_workspace_metadata` (≤150 LoC, Phase 212.D) shells `nros plan <bringup>` at *configure* time, emits a generated `nros_components.cmake`, `include()`s it. CMake sees component targets natively.

Diagnostics: gcc/clang → stderr → user terminal. No wrapping.

### 6.3 Mixed Rust + C++ workspace

**CMake is the top-level driver.** Rule: cargo can be consumed as a cmake target (via Corrosion); cmake cannot be consumed as a cargo target. So when graph crosses languages, cmake wins:

```cmake
project(my_ws)
add_subdirectory(<nano-ros-repo-root>)
include(nano_ros_workspace_metadata)
nano_ros_workspace_metadata(SYSTEM demo_bringup)

corrosion_import_crate(MANIFEST_PATH talker_pkg/Cargo.toml)
add_subdirectory(listener_pkg)
target_link_libraries(listener_pkg PRIVATE NanoRos::NanoRos talker_pkg)
```

For pure-Rust workspaces, cargo stays top-level — no cmake needed.

**Cross-language topo-ordering** at workspace level (e.g. "build all C++ RMW backends before Rust examples linking them") handled by `nros plan` emitting a topo DAG `plan.json`. Per-component dep resolution stays inside corrosion + build.rs.

### 6.4 Embedded path (Zephyr / FreeRTOS / ESP-IDF)

Untouched. Per-RTOS shells at `integrations/<rtos>/` (Phase 139/140) re-export root CMake. `west build` / `idf.py build` drive their own ninja graph; nros is the provisioner + codegen layer underneath. Orchestration package's `launch/` is irrelevant on MCU — flashed `.elf` runs `nros::init` from baked config.

**OPEN:** does the orchestration pkg's `system.toml` get baked into MCU firmware (like `app_config.h` today)? Or is `system.toml` purely host-side (host-tool reads to drive deploy/flash)? Leaning host-side. Per-node `[package.metadata.nros.component]` in component crate's `Cargo.toml` stays the bake source.

---

## 7. End-to-end user workflow

### Step 1 — Scaffold workspace

```bash
mkdir my_ws && cd my_ws
nros new system robot_bringup --components talker,listener   # NROS
```

Tree:
```
my_ws/
├── Cargo.toml                      # [workspace] members=["talker","listener"]
│                                   # [workspace.metadata.nros] default_system="robot_bringup"
├── robot_bringup/
│   ├── package.xml
│   ├── system.toml
│   └── launch/system.launch.xml
├── talker/
│   ├── Cargo.toml
│   └── src/lib.rs                  # #[nros::component] stub
└── listener/
    ├── Cargo.toml
    └── src/lib.rs
```

### Step 2 — Edit components

User edits `talker/src/lib.rs` + `listener/src/lib.rs`. Normal Rust authoring.

### Step 3 — Build

```bash
cargo build                                                  # CARGO
```

Cargo workspace builds talker + listener. Build-scripts (`nros-build`) regenerate message bindings on `.msg` change. Orchestration pkg untouched (excluded from workspace members).

### Step 4 — Plan + verify

```bash
cargo nros check                                              # NROS (transitive via cargo subcmd)
cargo nros plan                                               # NROS (default_system = robot_bringup)
```

`check` cross-validates `<exec_depend>` ↔ `[system].components`. `plan` emits `target/nros/robot_bringup/plan.json`.

### Step 5 — Deploy + launch

```bash
nros deploy native                                            # NROS
ros2 launch robot_bringup system.launch.xml                  # ROS 2 (when wanted)
# or
nros launch robot_bringup                                     # NROS (host-side, no ament install needed)
```

Multiple processes spawn per `plan.json`. Each reads its baked domain/RMW config.

**OPEN:** is `nros launch` a real verb? Or does the user always go through `ros2 launch` after a (one-time) ament install of `<bringup>/launch/`? Conflict: §2 constraint says "no colcon as primary" — but `ros2 launch` reads ament index. Maybe `nros launch` parses the same `system.launch.xml` independently of ament. Need to scope.

### C++-majority variant (step 3 replacement)

```bash
cmake -S . -B build && cmake --build build                   # CMAKE
```

Steps 4–5 unchanged. Step 4's `cargo nros plan` becomes either bare `nros plan robot_bringup` (no cargo subcmd) or `cmake --build build --target nros-plan`. **OPEN** — see §8.

---

## 8. Open questions

1. **Orchestration pkg `Cargo.toml`?** (Path A no-toml vs Path B stub-toml.) Decision blocked on prototype: can `cargo nros plan <dir>` cleanly walk outside `[workspace] members`? §4.
2. **Multi-system shared config.** Duplicate vs `include =` vs workspace-root `[defaults]`. Wait for real pain. §5.
3. **`nros launch` vs `ros2 launch`.** Host-side launcher independent of ament, or always shell to `ros2 launch` after a one-off install? Affects whether orchestration pkg needs `<buildtool_depend>ament_cmake</buildtool_depend>`. §7.
4. **C++ workspaces — `cmake nros` subcommand?** No cmake plugin idiom. C++ users invoke `nros plan` / `nros deploy` directly. Asymmetric vs cargo's `cargo nros plan`. Phase 212 line 670 already accepts this asymmetry as honest. Confirm. §6.
5. **Does `system.toml` belong to the orchestration pkg or stay workspace-root?** This doc says move to bringup pkg. Argument for staying root: a workspace w/ exactly one bringup pkg has indirection-for-nothing. Argument for moving: multi-system workspaces, ROS muscle memory, decouples build graph from system graph. Leaning move. §5.
6. **`[system].components` schema.** List of crate names, or list of `{name, role, qos_overrides}` tables? Today's `nros.toml` already has per-component override blocks. Where do they live in the split? Leaning: simple list in `[system].components`; per-component QoS lives in component crate's `[package.metadata.nros.component]`. Cross-cutting overrides go in `[[deploy.*]]`. §4.
7. **Mixed-language workspace bootstrap.** First-time user runs `cargo build` against a workspace containing a C++ component pkg — what happens? Cargo ignores non-Cargo dirs. User must know to `cmake -S . -B build` instead. Onboarding friction. Options: (a) document, (b) generate a top-level `Makefile` shim, (c) `nros build` (rejected by constraint 4). Leaning (a) — honest. §6.3.
8. **Codegen interface package shape.** Where does `my_interfaces/` (a `.msg`-only package) sit? Today: `packages/interfaces/<pkg>/` w/ codegen via `nros generate-rust`. In multi-pkg workspace: sibling `my_interfaces/` pkg w/ `package.xml` declaring `<member_of_group>rosidl_interface_packages</member_of_group>`? Component crates `cargo:rerun-if-changed=` against it. Not yet sketched.
9. **Embedded MCU + multi-pkg workspace.** Multi-component on Zephyr: does each component get its own `west` app, or one app composing multiple components via Kconfig? Phase 172.K.5 (per-node multi-domain routing) suggests one-app-N-components. Need pattern check w/ §7's launch step.

---

## 9. Rejected alternatives (so far)

- **Colcon inner loop.** Error attribution, embedded coverage, install/ overhead. §2 constraint 2.
- **`nros build` / `nros test` / `nros flash`.** Re-creates colcon's wrapping anti-pattern; hides cargo/cmake diagnostics. §2 constraint 4.
- **Single workspace-root `nros.toml` w/ `[system.<name>]` sub-tables.** Re-creates colcon monorepo-of-unrelated-systems pattern; breaks per-system `<exec_depend>` hygiene. §5.
- **Bringup pkg ships `CMakeLists.txt` + empty `install(DIRECTORY launch ...)`.** Drags ament_cmake into a pure-Rust workspace for zero benefit. nros reads `launch/` from source. §4.
- **`find_package(NanoRos)` consumption.** Already deleted Phase 140. Confirmed not coming back. Consumption stays `add_subdirectory(<repo-root>)`. §6.2.
- **Plain `<system>` naming (no `_bringup` suffix).** Collides w/ ament metapackage idiom. Forces awkward `<exec_depend>demo</exec_depend>` reading. §4.

---

## 10. Next concrete steps

1. **Prototype 3-package fixture** at `packages/testing/nros-tests/fixtures/multi_pkg_workspace/`: `demo_bringup` + `talker_pkg` + `listener_pkg`. Path A (no Cargo.toml in bringup). Run `cargo build` + `cargo nros plan demo_bringup` + `nros deploy native`. Confirm cargo workspace happy w/ excluded pkg.
2. **Spike `nros emit package-xml`** from `system.toml`. Validate against colcon-outer workflow (run `colcon build` on the fixture inside a host ROS 2 install). Confirms Autoware-style outer integration unbroken.
3. **Spike mixed-language fixture**: `talker_pkg` (Rust) + `listener_pkg` (C++) + `demo_bringup`. Top-level `CMakeLists.txt` + `corrosion_import_crate`. Confirm rustc errors still reach terminal verbatim through cmake.
4. **Resolve OPEN 3** (`nros launch`). Prototype host-side launcher reading `system.launch.xml` w/o ament index. If clean, retire `<buildtool_depend>` from bringup pkg.
5. **Document `cargo nros plan <dir>` discovery semantics** once Path A vs B settled. Update Phase 212.B writeup.
6. **Validate OPEN 9 (embedded multi-component)** on Zephyr w/ a 2-component bringup → one west app linking both. Phase 172.K.5 generator output should already cover this; confirm.
7. **Update `docs/design/ros2-user-workflow.md` §"nros new system"** scaffolding to match §4 LOCKED shape (Path A, no Cargo.toml in bringup). Today's writeup pre-dates this design doc.
