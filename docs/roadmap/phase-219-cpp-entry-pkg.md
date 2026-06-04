# Phase 219 — C / C++ Entry pkg support

**Goal.** Bring the C / C++ Entry-pkg path to parity with the Rust
`nros::main!()` flow landed in Phase 212.N.9. After this phase, a
`my_ws/src/native_entry_cpp/` Entry pkg with one CMakeLists.txt + one
`main.cpp` (and an optional `NROS_MAIN(...)` macro) wires a
multi-Node bringup against a Board with the same launch.xml +
workspace pkg-index semantics the Rust proc-macro already uses.

## 0. Reading guide

Three docs are in scope; read in order:

1. **`docs/design/multi-node-workspace-layout.md` §11** — LOCKED
   canonical three-role shape (Bringup + Node + Entry). Background.
2. **This file** — §1 + §2 + §3 sketch the surface; §4 lists every
   work item in landing order; §6 carries the acceptance bar.
3. **`docs/roadmap/phase-219-workflow-review.md`** — concrete
   2-Node pure-C++ workspace walkthrough that surfaced six
   integration gaps beyond the headline Entry-codegen gap. Each
   gap is anchored to a specific 219.X item; that doc is the
   verification artifact, this file is the plan.

**Status.** **DESIGN**, no code yet. Builds on Phase 212.N.6
(`nano_ros_entry(...)` cmake fn) + 212.N.10/11 (workspace pkg-index +
launch parser) + 212.M.5 (`__nros_component_<pkg>_register` mangled
symbols already exported by C and C++ Node pkgs).

**Priority.** P2 — Rust Entry path covers most internal users today;
C / C++ Entry parity unblocks ROS 2 migration cases that want to
keep the canonical `int main()` open-coded (rclcpp users) instead of
adopting a Rust binary as the boot harness for a C++ Node tree.

**Depends on.** Phase 212.N.6 (lands `nano_ros_entry`), 212.N.10
(pkg-index walker in nros-build), 212.N.11 (launch.xml parser in
nros-build), 212.M.5.a.1 (per-pkg mangled register symbol). All
already landed.

---

## 1. What works today (recap)

The node+entry workspace layout is locked in
`docs/design/multi-node-workspace-layout.md` §11. Three roles:

| Role | Today (Rust) | Today (C / C++) |
|---|---|---|
| **Node pkg** | `nros::node!(...)` Rust lib emits `__nros_component_<pkg>_register` | `NROS_NODE()` / `nros_node_pkg.h` emit identical mangled symbol — `nano_ros_node_register()` cmake fn wires it |
| **Bringup pkg** | language-agnostic — `package.xml` + `system.toml` + `launch/*.launch.xml` | identical (language-agnostic) |
| **Entry pkg** | `nros::main!(launch = "demo_bringup:sim.launch.xml")` proc-macro emits `fn main` + calls every Node pkg's register fn in launch order | **`nano_ros_entry(NAME SOURCES BOARD DEPLOY)` cmake fn exists but does NOT yet consume LAUNCH** — caller writes `main.cpp` by hand; bringup XML / pkg-index is invisible to the C++ side |

So: Node + Bringup are language-symmetric already. **Only Entry is
Rust-biased.** This phase closes that gap.

### 1.1 Anchor sources

- `cmake/NanoRosEntry.cmake` — current `nano_ros_entry()` impl.
- `cmake/NanoRosNodeRegister.cmake` — `nano_ros_node_register()` for
  Node pkgs; doc on the three pkg roles.
- `cmake/nano_ros_workspace_metadata.cmake` — shells `nros plan` at
  configure time, emits `nros_components.cmake`.
- `packages/core/nros-cpp/include/nros/node_pkg.hpp` +
  `packages/core/nros-c/include/nros/node_pkg.h` — Node-side
  declaration API.
- `packages/core/nros-macros/src/main_macro.rs` (948 LoC) — Rust
  `nros::main!()` impl. Reads workspace pkg-index, walks launch
  XML, emits `<pkg_ident>::register(runtime)?;` per `<node>` entry,
  threads `include_bytes!()` rebuild dep on every file consumed.
- `docs/design/multi-node-workspace-layout.md` §11.6 (Rust macro
  surface, **LOCKED**), §11.7 (C++ macro surface — **future**, the
  shape this phase makes concrete).

---

## 2. Symmetry rule

**One codegen library; two front-ends.**

- The pkg-index walker + launch.xml parser + plan→register-call
  resolver already live in `packages/cli/nros-cli-core/`
  (consumed today by the Rust proc-macro through a shared lib;
  Phase 218 brought the CLI into this monorepo so 219 work lands
  in the same tree). Phase 219 reuses **the same logic** from the
  C / C++ side via a new `nros codegen entry --lang {c,cpp}
  --launch <pkg>:<file>.launch.xml [--board <board>] --workspace
  <ws> --out <generated>.cpp` subcommand.
- The proc-macro becomes thin: it shells the same `nros codegen
  entry --lang rust` form (or stays as in-process proc-macro — both
  are valid). Whatever it does, the **canonical implementation lives
  in nros-cli**, not duplicated in cmake.

Why CLI shell (vs cmake parsing JSON itself): consistent error
messages between Rust/C/C++ front-ends, single SSoT for launch-XML
spec, no JSON-in-cmake parsing of substitutions / `<include>`
recursion.

---

## 3. Surface

### 3.0 What the user writes — canonical pure-C++ workspace

The §11.2 layout, expressed with the surface this phase delivers:

```text
my_ws/
├── CMakeLists.txt                # 4 lines (see §3.6)
└── src/
    ├── talker_pkg/               # Node pkg (C++)
    │   ├── package.xml
    │   ├── CMakeLists.txt        # nano_ros_node_register(...)
    │   └── src/Talker.cpp        # NROS_NODE_REGISTER(...)
    ├── listener_pkg/             # Node pkg (C++)
    │   ├── package.xml
    │   ├── CMakeLists.txt        # nano_ros_node_register(...)
    │   └── src/Listener.cpp      # NROS_NODE_REGISTER(...)
    ├── demo_bringup/             # Bringup pkg (language-agnostic)
    │   ├── package.xml
    │   ├── system.toml
    │   └── launch/
    │       └── system.launch.xml
    └── cpp_entry/                # Entry pkg (C++)
        ├── package.xml
        ├── CMakeLists.txt        # nano_ros_entry(LAUNCH ...)
        └── src/main.cpp          # NROS_MAIN(...) one-liner
```

Total user-authored cmake: **one workspace-root line per pkg + one
`nano_ros_*` call per pkg**. Same line-count budget as the Rust path.

### 3.1 cmake fn — `nano_ros_entry()` extended

Today:

```cmake
nano_ros_entry(
    NAME    native_entry
    SOURCES src/main.cpp
    BOARD   native
    DEPLOY  native
)
```

After Phase 219:

```cmake
nano_ros_entry(
    NAME    native_entry
    SOURCES src/main.cpp           # user-authored TU (optional — see §3.4)
    BOARD   native
    LAUNCH  "demo_bringup:sim.launch.xml"   # NEW (optional)
    DEPLOY  native
    ARGS    use_sim=true            # NEW (optional, forwarded to launch parser)
)
```

Behavior when `LAUNCH` present:

1. At configure time, locate the bringup pkg via the workspace
   pkg-index (workspace-root walk identical to Rust's N.10).
2. Shell `nros codegen entry --lang cpp --workspace ${WS}
   --launch demo_bringup:sim.launch.xml --board native --args
   "use_sim=true" --out
   ${CMAKE_BINARY_DIR}/nros_main_generated.cpp`.
3. Pin `CMAKE_CONFIGURE_DEPENDS` on every file the CLI reads
   (bringup `package.xml`, `system.toml`, the launch.xml, every
   transitively `<include>`d launch.xml, every Node pkg's
   `package.xml`). The CLI emits a `.d`-style depfile that the cmake
   fn slurps to populate `CMAKE_CONFIGURE_DEPENDS`.
4. Add the generated TU to the target's sources; link the same
   `NanoRos::NanoRosCpp` interface that the today's body links.

When `LAUNCH` absent: behavior unchanged (current `nano_ros_entry`
body) — single-node self-bringup, user-authored `main.cpp` provides
its own `main()`.

`SOURCES` becomes optional when `LAUNCH` present and the user
prefers the macro-only path (§3.4): the generated TU carries
`int main()`; the user's TU only needs to define
`NROS_MAIN(...)` (which, like Rust's `nros::main!()`, expands to
empty when the cmake fn already emitted the canonical body — see
§3.4 for the "header marker" pattern that suppresses double-emit).

### 3.2 CLI — `nros codegen entry`

```text
nros codegen entry
    --lang   {rust|c|cpp}                # required
    --launch <pkg>:<file>.launch.xml     # required for multi-Node bringup
    [--board <ident>]                    # optional; reads
                                         # [package.metadata.nros.entry] deploy
                                         # for the Rust path, otherwise required
    [--args   k=v[,k=v]…]                # launch-arg overrides
    --workspace <dir>                    # required (= Cargo.toml or
                                         # CMakeLists.txt root)
    --out     <generated>.{rs,c,cpp}     # required
    [--depfile <generated>.d]            # optional; emitted for
                                         # CMake CONFIGURE_DEPENDS /
                                         # `cargo:rerun-if-changed=` parity
```

Internally: `packages/cli/nros-cli-core/src/codegen/entry.rs` shared module reads pkg
index, parses launch.xml, resolves per-node `(pkg, exec)` to the
matching `__nros_component_<pkg>_register` symbol, builds a `Plan
{ board, nodes: [(pkg, args), …] }`, hands it to one of three
emitters:

- `emit_rust()` — already exists (today via proc-macro internals
  → factor out as a callable function so the CLI can drive it; the
  proc-macro keeps using it in-process).
- `emit_c()` — NEW.
- `emit_cpp()` — NEW.

### 3.3 Generated C++ TU shape (canonical)

```cpp
// Generated by `nros codegen entry --lang cpp` for native_entry @
// demo_bringup:sim.launch.xml. DO NOT EDIT — re-run the build to
// regenerate.

#include <nros/main.hpp>
#include <nros_board_native.hpp>

// One extern-C declaration per Node pkg the launch XML pulled in.
// Symbol = `__nros_component_<sanitised_pkg>_register` per Phase
// 212.M.5.a.1.
extern "C" {
    int32_t __nros_component_talker_pkg_register(nros::NodeContext*);
    int32_t __nros_component_listener_pkg_register(nros::NodeContext*);
}

int main(int /*argc*/, char** /*argv*/) {
    return nros::board::NativeBoard::run([](nros::Runtime& rt) -> int32_t {
        // Order matches launch.xml `<node>` order.
        if (auto rc = __nros_component_talker_pkg_register(rt.node_context());
                rc != 0) return rc;
        if (auto rc = __nros_component_listener_pkg_register(rt.node_context());
                rc != 0) return rc;
        return 0;
    });
}
```

Maps 1:1 to the Rust proc-macro output. Same launch-XML →
register-call ordering. Same Board ABI (`NativeBoard::run(closure)`
in Rust → `NativeBoard::run(lambda)` in C++; both already exposed
through `nros-cpp` via `nros::board::*`).

### 3.4 Optional `NROS_MAIN(...)` macro path

For users who want a single user-authored TU (no separate
generated file in `SOURCES`):

```cpp
// my_app/src/main.cpp
#include <nros/main.hpp>
NROS_MAIN(nros::board::NativeBoard, "demo_bringup:sim.launch.xml")
```

The macro is **declarative only** — it tells `nano_ros_entry()` to
expect a generated TU and tells the cmake fn that the user's TU
deliberately does NOT define `main`. Implementation:

- `nros/main.hpp` declares `NROS_MAIN(board, launch_spec)` as an
  expansion-to-nothing macro plus a sentinel symbol
  `__nros_entry_macro_present`. The cmake fn detects the sentinel
  via `target_sources` introspection or a `_NROS_ENTRY_HAS_MACRO` cache
  var set by a configure-time probe TU.
- When the macro is used, the cmake fn emits the generated TU as
  before; the macro itself is empty (Rust parity: the proc-macro IS
  the generator, but in C++ the cmake fn drives codegen + the macro
  is purely a doc/IDE hint).

Either way (macro-present or pure-cmake), one canonical generated
TU per Entry pkg. No double-emit.

### 3.5 C variant

Identical surface, swap `nros::board::NativeBoard` → C-FFI symbols
(`nros_board_native_run`). One macro:

```c
// my_app/src/main.c
#include <nros/main.h>
NROS_MAIN_C(nros_board_native, "demo_bringup:sim.launch.xml")
```

Same `nros codegen entry --lang c` path. C++ users use the C++
form; C users use the C form. Both ride the same cmake fn.

### 3.6 Workspace-root cmake — `nano_ros_workspace()`

The §11.2 tree wants a four-line workspace-root CMakeLists:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_ws LANGUAGES C CXX)

nano_ros_workspace(
    SYSTEM   demo_bringup
    BACKEND  zenoh                 # or xrce / cyclonedds
    SUBDIRS  src/talker_pkg
             src/listener_pkg
             src/cpp_entry
)
```

`nano_ros_workspace()` (landed by 219.I) does the heavy lifting:

1. Sets `NANO_ROS_PLATFORM=posix` + `NANO_ROS_RMW=${BACKEND}`.
2. `add_subdirectory(<nano-ros-root>)` **once** at root scope (so
   per-pkg subdirs don't collide on re-include — Gap 2 in the
   review).
3. `include(NanoRosNodeRegister.cmake)` once.
4. `include(NanoRosWorkspace.cmake)` so per-subdir
   `nano_ros_workspace_pkg_guard()` becomes available.
5. `nano_ros_workspace_metadata(SYSTEM ${SYSTEM})` so `nros plan`
   sees the Bringup pkg.
6. `add_subdirectory(<each member>)` for each `SUBDIRS` entry.

Each subdir CMakeLists (Node + Entry pkg) begins with:

```cmake
cmake_minimum_required(VERSION 3.22)
project(talker_pkg LANGUAGES C CXX)
nano_ros_workspace_pkg_guard()         # idempotent — no-op when
                                       # called inside a parent
                                       # workspace; full
                                       # standalone bootstrap when
                                       # called solo (preserves the
                                       # single-pkg copy-out path).
nros_find_interfaces(LANGUAGE CPP SKIP_INSTALL)
nano_ros_node_register(NAME talker
                       CLASS talker_pkg::Talker
                       SOURCES src/Talker.cpp
                       DEPLOY native)
```

`nano_ros_workspace_pkg_guard()` is the dual to `nano_ros_workspace()`
— call it at the top of every Node + Entry pkg CMakeLists. Body:

```cmake
function(nano_ros_workspace_pkg_guard)
    if(TARGET NanoRos::NanoRosCpp)
        return()      # parent workspace already imported nano-ros.
    endif()
    # Standalone path — replicate the §3.0 root's body for solo build.
    set(NANO_ROS_PLATFORM posix)
    set(NANO_ROS_RMW zenoh CACHE STRING "")
    add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../.."
                     nano_ros)
    include("${CMAKE_CURRENT_SOURCE_DIR}/../../../../cmake/NanoRosNodeRegister.cmake")
endfunction()
```

This is the cmake equivalent of `[workspace]` discipline in cargo:
every member compiles standalone OR as part of the workspace; both
shapes use the same surface. Closes Gap 1 + Gap 2 from the workflow
review.

---

## 4. Phased work plan

Single ordered list. Items grouped into three tracks; tracks land in
order but inside a track the items are independent. Cost tags
(`cheap` / `medium`) come from the workflow-review estimates;
`(✱)` marks items the workflow review (`phase-219-workflow-review.md`)
added on top of the original 219.A–G design.

### Track 1 — workspace plumbing (lands first)

These items unblock every multi-pkg C/C++ workspace and pay back
across the rest of Phase 219. Without them, 219.D's generated
`main()` fails to link.

- [x] **219.H — Idempotency guards in interface codegen.** (cheap, ✱)
      `cmake/NanoRosGenerateInterfaces.cmake` today guards only
      `builtin_interfaces`, `unique_identifier_msgs`, `action_msgs`
      against double-creation (lines 282-290). Every other
      interface pkg collides when two sibling Node pkgs both
      `<depend>` on it (e.g. both calling
      `nros_find_interfaces(LANGUAGE CPP)` with `std_msgs` in
      `package.xml`). Generalise the `if(NOT TARGET …)` guard so
      every interface pkg becomes idempotent. Collision sites at
      lines 462 / 471 / 607 per the review's anchor pointers.
      Closes review Gap 3.
- [x] **219.I — `nano_ros_workspace()` + `nano_ros_workspace_pkg_guard()`.**
      (cheap, ✱) Land `cmake/NanoRosWorkspace.cmake` per §3.6.
      Workspace-root single-call form pulls nano-ros once, includes
      `NanoRosNodeRegister.cmake` once, calls
      `nano_ros_workspace_metadata()`, then `add_subdirectory()` on
      every member. Per-pkg guard is the cmake equivalent of cargo
      `[workspace]` discipline — same subdir CMakeLists builds
      standalone OR inside the workspace. Closes review Gaps 1+2.

### Track 2 — Entry codegen (the headline work)

Once Track 1 is in, these items deliver the cmake `LAUNCH` arg + the
C / C++ TU emission. This is the original 219.A–F plan, unchanged in
shape.

- [x] **219.A — CLI: `nros codegen entry` skeleton.** Lift the
      proc-macro's pkg-index walk + launch parser out of
      `packages/core/nros-macros/src/main_macro.rs` into
      `packages/cli/nros-cli-core/src/codegen/entry/{mod.rs,emit_rust.rs}`. Add the
      `nros codegen entry --lang rust …` form; verify byte-identical
      output vs today's proc-macro `TokenStream` after
      `prettyplease` formatting. Proc-macro keeps in-process
      emission but defers semantic logic to the shared module.
- [x] **219.B — `emit_cpp()`.** Read `Plan` → emit the generated TU
      per §3.3. Include the `extern "C"` block, Board boot stub,
      register-call sequence. Unit-test against 1-Node + 2-Node
      fixtures.
- [x] **219.C — `emit_c()`.** Same shape, C language. C ABI for
      board entry (`nros_board_native_run` fn-ptr signature). Unit
      tests.
- [x] **219.D — cmake fn: `nano_ros_entry()` LAUNCH arg.** Extend
      `cmake/NanoRosEntry.cmake` with the `LAUNCH` + `ARGS` handling
      per §3.1. Shell `nros codegen entry --lang cpp` at configure
      time, append the generated TU to `add_executable(...)`
      sources, wire `CMAKE_CONFIGURE_DEPENDS` from the CLI's
      depfile. LAUNCH-absent fast path unchanged.
- [x] **219.E — Headers: `nros/main.hpp` + `nros/main.h`.** Empty
      macro definitions per §3.4-§3.5. No runtime symbols.
- [x] **219.J — Entry auto-links Node-pkg static libs.** (cheap, ✱)
      `nano_ros_entry()` today produces the exe but does NOT
      `target_link_libraries(<exe> PRIVATE
      <pkg>_<name>_component)` for the Node pkgs the launch XML
      pulls in — user writes the link by hand. The same loop in
      219.B/C that emits the `extern "C"` register decls owns the
      Node-pkg name list; emit a sibling `target_link_libraries`
      call from inside the cmake fn. Lands alongside 219.D — the
      generated `main()` is unlinkable without it. Closes review
      Gap 4.

### Track 3 — CLI cleanup (lands in parallel with Track 2)

These are CLI-side polish that doesn't block the cmake-side work
but is required for the end-to-end acceptance bar (§6).

- [x] **219.K — `nros codegen entry` runs without external
      `play-launch-parser`.** (cheap, ✱) **Resolved by design
      clarification (2026-06-04).** The workflow review's Gap 5 ("`nros
      plan` shells the external `play_launch_parser` Python tool")
      applies only to the `nros plan` path — that path supports both
      `.launch.xml` and `.launch.py`, and the Python form requires
      embedded CPython, which lives in the separate
      `play_launch_parser` tool to keep the `nros` binary free of
      pyo3/`libpython` (Phase 195.A rationale, preserved by 218).
      Phase 219's C/C++ codegen path is **XML-only** at v1
      (design-doc §11.5 explicitly defers `.launch.py`), so it does
      NOT route through `nros plan`. The Rust proc-macro already
      uses the in-process `packages/cli/nros-build/src/launch_parser.rs`
      (Phase 212.N.11) which covers every v1 tag (`arg` / `node` /
      `param` / `remap` / `group` / `include` / `launch`), every v1
      substitution (`$(find <pkg>)` / `$(var <arg>)` / `$(env <name>)`),
      and recursive `<include>` resolution. **Resolution:** when 219.A
      lands, the codegen entry verb consumes
      `nros_build::launch_parser::parse_launch_file()` directly — same
      in-process parser the Rust proc-macro uses; no shell-out, no
      Python dep, no cargo-feature toggle. Closes review Gap 5
      without code.
- [ ] **219.L — `nros metadata` walks cmake-only Node pkgs.**
      (medium, ✱) `nros metadata --workspace <ws> <bringup>`
      returns `preserved 0 metadata artifact(s)` against a
      pure-C++ workspace because
      `Workspace::component_declarations()` only reads `nros.toml
      [component]` tables; pure-C++ Node pkgs carry only
      `package.xml` + `CMakeLists.txt`. Today's flow relies on
      cmake configure writing
      `${CMAKE_BINARY_DIR}/nros-metadata.json` first, then `nros
      plan` reading it — that contract works but is undocumented.
      Land either:
      (a) extend the workspace walker to read
          `nano_ros_node_register(...)` calls statically from
          CMakeLists (parser is small — every call is single-line
          + keyword args); or
      (b) formalise the cmake-first contract with a `nros
          metadata --scan-cmake <build-dir>` flag and book
          chapter documenting "cmake configure must precede
          `nros plan`" for pure-C/C++ workspaces.
      Closes review Gap 6.
- [x] **219.M — `nros new` C/C++ scaffolds.** (cheap-to-medium, ✱)
      Today `nros new <name> --lang cpp --component` errors out
      ("scaffolds a Rust component; --lang cpp is not yet
      supported", `packages/cli/nros-cli-core/src/cmd/new.rs:116`). Plain
      `nros new <name> --lang cpp` emits a stub that still calls
      `find_package(NanoRos REQUIRED CONFIG)` (Phase 140 deleted
      that path) + `install(TARGETS ...)` (no install layout) +
      a hello-world `main.cpp` with zero nros surface. Land four
      scaffold templates aligned with §11.2: pure-C++ Node pkg,
      pure-C Node pkg, pure-C++ Entry pkg, Bringup pkg
      (language-agnostic). Closes review Gap 7.

### Track 4 — Fixture + docs (lands last)

- [x] **219.F / 219.N — In-tree multi-pkg pure-C++ fixture + nextest.**
      (cheap, ✱) Land `examples/native/cpp/multi-node-entry/`
      (canonical name). Two Node pkgs + one Bringup pkg + one Entry
      pkg, all C++. Nextest configures + builds + runs + asserts
      both register fns fire + the talker actually publishes.
      Regression guard for every other 219 item; doubles as the
      live example for 219.G. (The two original sub-items merged.)
- [ ] **219.G — Book chapter.** Add
      `book/src/concepts/cpp-entry-pkg.md` (or fold into
      `concepts/multi-node-workspace.md`) showing the §3.6
      workspace-root + the per-pkg shapes + the §3.4 macro form,
      side-by-side with the Rust `nros::main!()` form.

### Recommended landing order

```
Track 1                Track 2                Track 3      Track 4
─────────              ─────────────          ──────────   ──────────
219.H ─┐
       ├─→  219.A → B → C → D → E → J  ─┐
219.I ─┘    (Entry codegen + cmake fn)  │
                                         ├─→  219.F/N → 219.G
            219.K  (in parallel)        │
            219.L  (in parallel)  ──────┘
            219.M  (in parallel)
```

Track 1 first because every Track-2 demo / test workspace assumes
the §3.6 surface. Track 2 + Track 3 land in parallel (different
crates / fns). Track 4 is the regression-guard + book chapter on
top of everything.

## 5. Open questions

1. **`custom_tasks` parity for RTIC/Embassy boards.** The Rust
   proc-macro accepts `custom_tasks = [...]` (Phase 216.B.4). C++
   has no equivalent embedded board today (RTIC is Rust-only). For
   C++ this list is empty / not exposed for v1; revisit when
   216.D lands a C++ MCU board.
2. **Codegen output stability.** Emit a `// nros codegen X.Y.Z`
   version comment so a stale generated TU under VCS triggers a
   diff. (Generated TU sits under
   `${CMAKE_BINARY_DIR}/nros_main_generated.cpp` per current cmake
   conventions, so VCS isn't an issue — but if a user vendors the
   output we want the version banner.)
3. **rebuild-correctness on stable Rust = `include_bytes!`** today;
   the C/C++ path uses depfile + `CMAKE_CONFIGURE_DEPENDS`. Confirm
   both reach the same correctness floor on the launch-XML +
   `<include>` graph.
4. **`<param>` + `<remap>` codegen mapping.** §11.5 lists these as
   v1 launch tags. The Rust path currently emits a one-line param
   table per node — confirm the C++ Node ABI exposes the symmetric
   knob (`NodeOptions` already accepts a flat param list per
   212.M.5; sanity-check before 219.B/C land).
5. **Where the CLI lives.** **RESOLVED 2026-06-04.** Phase 218 landed
   (`549f413e3` through `e2a3c432e`) — `nros-cli` merged into this
   monorepo at `packages/cli/`. Every 219.X CLI item lives at
   `packages/cli/nros-cli-core/`, built + released alongside nano-ros
   via the JetPack-style bundle versioning + `just setup-cli` /
   `nros_cli_bin` resolver chain from 218.D. No cross-repo PRs for
   219 work.

## 6. Acceptance

- [ ] 219.A — `nros codegen entry --lang rust …` round-trips today's
      proc-macro output for two fixture workspaces (1-Node, 2-Node).
- [ ] 219.D — `examples/native/cpp/multi-node-entry/` configures
      cleanly via the cmake fn, no hand-written `main.cpp`.
- [ ] 219.F — the same Bringup pkg drives both a Rust Entry pkg
      (via `nros::main!()`) and a C++ Entry pkg (via
      `nano_ros_entry(LAUNCH …)`), with identical Node register
      order on both binaries — verified by a single integration test
      that diff-compares the two boot logs modulo language tag.
- [ ] 219.G — book chapter merged + cross-links the Rust and C++
      surfaces side-by-side.
- [ ] **219.acc.workflow** — workflow-review prereqs landed: a
      stock dev box, with the in-tree `packages/cli/target/release/nros`
      built (`just setup-cli`) on PATH via `activate.sh`, can
      `nros new --component --lang cpp talker_pkg` ×2 +
      `nros new --bringup demo_bringup` + `nros new --entry
      --lang cpp cpp_entry`, write the canonical workspace-root
      CMakeLists per 219.I, run `cmake -S . -B build && cmake
      --build build`, and execute the resulting binary. Talker
      pkg publishes 0, 1, 2, …; listener pkg receives. No
      hand-written `main()`, no hand-written `target_link_libraries`,
      no `pip install …`. Same workspace, swap `cpp_entry` →
      `rust_entry` calling `nros::main!(launch = …)` → identical
      behaviour modulo language tag.

## 7. Notes

- This phase intentionally **does not** touch the Node-pkg side. C
  and C++ Node pkgs already work end-to-end (Phase 212.L.9 +
  212.M.5.a.1). The only Rust-bias today is on the Entry side.
- The phase is **pure orchestration** — no new runtime, no new
  board ABI, no new RMW. Codegen + cmake-fn + headers only.
- Embedded C/C++ Entry pkgs are explicitly **out of scope** for
  v1. Phase 212.L.2 ruled Entry pkgs `native`-only at the cmake
  surface; an embedded C/C++ Entry path is a follow-on once a C/C++
  embedded board impl exists (cf. Phase 216.D).

---

## 8. C++ items surfaced 2026-06-04 (post-Phase-218 workflow audit)

Audit of the three-role per-language support matrix (alongside the
Phase 222 CLI shrink + Phase 223 C completion) surfaced four
C++-specific items that fall under this phase's scope. None block
the core 219.A–G plan; track as follow-ups on the same branch when
that lands.

### 8.1 `NROS_NODE` shorter alias

The Rust surface is `nros::node!(Talker);` (one arg). The C++ surface
is `NROS_NODE_REGISTER(Talker, "talker_pkg::Talker");` (two args). The
quoted qualified name is **derivable** at compile time from
`__PRETTY_FUNCTION__` / a constexpr stringification of the type, OR
from `NROS_PKG_NAME` + the user-supplied class name plus a `::`
separator (assuming the C++ pkg follows the `<pkg>::<UserClass>`
convention enforced by Phase 212.L.4's lint).

- [ ] **219.H.1** Add a 1-arg `NROS_NODE(UserClass)` alias that
      derives the qualified-class-name string by macro concatenation
      with `NROS_PKG_NAME`. Mirrors Rust's `nros::node!(Talker)`
      ergonomics. Existing 2-arg `NROS_NODE_REGISTER` stays for
      back-compat / explicit override cases.

**Files:** `packages/core/nros-cpp/include/nros/node_pkg.hpp`.

### 8.2 Document the C++ Node pkg → Rust Entry pkg link path

The C-FFI register trampoline (`__nros_component_<pkg>_register`) is
language-agnostic — a C++ Node pkg already links cleanly into a Rust
Entry pkg today (the per-pkg mangled register fn is C-ABI; cargo
sees it as an extern "C" symbol via build-script + linker glue). This
shape works but is **undocumented**: a user who wants C++ Node pkgs
linked into a Rust Entry pkg has no chapter / example to follow.

- [ ] **219.H.2** Book chapter (or `workspace-mixed-language.md`
      section, see Phase 223.B.2) documenting the C++ Node pkg → Rust
      Entry pkg link. Same path as C → C++/Rust (Phase 223.B); just
      a different upstream lang.

**Files:** `book/src/getting-started/workspace-mixed-language.md`
(new — owned by Phase 223.B.2; coordinated cross-ref).

### 8.3 `nros_entry()` cmake fn — accept C++ Node pkgs from `<exec_depend>`

When the Phase 219.A `nros_entry(LAUNCH "demo_bringup")` cmake fn
resolves the Bringup pkg's `<exec_depend>` list, it must accept
C++ Node pkg dependencies cleanly without forcing them into a
`<build_depend>` re-emission shape. Today's `nano_ros_node_register()`
cmake fn (Phase 212.L.9) already handles this for the
codegen-from-`system.toml` path; 219.A's launch.xml-driven path
needs to follow the same convention.

- [ ] **219.H.3** `nros_entry()` cmake fn — link C++ Node pkg static
      libs from the resolved `<node pkg="...">` set in launch.xml,
      treating C++ and Rust Node pkgs symmetrically (the existing
      C++ Node pkg `<pkg>_node_lib` target is linked the same way
      the Rust rlib import would be).

**Files:** `cmake/NanoRosEntry.cmake`.

### 8.4 C++ Entry pkg `--lang cpp` in `nros new`

- [ ] **219.H.4** `nros new --entry --lang cpp <name>` scaffolds a
      valid C++ Entry pkg (CMakeLists with `nros_entry(...)` +
      `main.cpp` with `NROS_MAIN(...)`). The CLI currently rejects
      this combination (see Phase 219.workflow-review's enumerated
      gaps). Lift the rejection once 219.A's codegen surface lands.

**Files:** `packages/cli/nros-cli-core/src/cmd/new.rs`.

### 8.5 Status

- [ ] **219.H** — all sub-items above. Land alongside or immediately
      after 219.A–F. None blocks 219.G (book chapter) — that chapter
      cross-references the 219.H mixed-lang link path + 1-arg
      `NROS_NODE` alias once they ship.
