# Phase 224 — pure C/C++ multi-pkg workspace workflow review

**Renumbered 2026-06-04** from `phase-219-workflow-review.md` to its own
slot now that Phase 219 (C/C++ Entry pkg) has landed. Originally written
as the verification artifact for Phase 219's plan; preserved here as the
canonical record of the 7-gap walkthrough that drove 219.H/I/J/K/L/M.

**Goal.** Verify Phase 219's claim ("only Entry is Rust-biased; Node + Bringup
are language-symmetric") by walking the end-to-end pure-C/C++ workflow against
the canonical §11.2 layout. Native (POSIX) only; embedded out of scope per
212.L.2.

**TL;DR.** Phase 219's diagnosis is **half-right**. The Entry-pkg codegen is
indeed the headline Rust-bias gap. **But it is not the only gap.** Three more
load-bearing gaps block a user from copy-pasting the §11.2 layout for
C/C++ today, all sitting BELOW the codegen surface (cmake-fn level — cheap to
fix). One additional gap sits in the `nros` CLI scaffold (`nros new --component
--lang cpp` rejected). The Rust path papers over all of these because
proc-macro + cargo workspace handle them implicitly.

---

## 1. Procedure followed

1. Read `docs/design/multi-node-workspace-layout.md` §11 +
   `docs/roadmap/phase-219-cpp-entry-pkg.md` + the three cmake fn modules +
   `node_pkg.hpp` / `node_pkg.h`.
2. Read existing in-tree C++ examples (single-Node native; multi-Node nav2-
   style FreeRTOS).
3. Inspected `nros` CLI: `plan`, `metadata`, `new --component`.
4. **Constructed a 2-Node pure-C++ workspace** under `/tmp/cpp_ws/`:
   `talker_pkg`, `listener_pkg`, `demo_bringup`, `cpp_entry` per §11.2.
5. Ran `cmake -S . -B build` against three workspace-root cmake variants.
   Captured each error class and minimal workaround.

---

## 2. Per-role gap matrix

| Role | Author files | Build-system call | Produces | Gap today? |
|---|---|---|---|---|
| **Node pkg** | `package.xml`, `CMakeLists.txt`, `src/Foo.cpp` with `NROS_NODE_REGISTER(...)` | `nano_ros_node_register(NAME CLASS SOURCES DEPLOY)` | STATIC lib `<pkg>_<name>_component` + per-pkg mangled register sym + JSON fragment in `${CMAKE_BINARY_DIR}/nros-metadata.json` | **Works** standalone. Breaks in workspace-root context due to gaps 3 + 4 below. |
| **Bringup pkg** | `package.xml`, `system.toml`, `launch/*.launch.xml`. NO CMake. | None (declarative). Consumed by `nros plan`. | n/a — read at plan time. | `package.xml` walk works; **`nros plan` requires external `play_launch_parser` Python tool on PATH** (gap 6). |
| **Entry pkg** | `package.xml`, `CMakeLists.txt`, `src/main.cpp` | `nano_ros_entry(NAME SOURCES BOARD DEPLOY)` | executable + JSON fragment | **Works for single-Node self-bringup.** No `LAUNCH` arg yet — user writes `main()` by hand, declares + calls each Node pkg's register sym by hand, hand-`target_link_libraries` each `<pkg>_<name>_component` static lib. **This is the Phase 219 headline gap.** Plus gap 5 (link-aggregation) which 219 also does NOT call out. |

---

## 3. Gap catalogue (workspace-root + top-level cmake)

### Gap 1 — No documented canonical workspace-root `CMakeLists.txt` shape

§11.2 shows the layout tree but **does not specify the body** of the top-
level `CMakeLists.txt`. Questions left unanswered:

- Does the workspace-root cmake call `nano_ros_workspace_metadata(SYSTEM <bringup>)`?
- Does it `add_subdirectory(<nano-ros>)` once at root, or do subdir Node pkgs
  do it themselves?
- Does it `include(NanoRosNodeRegister.cmake)` once, or each subdir does?
- Does it auto-include subdir `CMakeLists.txt`s, or does the user list every
  `add_subdirectory(src/<pkg>)` by hand?

No template, no in-tree fixture, no book chapter. Result: user must invent it.

### Gap 2 — `add_subdirectory(nano-ros)` collides on re-include

When **each** Node pkg subdir does `add_subdirectory(<nano-ros-root> nano_ros)`
(the per-pkg standalone shape advertised in `examples/native/cpp/talker/`),
the root build fails:

```
add_library cannot create target "nros_rmw_cffi_headers" because another
target with the same name already exists.
[...]
Corrosion: Failed to import Rust crate nros_c [...]
```

**Workaround required**: every Node-pkg / Entry-pkg `CMakeLists.txt` must
guard the include:

```cmake
if(NOT TARGET NanoRos::NanoRos)
    set(NANO_ROS_PLATFORM posix)
    set(NANO_ROS_RMW zenoh)
    add_subdirectory("<nano-ros-root>" nano_ros)
endif()
if(NOT COMMAND nano_ros_node_register)
    include("<nano-ros-root>/cmake/NanoRosNodeRegister.cmake")
endif()
```

Today's `examples/native/cpp/talker/CMakeLists.txt` (single-Node) does NOT
have these guards because it never gets included from above. The
`templates/multi-package-workspace/` template predates Phase 212 and uses
standalone-build-per-pkg with no workspace root at all.

**Cost to fix: cheap** — add the `if(NOT TARGET …)` guards to the root
`CMakeLists.txt` (already partly defensive) and document the canonical
workspace-root + subdir pattern. Or land a `nano_ros_workspace(...)` cmake fn
that does the `add_subdirectory` once and exposes `nano_ros_add_pkg(<dir>)`
for subdir registration.

### Gap 3 — `nros_find_interfaces` / `nros_generate_interfaces` collide across sibling Node pkgs

Two Node pkgs that both `<depend>std_msgs</depend>` and each call
`nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)` fail the second call:

```
add_custom_target cannot create target "std_msgs__nano_ros_cpp_gen" because
another target with the same name already exists.
[...]
add_library cannot create target "std_msgs__nano_ros_cpp" because another
target with the same name already exists.
```

Inside `cmake/NanoRosGenerateInterfaces.cmake`, only `builtin_interfaces`,
`unique_identifier_msgs`, `action_msgs` have `if(NOT TARGET …)` guards
(lines 282-290). Every other interface pkg the user `<depend>`s on
(`std_msgs`, `geometry_msgs`, `sensor_msgs`, …) hits the collision.

**Workaround required**: only ONE TU in the build graph may call
`nros_find_interfaces`/`nros_generate_interfaces` per interface pkg. In a
multi-Node workspace, that means lifting the calls into the workspace-root
`CMakeLists.txt`, which has no `package.xml` of its own and therefore can't
use `nros_find_interfaces` (which reads `${CMAKE_CURRENT_SOURCE_DIR}/package.xml`).
User must drop down to raw `nros_generate_interfaces(<pkg> …)` calls and
hand-resolve dep order — exactly the layered `find_package`-chain workflow
that Phase 210.E.4 was supposed to retire.

**Cost to fix: cheap** — generalise the `if(NOT TARGET <pkg>__nano_ros_<lang>)`
guard inside `nros_generate_interfaces` so the second caller becomes a no-op
that just registers the dep. Or add an aggregate `nano_ros_workspace_interfaces()`
fn that walks every Node pkg's `package.xml` once at root scope.

### Gap 4 — Entry pkg does NOT auto-link Node-pkg static libs

`nano_ros_entry(NAME … SOURCES … DEPLOY native)` produces the executable
but does NOT `target_link_libraries(<exe> PRIVATE <pkg>_<name>_component)`
for the Node-pkg static libs. User writes:

```cmake
nano_ros_entry(NAME cpp_entry SOURCES src/main.cpp BOARD native DEPLOY native)
target_link_libraries(cpp_entry PRIVATE
    talker_pkg_talker_component
    listener_pkg_listener_component)
```

`cpp_entry` must know its sibling Node pkgs' target names. This is the
metadata Phase 219 §3.3 generates for the call sequence — the same
metadata could drive `target_link_libraries`. **Phase 219 doesn't mention
this.** Without it, the LAUNCH-driven codegen TU compiles but does not
link (undefined `__nros_component_<pkg>_register`).

**Cost to fix: cheap** — `nano_ros_entry` already reads the LAUNCH-XML
node list (post 219.D). The same loop emits both the extern-C decl in the
generated TU AND a `target_link_libraries(... PRIVATE
<pkg>_<comp>_component)`. Driven by `nros-metadata.json`'s `components[]`
filtered by the launch-XML `<node pkg=...>` set.

### Gap 5 — `nros plan` depends on external `play_launch_parser` Python tool

`nros plan demo_bringup launch/system.launch.xml` fails on a stock dev
machine with:

```
Error: failed to run `play_launch_parser` (launch parser) [...]
Install it (`pip install play-launch-parser`, or build the play_launch_parser
binary) and put it on PATH, or set NROS_PLAY_LAUNCH_PARSER=<path>.
```

Phase 219.D's "shell `nros codegen entry`" path inherits this dependency.
The Rust proc-macro path does NOT (it walks launch XML in-process via the
nros-build crate's `play_launch_parser` source dep — see
`nros-cli-core/Cargo.toml` line 34). **This is the second Rust-bias.**

**Cost to fix: medium** — either (a) statically link the
`play-launch-parser` crate into the `nros` CLI binary (already a feature flag
`play-launch-parser` per planner.rs:2574 — flip it on), or (b) ship the
Python wheel via `nros setup`, or (c) bundle the binary in the
prebuilt `nros` release. (a) is cleanest.

### Gap 6 — `nros metadata` does NOT discover C++ Node pkgs via `package.xml` walk

`nros metadata --workspace /tmp/cpp_ws demo_bringup` returns:

```
nros metadata: preserved 0 metadata artifact(s)
```

The workspace walker in `nros-cli-core/src/orchestration/workspace.rs`
finds the `package.xml` files (line 99), but `component_declarations()`
only yields packages with an `nros.toml [component]` table (lines 141-166).
Pure-C++ Node pkgs declared by `nano_ros_node_register()` in cmake have
NO `nros.toml`. They show up in `nros-metadata.json` only AFTER cmake
configure runs (which writes the JSON itself).

**Implication:** the `nros plan` → resolve `<node pkg=talker_pkg exec=talker>`
flow has no way to know `talker_pkg` is a C++ Node pkg unless cmake
configure has already populated `nros-metadata.json`. Today's flow
hard-couples: cmake configure produces metadata → `nros plan` reads
metadata → codegen consumes plan. The Entry pkg codegen has to run
INSIDE the same cmake configure pass that produced the metadata, which
Phase 219.D assumes implicitly but does not call out.

**Cost to fix: medium** — extend `Workspace::component_declarations()`
to ALSO walk `package.xml` + look for sibling `CMakeLists.txt` calling
`nano_ros_node_register`. Or formalise the contract: cmake-only Node
pkgs require a one-shot `nros metadata --scan-cmake` pass that snapshots
the cmake-emitted metadata under workspace state.

### Gap 7 — `nros new --component --lang cpp` is rejected outright

```
$ nros new node_cpp --lang cpp --component --use-case talker
Error: `nros new --component` scaffolds a Rust component;
       --lang cpp is not yet supported
       Location: nros-cli-core/src/cmd/new.rs:116:13
```

`nros new <name> --lang cpp` (without `--component`) succeeds, but emits
**broken** scaffold:

```cmake
# Generated /tmp/.../talker_cpp/CMakeLists.txt
find_package(NanoRos REQUIRED CONFIG)   # <-- Phase 140 deleted this path
```

CLAUDE.md is explicit: "**There is no `find_package(NanoRos)` path** —
Phase 140 deleted it along with `just install-local`, the `build/install/`
layout, every `install(...)` rule and every `Config.cmake.in` template."
Combined with the `install(TARGETS ... DESTINATION lib/...)` rule in the
scaffold, the scaffolded project cannot configure.

`src/main.cpp` is also stub-shaped — `printf("Hello from talker_cpp!\n")` —
no `nros::Executor`, no `NROS_NODE_REGISTER`, no Talker class. The
scaffold is not aligned with Phase 212 at all.

**Cost to fix: cheap-to-medium** — the `nros-cli` scaffold generator
needs templates for `cpp` Node-pkg + `cpp` Entry-pkg shapes that emit
the §11.2 layout with the `add_subdirectory(<nano-ros>)` shape.

---

## 4. Concrete repro

### 4.1 Smallest pure-C/C++ workspace that DOES configure today

`/tmp/cpp_ws/` after applying all four gap workarounds:

```
/tmp/cpp_ws/
├── CMakeLists.txt                # workspace-root — calls nros_generate_interfaces at top
└── src/
    ├── talker_pkg/
    │   ├── package.xml           # <depend>std_msgs</depend>
    │   ├── CMakeLists.txt        # if(NOT TARGET NanoRos::NanoRos) guard; nano_ros_node_register(...)
    │   └── src/Talker.cpp        # NROS_NODE_REGISTER(talker_pkg::Talker, "talker_pkg::Talker")
    ├── listener_pkg/             # same shape
    ├── demo_bringup/             # package.xml + system.toml + launch/system.launch.xml
    └── cpp_entry/
        ├── package.xml
        ├── CMakeLists.txt        # nano_ros_entry(...) + MANUAL target_link_libraries(...)
        └── src/main.cpp          # HAND-WRITTEN main() — Phase 219.D would generate this
```

Workspace-root `CMakeLists.txt` body that works:

```cmake
cmake_minimum_required(VERSION 3.22)
project(cpp_ws LANGUAGES C CXX)
set(NANO_ROS_PLATFORM posix)
set(NANO_ROS_RMW zenoh)
add_subdirectory("/path/to/nano-ros" nano_ros)
include("/path/to/nano-ros/cmake/NanoRosNodeRegister.cmake")
# Workaround for Gap 3 — generate every interface pkg at root scope.
nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
add_subdirectory(src/talker_pkg)
add_subdirectory(src/listener_pkg)
add_subdirectory(src/cpp_entry)
```

`cmake --configure` succeeds. `nros-metadata.json` is correctly populated with
both Node pkgs + the Entry pkg. **`cmake --build build` was not attempted in
this review** — the Entry pkg's hand-written `main()` does not call into the
Node pkgs' register fns, so even a successful link would be a no-op runtime.
A real Phase 219 fix lands the codegen TU; build correctness needs to be
re-validated then.

### 4.2 Smallest pure-C/C++ workspace that does NOT configure today

The same layout, with subdir CMakeLists matching the in-tree
`examples/native/cpp/talker/CMakeLists.txt` pattern verbatim
(each subdir does `add_subdirectory(<nano-ros>)` itself; root delegates).
Fails on Gap 2 (duplicate-target collisions on `nros_rmw_cffi_headers`,
Corrosion crate conflict, `std_msgs__nano_ros_cpp` etc.).

`tmp/cpp_ws_broken/` would be the gap-fixture (not authored — same file
tree as §4.1 with the workarounds reverted).

---

## 5. Recommended Phase 219.* sub-items beyond the design doc

Phase 219 as written lands 219.A-G. Below are the items needed **before or
alongside** 219.D for a pure-C/C++ workspace user to copy §11.2 and have
it work. Each is cmake-fn-level (cheap) unless flagged.

### 219.H (NEW, cheap) — Idempotency guards in interface codegen

Generalise `if(NOT TARGET ${_pkg}__nano_ros_${_lang})` guards inside
`nros_generate_interfaces` so every call past the first for a given
interface pkg becomes a no-op (today: only `builtin_interfaces`,
`unique_identifier_msgs`, `action_msgs` are guarded). Closes Gap 3.

Files: `cmake/NanoRosGenerateInterfaces.cmake` lines ~282-290 (extend the
pattern), ~462, ~471, ~607.

### 219.I (NEW, cheap) — `nano_ros_workspace()` cmake fn + workspace-root canon

Land a workspace-root cmake fn that:

1. Pulls `add_subdirectory(<nano-ros>)` exactly once.
2. Includes `NanoRosNodeRegister.cmake` exactly once.
3. Calls `nano_ros_workspace_metadata(SYSTEM <bringup>)` (already exists).
4. Walks `src/*/package.xml` and `add_subdirectory()`s every Node + Entry
   pkg in topo order.
5. Generates the union of `<depend>` interface pkgs once at root scope.

Subdir `CMakeLists.txt`s become `nano_ros_node_register(...)` one-liner +
`project()` declaration; no `add_subdirectory(<nano-ros>)` needed. Closes
Gap 1 + Gap 2.

Files: new `cmake/NanoRosWorkspace.cmake`.

### 219.J (NEW, cheap) — Entry pkg auto-links Node-pkg components from metadata

Once 219.D parses the launch XML and emits the generated TU, the same
fn knows which Node pkgs are involved. Emit a
`target_link_libraries(<entry> PRIVATE ${_pkgs})` from the same data,
where `${_pkgs}` is each `<pkg>_<comp>_component` target read out of
`nros-metadata.json`. Closes Gap 4.

Files: `cmake/NanoRosEntry.cmake` body extension.

### 219.K (NEW, medium) — Static-link `play-launch-parser` into `nros` CLI

The `play-launch-parser` feature flag already exists in
`nros-cli-core/Cargo.toml` (planner.rs:2574 `#[cfg(feature =
"play-launch-parser")]`). Enable it in the prebuilt release. Or — if the
crate is upstream / external-only — vendor it and link statically. Closes
Gap 5 (and removes a Python dep from the C++ workflow's runtime
prereqs).

Files: `nros-cli` build manifest + release recipe.

### 219.L (NEW, medium) — `nros metadata` walks cmake-only Node pkgs

Either:

- (a) Extend `Workspace::component_declarations()` to synth a virtual
  declaration from `package.xml` + presence of `nano_ros_node_register`
  call in `CMakeLists.txt` (regex / cmake-file-api parse).
- (b) Formalise a one-shot `nros metadata --scan-cmake` that runs cmake
  configure in a scratch tree and slurps the resulting
  `nros-metadata.json`.

(b) is cleaner — cmake is the SSoT for C/C++ build config anyway.
Closes Gap 6.

Files: `nros-cli-core/src/cmd/metadata.rs` + `orchestration/workspace.rs`.

### 219.M (NEW, cheap) — Scaffold fixes

- Allow `nros new --component --lang cpp` (today: hardcoded error at
  `nros-cli-core/src/cmd/new.rs:116`).
- Replace the broken `nros new --lang cpp` scaffold's `find_package(NanoRos
  REQUIRED CONFIG)` with the `add_subdirectory(<nano-ros>)` shape (or, post
  219.I, the new `nano_ros_workspace()` fn one-liner).
- Make the scaffold emit a real Node-pkg `src/Talker.cpp` instead of a
  hello-world stub.

Closes Gap 7.

Files: `nros-cli-core/src/cmd/new.rs` + scaffold templates.

### 219.N (NEW, cheap) — In-tree multi-pkg C++ fixture under `examples/`

Today's `examples/native/cpp/*` are all single-Node self-bringup, and
`examples/templates/multi-package-workspace/` is the pre-Phase-212
shape (each pkg has its own `main()`, no Entry pkg, no Bringup pkg).

Land `examples/native/cpp/multi-node-entry/` (Phase 219.F already lists
this for the Rust-parity test) with the §11.2 shape: 2 Node pkgs + 1
Bringup pkg + 1 Entry pkg + workspace-root `CMakeLists.txt`. Also
exercise it from a nextest fixture so the gap doesn't regress.

Files: `examples/native/cpp/multi-node-entry/`,
`packages/testing/nros-tests/tests/cpp_multi_node_entry.rs`.

---

## 6. Conclusion

Phase 219's headline claim — "Node + Bringup are language-symmetric; only
Entry is Rust-biased" — is **correct at the per-role API level** (the
register-fn ABI, mangle scheme, `package.xml` + `system.toml` schema all
work uniformly). But **integration-level** the C/C++ path has four
additional gaps (Gaps 2-5 + 7) that the Rust path papers over via
proc-macro + cargo workspace + shared `nros-build` crate.

Phase 219 should be expanded to include 219.H + 219.I + 219.J as
**prerequisites** for 219.D (the cmake-fn LAUNCH arg) and 219.F (the
multi-Node-entry fixture). Without them, 219.D delivers a generated
`main()` that fails to link, and 219.F has nothing reproducible to test.

219.K + 219.L + 219.M are **adjacent CLI-side** fixes; they could land in
parallel with 219.A-C (the codegen module split) without crossing into
219.D's cmake-fn scope.

Order-of-operations recommendation:

```
219.H + 219.I  (cmake-fn-level idempotency + workspace-root canon)
  → 219.A-C    (CLI codegen module split; 219 as-designed)
    → 219.D-E (cmake fn LAUNCH arg + headers; 219 as-designed)
      → 219.J  (auto-link Node components from launch metadata)
        → 219.F  (multi-Node native fixture; 219 as-designed)
          → 219.G  (book chapter; 219 as-designed)
219.K, 219.L, 219.M   — in parallel with 219.A-G, CLI-side cleanup.
```

All seven gaps are at the **cmake-fn + CLI-glue layer**, not at codegen.
None requires a new runtime symbol, board ABI change, or RMW work.
Total work below the codegen surface is modest (low hundreds of LoC
across cmake + Rust); the codegen itself (219.A-C/D-E) is the bulk.
