---
rfc: 0024
title: "Multi-Node Workspace Layout: the rationale that still holds"
status: Stable
area: workspace-system-model
since: 2026-06
last-reviewed: 2026-11-15
implements-tracked-by: [phase-212]
supersedes: []
superseded-by: null
---

# Multi-Node Workspace Layout: the rationale that still holds

## 1. Status & Audience

**Stable, cut down (phase-555 W3, ruled 2026-11-14).** This RFC designed the
phase-212 workspace layout. [RFC-0121](0121-workspace-first-project-layout.md)
now owns the layout: every project is a colcon workspace, `system.toml`
lives only in a bringup, and the entry is generated under `build/`
([RFC-0065](0065-colcon-like-workspace-builder.md) D1, RFC-0121 D6). RFC-0121
amends this page. Its companion, RFC-0025 (the file tree per case), is
superseded by RFC-0121 and [archived](archived/0025-workspace-layout-by-case.md).

What stays here is the rationale that code and other RFCs cite, under its
original section numbers so those citations still resolve:

- §2, the constraints, as amended (`builder/handoff.rs` guards §2.4);
- §3, the reference patterns;
- §4, the bringup package's rules (`cmd/check_workspace.rs`,
  `cmd/bringup.rs`);
- §5, the workspace root as a thin pointer;
- §6.3, "cmake wins when the graph crosses languages" (`builder/plan.rs`);
- §6.4, the embedded path's crate types;
- §9 and §11.9, the rejected alternatives;
- §11.1, §11.3, §11.5, §11.6 and §11.8: the package roles, multiple launch
  files, launch XML verbatim, the Rust macro surface and the custom spin
  loop.

Sections whose content RFC-0121 replaced keep their heading and a pointer.
The full phase-212 text is in git history (before 2026-11-15).

Every §8 open question is resolved or filed; §8 says which.

---

## 2. Constraints (locked)

1. **ROS standard layout.** Launch files live in dedicated orchestration package (`<system>_bringup` convention). Component packages stay code-only.
2. **No colcon as primary orchestrator.** Error attribution (rustc/gcc diagnostics swallowed by `Failed <<<`), embedded targets ignored, install/ tree dead weight for MCUs, cross-language codegen invisible to colcon DAG. Colcon stays *available* for Autoware-style outer integration via `colcon-cargo-ros2` seam; never the inner workflow.
3. **cargo / cmake stay user-facing.** `cargo build` works at workspace root for Rust-majority. `cmake --build build` works for C++-majority. Rustc errors stay rustc errors.
4. **nros never a build verb.** No `nros build` / `nros test` / `nros flash`. nros = provisioner + codegen + metadata. Idf.py-shaped, not colcon-shaped.
   > **AMENDED 2026-08-25 by [RFC-0065](0065-colcon-like-workspace-builder.md) §D1.** The
   > objection recorded in §9 is to *wrapping the compiler* — "hides cargo/cmake
   > diagnostics" — not to *deriving the root build file*. `nros build` returns with two
   > properties that keep §2.2 and §2.3 intact: its final stage is an **`exec`**, not a
   > pipe, so diagnostics are byte-identical to the native tool's; and what it generates
   > is a **real build tree** a user can drive by hand thereafter. `nros test` and `nros
   > flash` stay rejected — neither has a derivation to perform, so both would be pure
   > wrapping.
5. **One-package workflow stays canonical for tiny fixtures.** Multi-package shape kicks in at ≥2 components. (Phase 212 already decided single-package workflow user-facing.)

---

## 3. Reference patterns

- **nav2 / Autoware / turtlebot3** — `<system>_bringup` carries `launch/`, `config/`, `package.xml` with only `<exec_depend>` lines pulling in component packages. No `<build_depend>` — orchestration role is pure runtime resolution. **Takeaway:** orchestration package is a *role*, not a build artifact. Zero compiled code.
- **Cargo workspace** — single dependency graph, unified feature DAG per (package, target), `-j N` rayon scheduler. Build-scripts are crates-first. **Takeaway:** cargo IS the orchestrator for Rust; do not wrap it.
- **CMake `add_subdirectory` vs `ExternalProject_Add`** — former shares cache + targets, latter isolates. **Takeaway:** own-code uses `add_subdirectory`; corrosion provides bidirectional cargo↔cmake bridge.
- **Zephyr west + ESP-IDF idf.py + PlatformIO** — all share shape: SSoT manifest at root (`west.yml` / `platformio.ini` / `idf` component scan) + per-component manifest (`module.yml` / `library.json` / `idf_component_register`). Tool synthesizes one CMake/SCons graph at invocation. **Takeaway:** root manifest + per-package manifest is the dominant pattern; nros.toml follows it.
- **Corrosion** — already used Phase 175.A. `corrosion_import_crate` makes Rust staticlib a normal CMake target. **Takeaway:** when graph crosses languages, CMake should be the top-level driver because cargo cannot consume CMake targets in reverse.

Citations: `docs/design/archived/0027-ros2-user-workflow.md` (superseded by RFC-0121), `docs/roadmap/archived/phase-212-ux-cargo-native-and-file-consolidation.md` (lines 473–731), `packages/cli/colcon-cargo-ros2/`, `CLAUDE.md` Examples + CMake Path Convention sections.

---

## 4. The orchestration package

**Shape.** A bringup package is declarative: `package.xml` + `system.toml`,
optionally `launch/` and `config/`. **No `Cargo.toml`. No `CMakeLists.txt`.
No `src/`.** RFC-0121 D2 restates this and makes it the only home of
`system.toml`. The per-target tables are `[image.<id>]` and `[host.<name>]`;
`[deploy.<target>]` is refused by name since phase-486 W8.

**Naming: `<system>_bringup`.** Aligns nav2/Autoware/turtlebot3. Accept `<system>_launch` as documented alias. Reject plain `<system>` (collides w/ ament metapackage idiom).

**Dependencies — two layers, both mandatory:**
- `package.xml` `<exec_depend>` → ament/colcon discovery + install ordering when used inside outer-colcon workspace.
- `system.toml` `[[component]]` → nros planner's authoritative runtime set.

`nros check` cross-validates the two. Single source = `system.toml`; `nros emit package-xml` regenerates `<exec_depend>` block.

**Lint:** `nros check` rejects orchestration pkg w/ `[[bin]]`/`[lib]`/`add_executable`. Code goes in sibling component pkg.

The two questions this section once left open are settled: the bringup has
no `Cargo.toml` (Path A; the workspace never lists it as a member), and its
`package.xml` omits `<buildtool_depend>` (§11.1, phase 212.J.5).

---

## 5. The workspace root

**Workspace root = thin pointer, not a system definition.** The tree this
section drew (a hand-written workspace-root `Cargo.toml` /
`CMakeLists.txt`) is gone: `nros build` generates the root build file under
`build/` (RFC-0065 D1, RFC-0121 D6), and
`check-workspace-root-build-files` refuses one under
`examples/{workspaces,templates}`.

**No system definition in a workspace-root `nros.toml`.** The system lives in `<bringup>/system.toml`. A root `nros.toml` holds only facts about the whole workspace, such as the package search path and provider overrides (RFC-0121 D2). Rationale: matches ROS muscle memory (every nav2/Autoware tutorial points users at `nav2_bringup/launch/`, not a root TOML). Decouples workspace's build graph from the system graph.

**No duplication.** Root pointer + per-system definition is two different concerns. The temptation to mirror per-system fields at root (e.g. `[workspace.metadata.nros.system.demo]`) is rejected — re-creates colcon's monorepo-of-systems pattern + breaks per-system `<exec_depend>` hygiene.

Whether two bringups can share a `system.toml` fragment is still open, and
duplication stays the answer until a workspace needs more:
[issue 2587](../issues/2587-multi-bringup-shared-system-fragments.md).

---

## 6. Build orchestration without colcon

### 6.1 Rust-majority workspace (cargo top-level)

Replaced: `nros build` derives the build tree and `exec`s cargo, so
diagnostics are cargo's own (§2.4 as amended; RFC-0065 D1).

### 6.2 C++-majority workspace (cmake top-level)

Replaced the same way, with cmake as the tool `nros build` hands off to
([RFC-0130](0130-one-ros-cmake-build.md)).

### 6.3 Mixed Rust + C++ workspace

**CMake is the top-level driver.** Rule: cargo can be consumed as a cmake target (via Corrosion); cmake cannot be consumed as a cargo target. So when graph crosses languages, cmake wins:

(Holds on boards whose startup is Rust too: there the Corrosion staticlib also carries the board's startup and CMake still owns the link, [RFC-0130](0130-one-ros-cmake-build.md) D2.)

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

Whether `system.toml` is baked into firmware is settled: it is not. The
resolved SystemModel is a build artifact the image is generated from
([RFC-0063](0063-system-model-is-a-build-artifact.md)), and an MCU image carries only what codegen bakes.

**Node-pkg `crate-type` is deployment-path-specific (corrected 2026-06-12, [RFC-0032 §3.1](0032-entry-codegen-pipeline.md), [issue 0045](../issues/archived/0045-freertos-entry-component-staticlib-panic-handler.md)).** An earlier draft listed `crate-type = ["rlib", "staticlib"]` as an *irreducible* Node-pkg item; that conflated two build paths:

- **Pure-cargo Entry path** (Rust-native FreeRTOS / NuttX / ThreadX) — the Entry pkg links the Node as an **rlib** only. The Node declares `crate-type = ["rlib"]`. A redundant `staticlib` here is dead weight that still forces rustc to demand a no_std `#[panic_handler]` the rlib must not carry (it would collide with the board-owned handler — RFC-0032 §3.1).
- **cmake / Corrosion path** (C-owned firmware, RFC-0003 bake) — the C firmware links the Node's **staticlib**. The Node declares `crate-type = ["staticlib"]` (the threadx fixtures already do, host-linked). Corrosion 0.5 `CRATE_TYPES` only selects from *declared* types, so this path must declare the staticlib.

A Node reused across both paths needs both declarations split by path (e.g. per-target manifest or a thin staticlib wrapper) — it is **not** a single universal `["rlib","staticlib"]`.

---

## 7. End-to-end user workflow

Replaced by RFC-0121 and the book: `nros new` makes a workspace with a
bringup, and `nros build` builds an image from it.

---

## 8. Open questions

All nine are closed:

1. Bringup `Cargo.toml`: none (Path A), §4.
2. Multi-system shared config: open, filed as
   [issue 2587](../issues/2587-multi-bringup-shared-system-fragments.md), §5.
3. `nros launch` vs `ros2 launch`: the bringup omits `<buildtool_depend>`,
   §11.1; the generated entry binary is the launch product.
4. C++ workspaces: resolved, cmake through `nros build` (RFC-0130).
5. Where `system.toml` lives: the bringup only (RFC-0121 D2).
6. `[system].components` schema: `[[component]]` tables.
7. Mixed-language bootstrap: CMake drives, Corrosion imports Rust, §6.3.
8. Interface packages: a `rosidl` message package is a package kind of its
   own (RFC-0121 D1); message deps are path deps
   ([RFC-0067](0067-env-invariant-msg-dep-identity.md)).
9. Embedded multi-component: one image links N node packages and one
   generated entry ([RFC-0015](0015-rtos-orchestration.md)).

---

## 9. Rejected alternatives (so far)

- **Colcon inner loop.** Error attribution, embedded coverage, install/ overhead. §2 constraint 2.
- **`nros build` / `nros test` / `nros flash`.** Re-creates colcon's wrapping anti-pattern; hides cargo/cmake diagnostics. §2 constraint 4. **`nros build` re-admitted 2026-08-25 as a generate-then-`exec` builder — see the amendment note on §2 constraint 4 and [RFC-0065](0065-colcon-like-workspace-builder.md). `test` / `flash` remain rejected.**
- **Single workspace-root `nros.toml` w/ `[system.<name>]` sub-tables.** Re-creates colcon monorepo-of-unrelated-systems pattern; breaks per-system `<exec_depend>` hygiene. §5.
- **Bringup pkg ships `CMakeLists.txt` + empty `install(DIRECTORY launch ...)`.** Drags ament_cmake into a pure-Rust workspace for zero benefit. nros reads `launch/` from source. §4.
- **`find_package(NanoRos)` consumption.** Already deleted Phase 140. Confirmed not coming back. Consumption stays `add_subdirectory(<repo-root>)`. §6.2.
- **Plain `<system>` naming (no `_bringup` suffix).** Collides w/ ament metapackage idiom. Forces awkward `<exec_depend>demo</exec_depend>` reading. §4.

---

## 10. Next concrete steps

Done or overtaken; phase-212 is archived and RFC-0121's phase-531 carries
the layout work.

---

## 11. LOCKED 2026-06-03 — Three-pkg-role shape + ROS 2 launch compat

Driven by two user constraints:
**(a) ROS 2 launch.xml from nav2/Autoware must copy-paste and Just Work;
(b) C++ Entry pkg support is available through CMake, so resolution must be
language-agnostic.**


### 11.1 Three pkg roles

| role | shape | required? |
|---|---|---|
| **Bringup pkg** | pure declarative: `package.xml` + `system.toml` + `launch/*.launch.xml` + `config/`. No Cargo.toml, no CMakeLists.txt. | Optional — only when ≥2 Entry pkgs share one topology |
| **Node pkg** | language-specific lib carrying `nros::node!()` (Rust) / `NROS_NODE()` (C++) declarations. `package.xml` + `Cargo.toml` (Rust) / `CMakeLists.txt` (C++). | Yes (one per node) |
| **Entry pkg** | language-specific binary that boots a topology against a Board. `package.xml` + `Cargo.toml` + `src/main.rs` with `nros::main!(...)` (Rust). C++ future: `CMakeLists.txt` with `nros_entry(...)` cmake fn + `main.cpp` with `NROS_MAIN(...)`. | Yes (one per deploy target) |

"Component pkg" terminology retires in favour of "Node pkg" — matches
ROS 2 composable-node naming.

**Phase 212.J.5 resolution (2026-06-03).** Bringup pkg `package.xml`
omits `<buildtool_depend>` since `nros launch` reads `launch/` from the
source tree, not from an install share path. Users wanting `ros2
launch <bringup>` compatibility add
`<buildtool_depend>ament_cmake</buildtool_depend>` themselves. The
`nros` CLI's generator (`nros emit package-xml`, internal helper) skips
the tag for the same reason; closes OPEN 3 in §10 and the §4 open
question.

Amended by RFC-0121: the bringup is no longer optional (D2), and the entry
package is generated under `build/` rather than written by the user (D6).


### 11.2 Canonical workspace layout

Replaced by RFC-0121 D1. Its "fold the bringup into the Entry pkg" for a
one-entry workspace is the shape RFC-0121 D2 refuses.

### 11.3 Bringup pkg with multiple launch files (nav2 convention)

`<system>_bringup/launch/` holds many `.launch.xml` files. Each is a
distinct topology entry point. `system.toml` carries
`[system] default_launch = "system.launch.xml"`. Users select with
`nros launch <bringup-pkg> [--launch <file>]` or with macro
`launch = "<bringup>:<file>"`. NOT split across `<sub>_launch` pkgs.

Naming: `<system>_bringup` default; `<system>_launch` accepted alias.
Matches nav2 / Autoware / turtlebot3.

### 11.4 Pkg discovery — workspace walk

Replaced by the workspace's package index (RFC-0121 D1 recognises each
package kind by what it holds).


### 11.5 Launch XML — ROS 2 schema verbatim

Tags supported at v1:

- `<launch>` — root.
- `<arg name="..." default="..." value="..." />` — launch arg.
- `<node pkg="..." exec="..." name="..." namespace="..." />` — spawn.
- `<param name="..." value="..." />` — per-node param.
- `<remap from="..." to="..." />` — topic/service remap.
- `<group ns="..." />` — namespace wrapper.
- `<include file="..." />` — recursive XML pull; args pass-through.

Substitutions supported v1:

- `$(find <pkg>)` — resolves to pkg source dir via the workspace pkg-
  index.
- `$(var <arg>)` — launch-arg reference.
- `$(env <name>)` — env lookup at build time.

Stock ROS 2 launch.xml from nav2 / Autoware / turtlebot3 paste in,
**Just Works** modulo unsupported tags. Python `.launch.py` form is
NOT supported v1; require XML.

### 11.6 Macro surface — Rust

```rust
// Single-node self-bringup — reads [package.metadata.nros.entry] deploy
nros::main!();

// Single-node, explicit board override
nros::main!(board = LinuxBoard);

// Multi-node, default launch from bringup pkg's system.toml
nros::main!(launch = "demo_bringup");

// Multi-node, explicit launch file
nros::main!(launch = "demo_bringup:sim.launch.xml");

// All explicit
nros::main!(
    board  = LinuxBoard,
    launch = "demo_bringup:sim.launch.xml",
    args   = [("use_sim", "true")],
);
```

One macro, four forms. Replaces today's `build.rs + include!()`
shape end-to-end — Entry pkg `Cargo.toml` drops the `nros-build`
build-dep, `main.rs` collapses to one line.

### 11.7 Macro surface — C++

Built since: a C/C++ entry is `nano_ros_entry(BRINGUP … LAUNCH …)`, and its
`main` is generated like a Rust one ([RFC-0130](0130-one-ros-cmake-build.md)).


### 11.8 Custom spin loop — escape hatch

When users want their own executor lifecycle, they skip
`nros::main!()` and write `main.rs` directly:

```rust
use nros::{Executor, ExecutorConfig, BoardEntry};
use nros_board_linux::LinuxBoard;

fn main() {
    let outcome = <LinuxBoard as BoardEntry>::run(|runtime| {
        runtime.runtime.register_dispatch_slot_dyn(
            __nros_component_talker_register,
            __nros_component_talker_init,
            __nros_component_talker_dispatch,
            __nros_component_talker_tick,
            "talker_pkg",
        )?;
        Ok(())
    });
    outcome.unwrap();
}
```

Or, even fully manual:

```rust
fn main() -> Result<(), Box<dyn Error>> {
    let executor = nros::Executor::open(&ExecutorConfig::default())?;
    let node = executor.create_node("my_talker")?;
    let publisher = node.create_publisher::<Int32>("/chatter", QoS::default())?;
    loop {
        publisher.publish(&Int32 { data: 0 })?;
        std::thread::sleep(Duration::from_secs(1));
    }
}
```

`nros::main!()` is convenience; `BoardEntry::run` + manual register
is the universal escape hatch; pure `Executor::open` is the rclcpp-
style direct path.

### 11.9 Rejected alternatives (this round)

- **Cargo-metadata-based pkg discovery.** Rust-only; would force a
  stub `Cargo.toml` on bringup pkgs; doesn't extend to C++. Replaced
  by language-agnostic workspace walk.
- **Workspace-root `[workspace.metadata.nros.bringups]` index.**
  Adding a bringup pkg = 2 edits (create dir + register in root
  table). Silent breakage on missed registration. Replaced by
  automatic `package.xml` walk.
- **Bringup pkg mandatory.** Over-prescription for single-target
  workspaces.
- **Custom launch XML schema.** Breaks copy-paste from
  nav2/Autoware/turtlebot3.
- **Python `.launch.py` v1.** Needs a Python interpreter at build
  time; revisit on demand.
- **Splitting launches across `<sub>_launch` sibling pkgs.**
  Un-ROS-2-idiomatic at our scale (nav2 et al. ship one bringup pkg
  with many launches inside).

"Bringup pkg mandatory" is no longer rejected: RFC-0121 D2 makes it the only
home of `system.toml`.


### 11.10 Work items (cross-refs to phase doc)

Done; phase-212 is archived.
