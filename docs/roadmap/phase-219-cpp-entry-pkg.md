# Phase 219 — C / C++ Entry pkg support

**Goal.** Bring the C / C++ Entry-pkg path to parity with the Rust
`nros::main!()` flow landed in Phase 212.N.9. After this phase, a
`my_ws/src/native_entry_cpp/` Entry pkg with one CMakeLists.txt + one
`main.cpp` (and an optional `NROS_MAIN(...)` macro) wires a
multi-Node bringup against a Board with the same launch.xml +
workspace pkg-index semantics the Rust proc-macro already uses.

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
  resolver already live in the `nros-cli` codebase (consumed by the
  Rust proc-macro through a shared lib). Phase 219 reuses **the
  same logic** from the C / C++ side via a new
  `nros codegen entry --lang {c,cpp} --launch <pkg>:<file>.launch.xml
  [--board <board>] --workspace <ws> --out <generated>.cpp`
  subcommand.
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

Internally: `nros-cli/src/codegen/entry.rs` shared module reads pkg
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

---

## 4. Phased work plan

**Update 2026-06-04 (post-workflow-review).** Phase 219 originally
listed 219.A through 219.G. A pure-C/C++ workspace walkthrough
(`docs/roadmap/phase-219-workflow-review.md`) surfaced **six more
gaps** the original plan papered over — four cmake-fn-level + two
CLI-level. They sit BELOW the Entry-codegen surface; without them
219.D produces a generated `main()` that fails to link, and 219.F
has nothing reproducible to test against. New items 219.H–N below;
recommended ordering 219.H+I → 219.A-E → 219.J → 219.F → 219.G,
with 219.K/L/M in parallel.

- [ ] **219.A — CLI: `nros codegen entry` skeleton (lang=rust passthrough).**
      Lift the proc-macro's pkg-index walk + launch parser out of
      `packages/core/nros-macros/src/main_macro.rs` into
      `nros-cli/src/codegen/entry/{mod.rs,emit_rust.rs}`. Add the
      `nros codegen entry --lang rust …` form, verify it produces a
      file byte-identical to today's proc-macro `TokenStream` after
      `prettyplease` formatting. Tag for follow-up: the proc-macro
      keeps in-process emission for incrementality but defers
      semantic logic to the shared module.
- [ ] **219.B — `emit_cpp()`.** Read `Plan` → emit the generated TU
      per §3.3. Include the `<extern "C">` block, Board boot stub,
      register-call sequence. Unit-test against a 1-Node + 2-Node
      fixture.
- [ ] **219.C — `emit_c()`.** Same shape, C language. C ABI for
      board entry (`nros_board_native_run` fn ptr signature). Unit
      tests.
- [ ] **219.D — cmake fn: `nano_ros_entry()` LAUNCH arg.** Extend
      the existing `cmake/NanoRosEntry.cmake` with the LAUNCH +
      ARGS handling per §3.1. Shell `nros codegen entry --lang
      cpp` at configure time, append the generated TU to
      `add_executable(...)` sources, wire `CMAKE_CONFIGURE_DEPENDS`
      from the CLI's depfile. Keep the LAUNCH-absent fast path
      unchanged.
- [ ] **219.E — Headers: `nros/main.hpp` + `nros/main.h`.** Empty
      macro definitions per §3.4-§3.5. No runtime symbols.
- [ ] **219.F — Test fixture.** New
      `examples/native/cpp/multi-node-entry/` with one Bringup pkg
      pulling two Node pkgs. Nextest: configure + build + run +
      assert both Node pkgs' register fns fire.
- [ ] **219.G — Book chapter.** Add
      `book/src/concepts/cpp-entry-pkg.md` (or fold into
      `concepts/multi-node-workspace.md`) showing the cmake fn +
      macro shape side-by-side with the Rust `nros::main!()` form.

### 4.1 Workflow-review prereqs (added 2026-06-04)

These items come from the gap walkthrough in
`docs/roadmap/phase-219-workflow-review.md`. They block 219.D
delivering a linkable binary, and they each pay back across every
future multi-pkg C/C++ workspace, not just the Entry-codegen flow.

- [ ] **219.H — Idempotency guards in interface codegen** (cheap).
      `cmake/NanoRosGenerateInterfaces.cmake` today guards only
      `builtin_interfaces`, `unique_identifier_msgs`, `action_msgs`
      against double-creation (lines 282-290). Every other interface
      pkg collides when two sibling Node pkgs both `<depend>` on it
      (e.g. two pkgs each calling `nros_find_interfaces(LANGUAGE
      CPP)` with `std_msgs` in their `package.xml`). Generalise the
      `if(NOT TARGET …)` guard so every interface pkg becomes
      idempotent; collision sites at lines 462 / 471 / 607 per the
      review's anchor pointers.
- [ ] **219.I — `nano_ros_workspace()` cmake fn + canonical root**
      (cheap). §11.2 shows the layout but doesn't spell out the
      workspace-root cmake body. Land:
      - `cmake/NanoRosWorkspace.cmake` exposing
        `nano_ros_workspace(SYSTEM <bringup> [BRINGUP <pkg>]
        [SUBDIRS <dir>…])`.
      - Body: sets `NANO_ROS_PLATFORM` / `NANO_ROS_RMW`, does the
        single `add_subdirectory(<nano-ros>)` call, includes
        `NanoRosNodeRegister.cmake` once at root scope, then
        `add_subdirectory(<each member>)`.
      - Subdir CMakeLists for Node + Entry pkgs guard their
        `add_subdirectory(<nano-ros>)` + `include(...)` calls
        with `if(NOT TARGET NanoRos::NanoRos)`. Today's in-tree
        examples don't have the guards (workspace-root re-include
        dies on `nros_rmw_cffi_headers` duplicate target + Corrosion
        crate conflict). This also gives the multi-pkg path the
        same single-import discipline cargo workspaces enforce.
- [ ] **219.J — Entry auto-links Node-pkg static libs from launch
      metadata** (cheap). `nano_ros_entry()` today produces the exe
      but does NOT `target_link_libraries(<exe> PRIVATE
      <pkg>_<name>_component)` for the Node pkgs the launch XML
      pulls in. User writes the link by hand. The same loop in
      219.B/C that emits the `extern "C"` register decls owns the
      Node-pkg name list — emit a sibling `target_link_libraries`
      call from inside the cmake fn so the generated TU actually
      resolves at link time. Land alongside 219.D (the LAUNCH
      arg).
- [ ] **219.K — `nros codegen entry` runs without external
      `play-launch-parser`** (medium). `nros plan` shells the
      external `play_launch_parser` Python tool / binary today
      (`nros-cli-core/src/orchestration/planner.rs::437-462`,
      gated by the `play-launch-parser` cargo feature). The Rust
      proc-macro path doesn't (in-process via the `nros-build`
      dep). For C/C++ symmetry the shipped CLI must run on a
      stock dev box without `pip install play-launch-parser` /
      manual `NROS_PLAY_LAUNCH_PARSER=<path>`. Pick one of:
      (1) flip the `play-launch-parser` feature on in the
      prebuilt CLI release pipeline; (2) vendor + static-link
      the parser into the CLI binary; (3) write a thin
      in-process replacement on the subset of launch.xml the
      v1 spec covers (§11.5). Document the choice; whatever
      lands, `nros codegen entry --lang cpp` must succeed on a
      box where only `~/.nros/bin/nros` is installed.
- [ ] **219.L — `nros metadata` walks cmake-only Node pkgs**
      (medium). `nros metadata --workspace <ws> <bringup>` returns
      `preserved 0 metadata artifact(s)` against a pure-C++
      workspace because `Workspace::component_declarations()` only
      reads `nros.toml [component]` tables; pure-C++ Node pkgs
      carry only `package.xml` + `CMakeLists.txt`. Today's flow
      relies on cmake configure writing
      `${CMAKE_BINARY_DIR}/nros-metadata.json` first, then `nros
      plan` reading it — that contract works but is undocumented.
      Land either:
      (a) extend the workspace walker to read
          `nano_ros_node_register(...)` calls statically from
          CMakeLists (parser is small — every call is single-line
          + keyword args); or
      (b) formalise the cmake-first contract with a
          `nros metadata --scan-cmake <build-dir>` flag and book
          chapter documenting "cmake configure must precede
          `nros plan`" for pure-C/C++ workspaces.
- [ ] **219.M — `nros new --component --lang cpp` accepted +
      `--lang cpp` scaffold updated** (cheap-to-medium). Today
      `nros new <name> --lang cpp --component` errors out
      ("scaffolds a Rust component; --lang cpp is not yet
      supported", `nros-cli-core/src/cmd/new.rs:116`). Plain
      `nros new <name> --lang cpp` emits a stub that still calls
      `find_package(NanoRos REQUIRED CONFIG)` (Phase 140 deleted
      that path) + `install(TARGETS ...)` (no install layout) +
      a hello-world `main.cpp` with zero nros surface. Land four
      scaffold templates aligned with §11.2: pure-C++ Node pkg,
      pure-C Node pkg, pure-C++ Entry pkg, Bringup pkg
      (language-agnostic).
- [ ] **219.N — In-tree multi-pkg pure-C++ fixture + nextest**
      (cheap). Land
      `examples/native/cpp/multi-node-entry/` (canonical name
      tracks 219.F) as the regression guard for 219.A through M.
      Two Node pkgs + one Bringup pkg + one Entry pkg, all C++.
      Nextest configures + builds + runs + asserts both register
      fns fire + the talker actually publishes. Doubles as the
      book-chapter live example for 219.G.

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
5. **Where the CLI lives.** Phase 218 (`merge-cli-into-monorepo`) is
   open. If 218 lands before 219.A starts, codegen module sits in
   `packages/nros-cli/src/codegen/entry/`. If not, it sits in the
   standalone `nros-cli` repo and crosses the boundary the same way
   `nros plan` does today.

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
      stock dev box, with only `~/.nros/bin/nros` installed, can
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
