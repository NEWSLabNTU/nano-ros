# Phase 210 — ROS-convention codegen + workspace discovery

**Goal.** A standard ROS 2 msg package — verbatim `package.xml` +
`msg/*.msg` + the canonical `rosidl_generate_interfaces(...)` CMakeLists.txt
— builds against nano-ros **unmodified**, regardless of whether it lives in
the user's local `src/<pkg>/` workspace or in an ament-installed prefix on
`AMENT_PREFIX_PATH`. We roll our own codegen, but the source layout and the
CMake call shape are ROS's, so the same `src/` tree builds under both
`colcon build` (rosidl's bindings) and a nano-ros build (ours). Subsumes the
Phase 209.E bulk-codegen item.

**Status.** MVP DONE in-tree (2026-05-30, branch `phase-210-ros-convention-codegen`). A.1/.2/.3/.4 + B.1/.2 + E.1/.4 landed; C/D and the remaining E items are tracked but separate (C/D need nros-cli changes + a new Rust helper crate; E.2/.3 are in-tree migrations of existing call sites).

**Priority.** P2 — adoption ergonomics, not a capability gap. Closing it
turns "port a ROS msg pkg" from a per-CMakeLists rewrite into "drop the pkg
into `src/`, source the env".

**Depends on.** Phase 209.A–D (compat surface). Orthogonal to embedded-size
(204), Zephyr starter (205).

**Design.** [`docs/design/codegen-workspace-discovery.md`](../design/codegen-workspace-discovery.md).

## Overview — the two convention shifts

1. **Msg-package source layout = upstream ROS, verbatim.** Zero nano-ros-
   specific files in a msg package. The same `my_msgs/` directory builds
   under both colcon and nano-ros — different build systems, identical
   source layout + CMakeLists.txt.

2. **CMake call shape = upstream ROS, verbatim.** Public surface is
   `rosidl_generate_interfaces(<target> <files> [DEPENDENCIES …])` (the
   `rosidl_default_generators` signature). `find_package(<pkg> REQUIRED)`
   resolves a msg pkg through a layered search path and emits the
   canonical `${pkg}::${pkg}` IMPORTED INTERFACE target — no explicit
   `nros_*` call in user code. The legacy `nros_generate_interfaces(<pkg>)`
   + `nros_find_interfaces()` keep working as deprecated wrappers.

## Interface-package search path (layered)

| Layer | Source | Notes |
|---|---|---|
| 1 | `NROS_INTERFACE_SEARCH_PATH` (env / `-D`) | Colon-separated colcon-`src/`-style roots; immediate subdirs with `package.xml` are candidates. Highest priority. |
| 2 | `AMENT_PREFIX_PATH` | Already honoured (sourced `setup.bash`); `<prefix>/share/<pkg>/{msg,srv,action}/`. |
| 3 | `<nano-ros>/packages/interfaces/` + `share/nano-ros/interfaces/` | Bundled (today's `rcl-interfaces`, `lifecycle-msgs`). |

Shadowing (a workspace `std_msgs` shadowing an AMENT `std_msgs`) → take the
higher layer + warn loudly.

## Work Items

### 210.A — `rosidl_generate_interfaces(...)` + smart Find-stub
- [x] **210.A.1** `cmake/NanoRosGenerateInterfaces.cmake`: add
      `rosidl_generate_interfaces(<target> <files>… [DEPENDENCIES <pkg>…]
      [SKIP_INSTALL] [LIBRARY_NAME] [ADD_LINTER_TESTS]
      [SKIP_GROUP_MEMBERSHIP_CHECK])`. Takes explicit file paths (upstream
      shape); internally drives the existing codegen pipeline that
      `nros_generate_interfaces(<pkg>)` already uses. Rosidl-only flags
      (`ADD_LINTER_TESTS`, `SKIP_GROUP_MEMBERSHIP_CHECK`) accepted +
      no-opped with a `message(STATUS …)`. **Size:** ~80 LOC cmake.
- [x] **210.A.2** Smart Find-stub helper at
      `cmake/compat/stubs/_NrosFindRosMsgPackage.cmake`. Walks the search
      path → finds the named pkg → reads its `package.xml` (deps) + globs
      `{msg,srv,action}/*` → runs nano-ros codegen → emits IMPORTED
      INTERFACE `${pkg}::${pkg}` aliasing `${pkg}__nano_ros_cpp` /
      `__nano_ros_rust`. **Size:** ~150 LOC cmake.
- [x] **210.A.3** Collapse the per-pkg `cmake/compat/stubs/Find<msg>.cmake`
      files to 2 lines each (include + delegate). One file per msg pkg the
      compat ships; adding a new one is two lines.
- [x] **210.A.4** Fixture: a tiny `examples/templates/local-msg-package/`
      with a verbatim ROS msg pkg (`package.xml` + `msg/MyMsg.msg` +
      canonical CMakeLists.txt) + a consumer node that just writes
      `find_package(local_msgs REQUIRED) + target_link_libraries
      (my_node local_msgs::local_msgs)`. Builds the same source under both
      `colcon build` and a nano-ros cmake build — captured in CI.
- [ ] **Acceptance:** the fixture's msg pkg's CMakeLists.txt has **zero
      nano-ros-specific lines**; the consumer's `find_package(local_msgs)`
      resolves through the smart stub and emits a target the consumer
      links against without any explicit codegen call.

### 210.B — `NROS_INTERFACE_SEARCH_PATH` + `nros_workspace_interfaces()`
- [x] **210.B.1** Plumb `NROS_INTERFACE_SEARCH_PATH` (env + cmake var)
      through the smart Find-stub (210.A.2).
- [x] **210.B.2** `nros_workspace_interfaces([PATHS <dir>…] [LANGUAGE …])`
      — bulk orchestrator. Scans the search path, identifies pkgs by
      `<member_of_group>rosidl_interface_packages</member_of_group>` in
      their `package.xml`, topo-sorts (via existing `nros codegen
      resolve-deps`), `add_subdirectory(<pkg-dir>)` each so the pkg's own
      CMakeLists runs (which calls `rosidl_generate_interfaces`). **Size:**
      ~100 LOC cmake.
- [ ] **210.B.3** Optional `nros workspace env [<dir>]` CLI subcommand
      printing `export NROS_INTERFACE_SEARCH_PATH=<dir>:$NROS_INTERFACE_SEARCH_PATH`
      (mirrors colcon's `setup.bash` ergonomics). Lives in nros-cli.
- [ ] **Acceptance:** a user workspace at `$HOME/my_ros2_ws/src/{a,b}` (b
      depends on a; both rosidl-interface-pkgs) builds with a single
      `nros_workspace_interfaces()` call in the consuming app's
      CMakeLists.txt; the order is correct (topo-sorted); a shadowed pkg
      (workspace's `std_msgs` over AMENT's) takes the workspace one with
      a warning.

### 210.C — `nros codegen --workspace` + upstream header layout (nros-cli)
- [ ] **210.C.1** Extend `nros codegen resolve-deps` with `--workspace <dir>`
      / `--search-path <dir>` flags (consistent with the cmake side).
- [ ] **210.C.2** `nros generate cpp --workspace <dir>` and
      `nros generate-rust --workspace <dir>` subcommand wrappers.
- [ ] **210.C.3** Codegen also emits the upstream-style
      `<pkg>/msg/<name>.hpp` per-message header alongside the existing
      `<pkg>/<pkg>.hpp` umbrella. Closes the cosmetic in Phase 209.G iter 2.
- [ ] **Acceptance:** `nros generate cpp --workspace ./` produces every
      pkg's bindings into `./build/codegen/` in topo order; ported source
      compiles with both `<pkg>/msg/<name>.hpp` and `<pkg>/<pkg>.hpp`
      includes.

### 210.D — Rust `build.rs` helper (`nros-build-codegen`)
- [ ] **210.D.1** New crate `packages/core/nros-build-codegen/` (mirrors
      `nros-build-paths`). Public API:
      ```rust
      fn main() { nros_build_codegen::workspace().run().unwrap(); }
      ```
      Discovers + codegens via the same search path.
- [ ] **210.D.2** Convert one rust example (probably
      `examples/native/rust/talker`) to use it; deprecate the ad-hoc
      `fixtures-build.sh` rust codegen loop for pkgs that adopt the helper.
- [ ] **Acceptance:** the converted example's `build.rs` is two lines;
      `cargo build` from a clean checkout produces the same artefacts as
      the current per-example codegen.

### 210.E — UX + docs + in-tree migration
- [x] **210.E.1** Book page `book/src/getting-started/your-own-msg-package.md`
      walking the upstream workflow: drop a `src/my_msgs/` (verbatim ROS
      shape), source the env, build. Both colcon AND nano-ros work on the
      same source. Cross-ref 210.A's fixture.
- [ ] **210.E.2** Update existing
      `book/src/getting-started/porting-a-cpp-node.md` (209.G iter 2)
      `nros_generate_interfaces(<pkg>)` glue example to the new
      `find_package(<pkg>) / nros_workspace_interfaces()` shape so the
      porting story collapses to "drop standard `find_package` calls in;
      no `nros_*` macros".
- [ ] **210.E.3** Migrate the in-tree per-pkg `nros_generate_interfaces
      (<pkg>)` call sites (sample: `examples/native/{cpp,rust}/*/`,
      `examples/qemu-arm-*/{cpp,rust}/*/`) to the
      `find_package(<pkg>) + target_link_libraries` shape. Incremental —
      examples that explicitly want the bundled-pkg form keep it.
- [x] **210.E.4** Mark `nros_generate_interfaces(<pkg>)` +
      `nros_find_interfaces()` deprecated in their function-header
      comments; point to `rosidl_generate_interfaces` + `find_package`.

## Acceptance criteria

- [ ] A standard ROS msg package (verbatim `package.xml` +
      `rosidl_generate_interfaces(...)` CMakeLists.txt) builds against
      nano-ros via `add_subdirectory(src/my_msgs)` with **zero** edits to
      the msg pkg.
- [ ] A consumer writes `find_package(my_msgs REQUIRED)` +
      `target_link_libraries(my_node my_msgs::my_msgs)` (verbatim upstream
      shape); the smart Find-stub does the codegen.
- [ ] The same `src/` workspace builds with both `colcon build` and a
      nano-ros cmake build (different build systems, identical source).
- [ ] An app's `CMakeLists.txt` drops the N per-pkg codegen lines to one
      optional `nros_workspace_interfaces()` call.
- [ ] `nros generate cpp --workspace ./` produces a full closure for a
      multi-pkg `src/`.
- [ ] Book page `your-own-msg-package.md` walks the workflow end-to-end.

## Notes / cross-refs

- Subsumes the Phase 209.E item (`nros generate cpp --workspace` was
  originally filed there; 210.C is the same work in the broader workspace-
  discovery frame).
- Phase 209.G iter 2's two codegen cosmetics (FixedString vs std::string;
  umbrella vs per-msg header path) are closed by 210.C.3 — but the
  FixedString-vs-std::string aspect needs its own follow-up (it's a codegen
  output-shape choice, not a layout one; tracked as a sub-bullet under
  210.C if it turns out to affect upstream-source compile).
- Legacy `packages/interfaces/<pkg>/` bundled layout is preserved as the
  lowest-priority search layer; nothing moves.
