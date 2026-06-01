# Phase 210 — ROS-convention codegen + workspace discovery

**Goal.** A standard ROS 2 msg package — verbatim `package.xml` +
`msg/*.msg` + the canonical `rosidl_generate_interfaces(...)` CMakeLists.txt
— builds against nano-ros **unmodified**, regardless of whether it lives in
the user's local `src/<pkg>/` workspace or in an ament-installed prefix on
`AMENT_PREFIX_PATH`. We roll our own codegen, but the source layout and the
CMake call shape are ROS's, so the same `src/` tree builds under both
`colcon build` (rosidl's bindings) and a nano-ros build (ours). Subsumes the
Phase 209.E bulk-codegen item.

**Status.** MVP + Rust workspace flow DONE (2026-05-31). Mixed-workspace
cmake + Rust path proved end-to-end. Remaining work: book refresh +
in-tree migration + ws doctor siblings + shadowing fixture.

In-tree (landed on main):
* 210.A.1/.2/.3/.4 — rosidl wrapper + smart Find-stub + per-pkg delegators
  + local-msg-package fixture.
* 210.B.1/.2 — `NROS_INTERFACE_SEARCH_PATH` + `nros_workspace_interfaces()`
  bulk orchestrator with topo-sort.
* 210.D.3 — Rust mixed-workspace fixture sibling
  (`local-msg-package/src/rust_consumer/`) — plain `cargo build` after
  `nros ws sync`, 4 msg families (workspace + AMENT) linked in.
* 210.E.1 — book page `your-own-msg-package.md`.
* 210.E.2 — book page `porting-a-cpp-node.md` refreshed to Phase 210
  shape (find_package + ament_target_dependencies + workspace umbrella).
* 210.E.3.a — native cpp examples migrated (talker / listener /
  service-* / action-* — 6 examples switched from
  `nros_generate_interfaces(<pkg>)` to `find_package(<pkg>) +
  ament_target_dependencies` shape).
* 210.E.3.b — embedded cpp examples migrated (24 examples across
  freertos/nuttx/threadx-riscv64/threadx-linux). freertos +
  threadx-linux build green; nuttx + threadx-riscv have pre-existing
  build failures (chdir-to-bash; cc-rs picolibc include) unrelated to
  the migration — same errors on unmigrated CMakeLists.
* 210.E.4 — deprecation comments on legacy `nros_generate_interfaces` +
  `nros_find_interfaces`.
* 210.F.1 — mixed-workspace fixture (workspace + AMENT msg deps in one
  consumer, multi-level dep closure cache `_NROS_PKG_<pkg>_*`).
* 210.F.2 — colcon-parity CI gate (`.github/workflows/colcon-parity.yml`
  builds the fixture via `colcon build --base-paths src` + verifies the
  binary).

Supporting fixes (landed alongside the migrations):
* `cmake/compat/stubs/Findexample_interfaces.cmake` — 209.B's
  collapse list missed it; without the stub `find_package
  (example_interfaces)` falls through to upstream's ament-based config
  which errors on nano-ros builds.
* `nros_generate_interfaces` closure-builder CACHE-fallback (BOTH
  spots) — multi-level dep chains through the smart Find-stub need to
  read `_NROS_PKG_<pkg>_GENERATED_RS_FILES` from cache when the dep
  was generated in a sibling call tree (smart-stub recursion vs auto-
  closure nested call).

nros-cli (`github.com/NEWSLabNTU/nros-cli` branch `phase-210-workspace-
codegen`):
* 210.B.3 — `nros ws env [<dir>] [--shell posix|fish]` (commit `41177dd`).
* 210.D.1 — `nros ws sync` subcommand + single-pkg-mode (commits
  `71688f2` + `190b891`). Codegens msg pkgs + writes delimited
  `[patch.crates-io]` block into patch authority Cargo.toml. Ships in
  next `nros` release.

Already-shipped:
* 210.C.3 — `<pkg>/msg/<name>.hpp` alias header (Phase 123.B.8).

Open (concrete acceptance below):
* 210.C.1 / C.2 — `nros codegen resolve-deps --workspace` +
  `nros generate cpp --workspace`. Blocked on 210.D needing them; still
  deferred (sync handles the resolve+codegen path internally).
* 210.D.2 — convert `examples/native/rust/talker`. Deferred → 210.E.3.d
  (whole rust-example fleet migration; talker is multi-RMW with a cmake
  cyclone variant on top, safer to migrate as one unit).
* 210.E.3.c — Zephyr cpp examples. DEFERRED with concrete blocker:
  Zephyr's cmake context uses `find_package(Zephyr)` + zephyr_library
  aggregation, NOT `add_subdirectory(nano-ros)`. NrosRclcppCompat's
  sanity check fires FATAL_ERROR (no `NanoRos::NanoRosCpp` target).
  Fix needs cache-the-NROS_REPO_DIR + alias-zephyr-lib-nros work
  (partially explored in 209.G iter 2). Real follow-up.
* 210.E.3.d — Rust example migration (D.2 talker + others). DEFERRED.
  Talker is multi-RMW with cmake-cyclonedds; needs a dedicated rust-
  migration plan. `nros ws sync` single-pkg-mode is in place for the
  swap.
* 210.F.3 — `nros ws doctor` + `list` + `status` + `clean` siblings.
* 210.F.4 — shadowing matrix smoke fixture + book doc.

**CLI surface (2026-05-31):** workspace-level commands live under the
`nros ws` namespace. Shipped subcommands: `env`, `sync`. Planned:
`list`, `status`, `clean`, `doctor` (210.F.3).

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
- [x] **210.B.3** `nros ws env [<dir>] [--shell posix|fish]` — landed
      in `github.com/NEWSLabNTU/nros-cli` branch
      `phase-210-workspace-codegen` (commit `41177dd`). Ships in next
      `nros` release. Prepends the resolved absolute path to
      `NROS_INTERFACE_SEARCH_PATH` (literal `${NROS_INTERFACE_SEARCH_PATH:-}`
      expansion so stacked `eval "$(nros ws env ...)"` calls
      compose). POSIX + fish output. (2026-05-30.)
- [ ] **Acceptance:** a user workspace at `$HOME/my_ros2_ws/src/{a,b}` (b
      depends on a; both rosidl-interface-pkgs) builds with a single
      `nros_workspace_interfaces()` call in the consuming app's
      CMakeLists.txt; the order is correct (topo-sorted); a shadowed pkg
      (workspace's `std_msgs` over AMENT's) takes the workspace one with
      a warning.

### 210.C — `nros codegen --workspace` + upstream header layout (nros-cli)
- [ ] **210.C.1** (DEFERRED — re-file with 210.D) Extend
      `nros codegen resolve-deps` with `--workspace <dir>` /
      `--search-path <dir>` flags. Cmake-side `nros_workspace_interfaces()`
      self-scans + topo-sorts; CLI workspace-resolve has no current consumer.
      Re-file when 210.D Rust build.rs helper lands (it would shell out to
      `nros ws sync` flow + per-pkg `nros generate-rust --search-path`).
- [ ] **210.C.2** (DEFERRED — re-file with 210.D) `nros generate cpp
      --workspace <dir>` and `nros generate-rust --workspace <dir>`
      subcommand wrappers. Same reason as C.1.
- [x] **210.C.3** Codegen already emits the upstream-style
      `<pkg>/msg/<name>.hpp` per-message header alongside the existing
      `<pkg>/<pkg>.hpp` umbrella — **already shipped under Phase 123.B.8**
      (`NROS_ALIAS_*_HPP_` forwarder headers). Verified in the
      `local-msg-package` fixture build dir; closes the 209.G iter 2
      cosmetic with no extra work.
- [ ] **Acceptance:** `nros generate cpp --workspace ./` produces every
      pkg's bindings into `./build/codegen/` in topo order; ported source
      compiles with both `<pkg>/msg/<name>.hpp` and `<pkg>/<pkg>.hpp`
      includes.

### 210.D — Rust workspace codegen via `nros ws sync` (LOCKED 2026-05-31)

**Design constraints driving the shape:**

* User runs **plain `cargo build`** — no `nros build` wrapper, no
  `build.rs` helper that auto-codegens at cargo time.
* User-pkg `Cargo.toml` is **verbatim upstream-ROS shape** (`local_msgs =
  "*"`) — same `Cargo.toml` builds under stock `colcon build`.
* No forced Cargo workspace layout — works for standalone pkgs AND mixed
  Cargo workspaces.

The chicken-egg blocker: cargo's dep graph is closed-set, resolved BEFORE
`build.rs` runs. To redirect `local_msgs = "*"` to a generated crate, the
`[patch.crates-io]` table must exist when cargo parses Cargo.toml. Cargo
only honors `[patch]` from **Cargo.toml** (workspace root or standalone
pkg) — NOT from `.cargo/config.toml`. So the redirect lives in Cargo.toml,
auto-managed by a pre-cargo step (`nros ws sync`).

#### 210.D.1 — `nros ws sync` subcommand (nros-cli) — **LANDED 2026-05-31**

Shipped on `phase-210-workspace-codegen` (commits `71688f2` initial impl +
`190b891` single-pkg-mode follow-up). Ships in next `nros` release.

```
nros ws sync [<workspace>]
    [--build-dir <dir>]              # default ./build
    [--nano-ros-path <dir>]          # also env: NROS_REPO_DIR
    [--ros-edition humble|iron]      # default humble
    [--dry-run]
    [--check]                        # exit non-zero if stale
    [-v, --verbose]
```

Behavior:
1. Resolve workspace root (cwd or `--workspace` arg). Two layouts:
   * **colcon-style**: `src/<pkg>/package.xml` (multi-pkg workspace).
   * **single-pkg**: `<root>/package.xml` (standalone example shape).
2. Scan workspace for msg pkgs (member_of_group=rosidl_interface_packages
   OR msg/srv/action dirs).
3. Recursively codegen AMENT_PREFIX_PATH msg pkgs that workspace pkgs +
   Rust consumers transitively depend on (resolved via
   `rosidl_bindgen::ament::AmentIndex`).
4. Codegen each msg pkg via `rosidl_bindgen::generator::generate_package`
   into **`build/nros_generator_rs/<pkg>/`** (flat layout — generator
   emits sibling-relative path deps, so all generated crates live as
   siblings under one dir; we don't nest into colcon's per-pkg-subdir
   `build/<pkg>/rosidl_generator_rs/...` since that'd require rewriting
   every `path = "../<dep>"` in generated Cargo.toml files).
5. Auto-detect patch authority per Rust consumer pkg: walk up from the
   pkg dir to find first Cargo.toml containing `[workspace]` (cargo's
   own rule — `[patch]` only allowed at workspace root or standalone
   pkg). Standalone pkgs with `[workspace]` empty marker get the block
   in their own Cargo.toml; Cargo workspace members get it at the
   umbrella.
6. Write/refresh a **delimited `[patch.crates-io]` block** in the patch
   authority. Block covers (a) generated msg crates and (b) `nros-*`
   runtime crates (when `--nano-ros-path` / `NROS_REPO_DIR` is set).
   Idempotent: re-sync replaces only content between the markers.

Block shape (verified output):
```toml
# === BEGIN nros-managed [patch.crates-io] ===
# Auto-generated by `nros ws sync` (2026-05-31T10:23:45Z).
# Do not edit between the BEGIN/END markers — re-run sync instead.
[patch.crates-io]
builtin_interfaces = { path = "../../build/nros_generator_rs/builtin_interfaces" }
extra_msgs         = { path = "../../build/nros_generator_rs/extra_msgs" }
geometry_msgs      = { path = "../../build/nros_generator_rs/geometry_msgs" }
local_msgs         = { path = "../../build/nros_generator_rs/local_msgs" }
sensor_msgs        = { path = "../../build/nros_generator_rs/sensor_msgs" }
std_msgs           = { path = "../../build/nros_generator_rs/std_msgs" }

# nros-* runtime crates
nros               = { path = "../../../../../packages/core/nros" }
nros-core          = { path = "../../../../../packages/core/nros-core" }
nros-serdes        = { path = "../../../../../packages/core/nros-serdes" }
nros-platform      = { path = "../../../../../packages/core/nros-platform" }
nros-platform-cffi = { path = "../../../../../packages/core/nros-platform-cffi" }
nros-node          = { path = "../../../../../packages/core/nros-node" }
nros-rmw           = { path = "../../../../../packages/core/nros-rmw" }
nros-rmw-cffi      = { path = "../../../../../packages/core/nros-rmw-cffi" }
nros-log           = { path = "../../../../../packages/core/nros-log" }
nros-macros        = { path = "../../../../../packages/core/nros-macros" }
nros-rmw-zenoh     = { path = "../../../../../packages/zpico/nros-rmw-zenoh" }
# === END nros-managed [patch.crates-io] ===
```

**Why `Cargo.toml` and not `.cargo/config.toml`:** cargo's hard rule —
`[patch]` is only honored from Cargo.toml. `.cargo/config.toml` accepts
`[source]` replacement, but that's all-or-nothing (every crates.io
lookup goes local; requires vendoring every transitive dep including
non-ROS ones). Heavy + breaks mixed workspaces where some members have
no ROS deps. Rejected.

**Patch authority auto-detection is the deciding feature for mixed
workspaces:** sync writes to whichever Cargo.toml cargo treats as the
patch authority (cargo's rule: patch only in workspace root or
standalone pkg). The user's Cargo workspace layout — whatever it is —
is respected.

#### 210.D.2 — Convert one rust example — **DEFERRED to 210.E.3.d**

`examples/native/rust/talker` is multi-RMW (zenoh/cyclonedds/xrce) with a
sibling cmake-cyclonedds variant on top. The patch table needs to live
alongside the cmake-driven cyclone build, not replace it; the safe
migration lands as part of the whole rust-example fleet migration in
210.E.3.d. The `nros ws sync` single-pkg-mode (commit `190b891` in
nros-cli) is in place so the migration is a swap-in when E.3.d runs.

#### 210.D.3 — Rust mixed-workspace fixture sibling — **LANDED 2026-05-31**

Add `src/rust_consumer/` to `examples/templates/local-msg-package/`
consuming `local_msgs` + `extra_msgs` + `std_msgs` + `geometry_msgs`
(same msg coverage as the C++ consumer landed in `0ddcc60fc`).

User experience:
```sh
$ cd examples/templates/local-msg-package
$ nros ws sync
nros: scanning src/ … 4 msg pkgs (local_msgs, extra_msgs, std_msgs, geometry_msgs)
nros: codegen → build/{local_msgs,extra_msgs,std_msgs,geometry_msgs}/nros_generator_rs/.../rust/
nros: patch authority: src/rust_consumer/Cargo.toml (standalone)
nros: refreshed [patch.crates-io] block

$ cd src/rust_consumer
$ cargo build      # plain cargo, no wrapper, no build.rs hack
```

`src/rust_consumer/Cargo.toml`:
```toml
[package]
name = "rust_consumer"
version = "0.1.0"
edition = "2024"

[workspace]      # standalone marker (cargo-ros2 pattern); makes Cargo.toml
                 # the patch authority for this pkg

[dependencies]
nros          = "*"
local_msgs    = "*"
extra_msgs    = "*"
std_msgs      = "*"
geometry_msgs = "*"

# (Auto-managed [patch.crates-io] block lands here on first sync.)
```

NO `build.rs`, NO `nros-build-codegen` build-dependency. The pkg is verbatim-
upstream shape; the patch table is auto-managed metadata at the bottom.

#### 210.D Acceptance

- [x] `nros ws sync` from `examples/templates/local-msg-package/` codegens
      msg pkgs into `build/nros_generator_rs/<pkg>/` and writes the
      patch block into `src/rust_consumer/Cargo.toml`. (Verified
      2026-05-31.)
- [x] `cd src/rust_consumer && cargo build` (plain cargo, no wrapper) links
      against all four msg families via the patch table — `nm` shows
      94 symbols across greeting/echo/geometry/sensor (verified).
- [ ] `nros ws sync --check` from a fresh checkout (pre-first-sync) exits
      non-zero with a clear message.
- [ ] Editing any `*.msg` file → `nros ws sync` regenerates ONLY the
      affected crate (mtime check) + leaves the patch block untouched (no
      churn).
- [ ] In a Cargo workspace (real `[workspace] members = [...]` at umbrella
      Cargo.toml), `nros ws sync` writes the patch block to the umbrella,
      NOT to the member pkg.

### 210.E — UX + docs + in-tree migration
- [x] **210.E.1** Book page `book/src/getting-started/your-own-msg-package.md`
      walking the upstream workflow: drop a `src/my_msgs/` (verbatim ROS
      shape), source the env, build. Both colcon AND nano-ros work on the
      same source. Cross-ref 210.A's fixture.
- [x] **210.E.2** Update existing
      `book/src/getting-started/porting-a-cpp-node.md` (209.G iter 2)
      `nros_generate_interfaces(<pkg>)` glue example to the new
      `find_package(<pkg>) / nros_workspace_interfaces()` shape so the
      porting story collapses to "drop standard `find_package` calls in;
      no `nros_*` macros".
      **Acceptance:** the worked example in the book page has zero
      `nros_*` cmake calls; only `find_package` + `target_link_libraries`
      + (optionally) `nros_workspace_interfaces()` if the user has
      workspace-local pkgs. Cross-ref the `local-msg-package` fixture.
- [ ] **210.E.3** Migrate the in-tree per-pkg `nros_generate_interfaces
      (<pkg>)` call sites to the `find_package(<pkg>) +
      ament_target_dependencies` shape. Incremental — examples that
      explicitly want the bundled-pkg form keep it.
      **Sub-items**:
      * **210.E.3.a** — Native fixtures: `examples/native/cpp/{talker,
        listener,service-*,action-*}/`. **DONE 2026-05-31** (commit
        `2e5f50b2c`). Swapped 6 examples; supporting fix landed:
        `cmake/compat/stubs/Findexample_interfaces.cmake` added +
        `nros_generate_interfaces` CACHE-fallback in BOTH closure
        builders (multi-level dep chains through the smart Find-stub
        cache).
      * **210.E.3.b — Embedded fixtures (freertos/nuttx/threadx-riscv64/
        threadx-linux cpp).** **DONE 2026-05-31** (commit `055d56773`).
        Earlier deferral rationale was wrong — spike-tested by migrating
        freertos cpp talker manually; configure + build both ✓ on the
        same shape as native (E.3.a). Migrated 24 cpp examples across
        4 platforms. Verified per-platform talker:
            - qemu-arm-freertos       configure ✓ build ✓
            - qemu-arm-nuttx          configure ✓ build PRE-EXISTING FAIL
            - qemu-riscv64-threadx    configure ✓ build PRE-EXISTING FAIL
            - threadx-linux           configure ✓ build ✓
        Pre-existing FAILs reproduce on unmigrated CMakeLists too —
        unrelated to find_package migration; tracked separately.
      * **210.E.3.c — Zephyr fixtures.** DEFERRED (real blocker).
        Investigated 2026-05-31: Zephyr's cmake context has
        `find_package(Zephyr)` before `project()` — the nros zephyr
        module auto-pulls `NanoRos` via `zephyr_library_named(nros)`,
        NOT via `add_subdirectory(nano-ros)`. So:
            1. `NanoRos::NanoRosCpp` target doesn't exist —
               NrosRclcppCompat's sanity check fires FATAL_ERROR.
            2. `NROS_REPO_DIR` is set in the module but NOT cached, so
               the example's `include("${NROS_REPO_DIR}/cmake/compat/
               NrosRclcppCompat.cmake")` resolves to `/cmake/compat/...`.
        Both fixable (cache NROS_REPO_DIR + alias zephyr_library `nros`
        → `NanoRos::NanoRosCpp` inside NrosRclcppCompat when CONFIG_NROS
        is set — work pattern partially explored in 209.G iter 2 then
        reverted). Real follow-up phase, not a same-day swap.
      * **210.E.3.d — Rust examples (`.cargo/config.toml [patch.crates-io]`
        deprecation + talker D.2 migration).** DEFERRED. Touches every
        rust example. Talker is multi-RMW with a cmake-cyclonedds
        variant. The `nros ws sync` single-pkg-mode (nros-cli commit
        `190b891`) is in place so the swap is mechanical when a
        dedicated rust-migration phase runs.
- [x] **210.E.4** Mark `nros_generate_interfaces(<pkg>)` +
      `nros_find_interfaces()` deprecated in their function-header
      comments; point to `rosidl_generate_interfaces` + `find_package`.

### 210.F — Workspace cases (mixed sources, colcon parity, doctor) — POST-MVP

The MVP (A + B + E.1 + E.4) proves the surface; the local-msg-package
mixed-workspace fixture (`0ddcc60fc`) proves cmake-side workspace + AMENT
coverage works end-to-end. Stage F closes the **workspace** story across
the Rust frontend, the colcon-parity proof, and the doctor surface.

- [x] **210.F.1** Mixed-workspace fixture (workspace + AMENT msg sources
      in one consumer). Landed `0ddcc60fc` (2026-05-30). Local fixture
      `examples/templates/local-msg-package/src/consumer/` pulls msgs
      from `local_msgs` + `extra_msgs` (workspace) AND `geometry_msgs` +
      `sensor_msgs` + `std_msgs` (AMENT) via one `find_package(<pkg>)`
      shape. Surfaced + fixed the multi-level dep closure issue (cache
      stash in `_NROS_PKG_<pkg>_*` INTERNAL vars from both rosidl wrapper
      AND smart Find-stub).
- [x] **210.F.2** colcon-parity CI gate. The fixture's `src/` tree is
      meant to build under BOTH a nano-ros umbrella AND `colcon build`.
      Today the parity is asserted in `README.md`, not in CI.
      **Work:** add a CI job (probably under `.github/workflows/`) that
      sources `/opt/ros/humble/setup.bash` + runs `colcon build` against
      `examples/templates/local-msg-package/src/`; verifies the binary
      runs (publishes + exits cleanly). Skip the embedded targets — only
      the native-cpp parity matters here.
      **Acceptance:** CI fails if a future edit breaks the colcon build
      of the same source; the fixture stays parity-true.
- [ ] **210.F.3** `nros ws doctor` (+ siblings). Today's `nros doctor`
      doesn't know about `NROS_INTERFACE_SEARCH_PATH`. Add a check under
      the `nros ws` namespace: iterate workspace pkgs under the search
      path, validate each has a well-formed `package.xml` (parseable +
      non-empty `<name>`), warn on pkgs that look like rosidl pkgs but
      lack `<member_of_group>rosidl_interface_packages</member_of_group>`.
      Also surface staleness: if any patch block is older than its
      corresponding `.msg` files, fail loudly (mirror of `nros ws sync
      --check`).
      Sibling subcommands rounding out the namespace:
      * `nros ws list`   — print discovered msg pkgs + their source layer
                           (workspace vs AMENT vs bundled).
      * `nros ws status` — freshness check (same logic as sync --check,
                           but non-fatal — prints a one-line summary).
      * `nros ws clean`  — `rm -rf build/<pkg>/nros_generator_*` for each
                           workspace pkg; leaves user files alone.
      Lives in nros-cli; mirrors the smart Find-stub's "is it a msg pkg"
      heuristic.
      **Acceptance:** `nros ws doctor` from inside the local-msg-package
      fixture lists `local_msgs`, `extra_msgs`, `consumer` with the
      correct kind tag (msg/app); a deliberately broken `package.xml`
      makes it fail loudly. `nros ws status` prints a one-line summary
      with up-to-date vs stale counts.
- [ ] **210.F.4** Shadowing matrix verification. When a workspace pkg
      name collides with an AMENT-installed pkg (e.g. workspace
      `std_msgs` over `/opt/ros/.../std_msgs`), the workspace one
      should win + emit a `STATUS` line. The cmake-side smart Find-stub
      handles this; the `nros_workspace_interfaces()` bulk orchestrator
      handles intra-workspace shadowing. **Work:** add a smoke fixture
      `examples/templates/workspace-shadowing/` where the workspace
      defines a custom `std_msgs` that shadows the AMENT one; verify
      the build picks the workspace copy. Document the shadowing
      contract in `book/src/getting-started/your-own-msg-package.md`.
      **Acceptance:** the workspace `std_msgs` is the one linked into
      the consumer's binary (verified via `nm | grep std_msgs_...`).

## Acceptance criteria

- [x] A standard ROS msg package (verbatim `package.xml` +
      `rosidl_generate_interfaces(...)` CMakeLists.txt) builds against
      nano-ros via `add_subdirectory(src/my_msgs)` with **zero** edits to
      the msg pkg. (Met by 210.A.4 local-msg-package fixture.)
- [x] A consumer writes `find_package(my_msgs REQUIRED)` +
      `target_link_libraries(my_node my_msgs::my_msgs)` (verbatim upstream
      shape); the smart Find-stub does the codegen. (Met by 210.A.2 +
      210.A.4.)
- [ ] The same `src/` workspace builds with both `colcon build` and a
      nano-ros cmake build (different build systems, identical source).
      *(Asserted in README; CI gate is 210.F.2.)*
- [x] An app's `CMakeLists.txt` drops the N per-pkg codegen lines to one
      optional `nros_workspace_interfaces()` call. (Met by 210.B.2.)
- [x] A consumer pulling msgs from BOTH the workspace AND AMENT-installed
      pkgs works via one `find_package(<pkg>)` shape. (Met by 210.F.1
      mixed-workspace fixture.)
- [ ] `nros generate cpp --workspace ./` produces a full closure for a
      multi-pkg `src/`. *(Deferred → land with 210.D.)*
- [x] Book page `your-own-msg-package.md` walks the workflow end-to-end.
      (Met by 210.E.1.)
- [ ] Rust nodes consume the same workspace via `build.rs` calling a
      `nros-build-codegen::workspace()` helper. *(Stage 210.D.)*
- [ ] CI gate proves colcon-parity stays unbroken. *(Stage 210.F.2.)*

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
