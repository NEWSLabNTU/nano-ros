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

> **Post-Phase-218**: References below to nros-cli branches /
> `github.com/NEWSLabNTU/nros-cli` predate the Phase 218 monorepo
> merge — the CLI now lives in-tree at `packages/cli/` (the standalone
> repo is archived / read-only). Build via `just setup-cli`.

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
* 210.D.2 — convert `examples/native/rust/talker`. Done as part of
  210.E.3.d (whole rust-example fleet migration; talker landed via
  K.7.7 `fcbc498cc`).
* 210.E.3.c — Zephyr cpp examples. **DONE.** `zephyr/CMakeLists.txt`
  promotes `NROS_REPO_DIR` to `CACHE PATH` so the app's
  `include("${NROS_REPO_DIR}/cmake/compat/NrosRclcppCompat.cmake")`
  resolves; `cmake/compat/NrosRclcppCompat.cmake` aliases the Zephyr
  `zephyr_library_named(nros)` target to `NanoRos::NanoRosCpp` when
  `CONFIG_NROS_CPP_API` is set + skips the auto-`-include` of
  `nros/rclcpp_compat.hpp` (Zephyr libstdc++ subset can't pull
  `<memory>`/`<string>`). All 6 zephyr cpp examples (talker / listener
  / service-{server,client} / action-{server,client}) use the
  canonical `find_package(<msg_pkg>) + ament_target_dependencies`
  shape.
* 210.E.3.d — Rust example migration (D.2 talker + others). **DONE
  2026-06-03.** All 21 native rust examples now host the canonical
  `[patch.crates-io]` block in `Cargo.toml` (BEGIN/END nros-managed
  markers); the orphan `.cargo/config.toml [patch.crates-io]` table
  has been retired tree-wide for `examples/native/rust/`. Sweep
  landed across three waves: K.7.7 (talker / listener pub/sub),
  K.7.7.b (services + actions regular variants), and the Z.8 E.3.d
  sweep (`listener-rtic` pilot + 13 follow-up commits) covering
  every RTIC + async + custom-* + serial-* + lifecycle-node variant
  and the `custom-msg` special-case (full migration via
  hand-mirrored BEGIN/END block).
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

**Design.** [`docs/design/0023-codegen-workspace-discovery.md`](../design/0023-codegen-workspace-discovery.md).

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
- [x] **Acceptance:** the fixture's msg pkg's CMakeLists.txt has **zero
      nano-ros-specific lines**; the consumer's `find_package(local_msgs)`
      resolves through the smart stub and emits a target the consumer
      links against without any explicit codegen call. Closed by 210.A.4
      (`examples/templates/local-msg-package/`). Verified 2026-06-03:
      `src/local_msgs/CMakeLists.txt` carries only verbatim
      `rosidl_generate_interfaces(...)` upstream calls (zero
      `nros_*`); `src/consumer/CMakeLists.txt` resolves through
      `find_package(local_msgs)` + the smart Find-stub +
      `ament_target_dependencies`.

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
- [x] **Acceptance:** a user workspace at `$HOME/my_ros2_ws/src/{a,b}` (b
      depends on a; both rosidl-interface-pkgs) builds with a single
      `nros_workspace_interfaces()` call in the consuming app's
      CMakeLists.txt; the order is correct (topo-sorted); a shadowed pkg
      (workspace's `std_msgs` over AMENT's) takes the workspace one with
      a warning. Closed by 210.A.4 + 210.B.2 + 210.F.1 fixtures.
      Verified 2026-06-03: `examples/templates/local-msg-package/src/`
      ships `local_msgs` + `extra_msgs` (extra_msgs `<depend>local_msgs</depend>`
      per its `package.xml`) — exactly the {a,b} topology the bullet
      describes. `src/consumer/CMakeLists.txt` pulls both via the
      `nros_workspace_interfaces()` bulk path; topo-sort orders local
      before extra. Shadowing case covered by the same fixture (workspace
      msgs + AMENT-installed `std_msgs` / `sensor_msgs` mixed in one
      `find_package(<pkg>)` shape per 210.F.1 closure landing
      `0ddcc60fc`).

### 210.C — `nros codegen --workspace` + upstream header layout (nros-cli)
- [x] **210.C.1 OBSOLETE** — superseded by 210.D.1 `nros ws sync`.
      The deferral rationale (no current consumer until 210.D Rust
      build.rs helper lands) is moot: 210.D.1's `nros ws sync` carries
      its own workspace-walk + topo-sort internally (via the shared
      smart Find-stub resolution path), and consumer-side wiring (Rust
      `build.rs` → `nros-build::generate_run_plan`) hangs off the
      Phase 212.N.4 + 210.D.1 patch-block writer rather than a
      `nros codegen resolve-deps --workspace` invocation. Standalone
      verb landing is no longer the right interface.
- [x] **210.C.2 OBSOLETE** — superseded by 210.D.1 `nros ws sync` for
      Rust (`nros-build::generate_run_plan` + the patch-block writer)
      + 210.B.2 `nros_workspace_interfaces()` for C++. Both surfaces
      ship the workspace-walk + per-pkg codegen orchestration the
      deferred subcommand would have wrapped. The
      `nros generate cpp --workspace <dir>` shape isn't the way the
      user surface shipped; users invoke `nros ws sync` (Rust) or
      `nros_workspace_interfaces()` cmake fn (C++).
- [x] **210.C.3** Codegen already emits the upstream-style
      `<pkg>/msg/<name>.hpp` per-message header alongside the existing
      `<pkg>/<pkg>.hpp` umbrella — **already shipped under Phase 123.B.8**
      (`NROS_ALIAS_*_HPP_` forwarder headers). Verified in the
      `local-msg-package` fixture build dir; closes the 209.G iter 2
      cosmetic with no extra work.
- [x] **Acceptance: OBSOLETE per C.1/C.2 supersession** — the
      workspace-wide closure contract is satisfied by
      `nros_workspace_interfaces()` (cmake-side, bulk orchestrator
      under 210.B.2) for C++ and `nros ws sync` for Rust (210.D.1).
      The original spec named a `nros generate cpp --workspace`
      verb that didn't ship; the closure it described is delivered
      by the two surfaces above. The dual-header consumption
      contract (`<pkg>/msg/<name>.hpp` + `<pkg>/<pkg>.hpp`)
      already passes per 210.C.3 (verified in `local-msg-package`
      fixture build dir).

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

#### 210.D.2 — Convert one rust example — **DONE via 210.E.3.d**

`examples/native/rust/talker` is multi-RMW (zenoh/cyclonedds/xrce) with a
sibling cmake-cyclonedds variant on top. The patch table needed to live
alongside the cmake-driven cyclone build, not replace it; the migration
landed as part of the whole rust-example fleet sweep in 210.E.3.d
(K.7.7 wave, commit `fcbc498cc`). The orphan
`.cargo/config.toml [patch.crates-io]` has been retired tree-wide for
`examples/native/rust/` (E.3.d sweep, 2026-06-03).

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
- [x] `nros ws sync --check` from a fresh checkout (pre-first-sync) exits
      non-zero with a clear message. Closed by the 210.D.1 follow-up
      landing in nros-cli (`nros ws sync` carries `--check` flag with
      "Exit non-zero if any patch block is missing or stale (CI hook;
      also used by `nros ws status`)" — verified 2026-06-03 via
      `nros ws sync --help`).
- [x] Editing any `*.msg` file → `nros ws sync` regenerates ONLY the
      affected crate (mtime check) + leaves the patch block untouched (no
      churn). **Full close 2026-06-03** (Z.4 audit + Z.6 mtime guard
      landing):
      - **Patch-block-untouched** half MET — verified against
        `cp -r examples/templates/local-msg-package /tmp/lmp-test`:
        pre-sync the BEGIN/END nros-managed patch block md5-hashes to
        `661760a75c08eb89d86e4b20e48e266e`; editing
        `src/local_msgs/msg/Greeting.msg` (added `string new_field`)
        + re-running `nros ws sync` leaves the block byte-identical.
      - **mtime-check half MET** post-Z.6 — nros-cli `ee800694c`
        (`feat(210.D): per-pkg mtime guard in `nros ws sync``)
        added `pkg_is_up_to_date()` + `touch_witness()` to the
        per-pkg loop in `cmd/ws.rs`. Idle-re-sync prints zero
        `codegen <pkg>` lines; editing a single `.msg` regenerates
        ONLY that pkg's crate (plus any workspace dep that transitively
        consumes it — verified by the `sync_regenerates_after_dep_msg_edit`
        regression test). AMENT pkgs cache forever until their
        `share_dir` mtime moves forward (which it does on ROS
        upgrade). New `--force` flag bypasses both guards for
        toolchain bumps.
- [x] In a Cargo workspace (real `[workspace] members = [...]` at umbrella
      Cargo.toml), `nros ws sync` writes the patch block to the umbrella,
      NOT to the member pkg. **Verified 2026-06-03** (Z.4 audit) via a
      synthetic umbrella at `/tmp/ws-test/Cargo.toml` carrying
      `[workspace] members = ["src/rust_consumer"]` with the member's
      empty `[workspace]` marker dropped. `nros ws sync` reports
      `refreshed [patch.crates-io] block in /tmp/ws-test/Cargo.toml`
      (umbrella path); post-sync the umbrella carries 1 ×
      `[patch.crates-io]` + BEGIN/END markers and the member
      `src/rust_consumer/Cargo.toml` carries 0 × `[patch.crates-io]`
      + 0 × BEGIN/END markers — exactly the contract.

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
- [x] **210.E.3** Migrate the in-tree per-pkg `nros_generate_interfaces
      (<pkg>)` call sites to the `find_package(<pkg>) +
      ament_target_dependencies` shape. **DONE 2026-06-03** — all
      four sub-items (a/b/c/d) flipped to [x]; E.3.d (rust example
      `.cargo/config.toml [patch.crates-io]` retirement) closed the
      umbrella. Incremental — examples that explicitly want the
      bundled-pkg form keep it.
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
      * **210.E.3.c — Zephyr fixtures.** **DONE 2026-06-03.** Both
        prerequisites landed:
            1. `zephyr/CMakeLists.txt:25` promotes `NROS_REPO_DIR`
               to `CACHE PATH` so the example's
               `include("${NROS_REPO_DIR}/cmake/compat/
               NrosRclcppCompat.cmake")` resolves at app-config
               time.
            2. `cmake/compat/NrosRclcppCompat.cmake:82-84` aliases
               the Zephyr `zephyr_library_named(nros)` target to
               `NanoRos::NanoRosCpp` when `CONFIG_NROS_CPP_API` is
               set; the sibling `_NROS_COMPAT_ON_ZEPHYR` branch
               (lines 130+) skips the auto-`-include` of
               `nros/rclcpp_compat.hpp` since Zephyr libstdc++
               subset can't pull `<memory>`/`<string>` — users opt
               in manually if they really port rclcpp source.
        All 6 zephyr cpp examples (talker / listener / service-
        {server,client} / action-{server,client}) carry the canonical
        `find_package(<msg_pkg>) + ament_target_dependencies` shape.
        Zephyr cpp/cyclonedds/talker-aemv8r carve-out preserved
        per CLAUDE.md.
      * **210.E.3.d — Rust examples (`.cargo/config.toml [patch.crates-io]`
        deprecation + talker D.2 migration).** **DONE 2026-06-03.**
        All 21 native rust examples are on the canonical Cargo.toml
        BEGIN/END nros-managed `[patch.crates-io]` block; every
        orphan `.cargo/config.toml` carrying `[patch.crates-io]`
        has been deleted from `examples/native/rust/`. Sweep
        landed across three waves:
          - K.7.7 — talker / listener pub/sub (zenoh + cyclonedds
            CMake/Corrosion variants), commit `fcbc498cc`.
          - K.7.7.b — services + actions regular (non-RTIC,
            non-async) variants.
          - Z.8 E.3.d sweep — `listener-rtic` pilot (`71833a5e9`)
            + 13 follow-up chore commits covering every RTIC +
            async + custom-* + serial-* + lifecycle-node variant
            and the `custom-msg` special-case (full migration via
            hand-mirrored BEGIN/END block sibling reference, since
            the installed `nros` CLI does not yet expose
            `nros ws sync`).
        `cargo test -p nros-tests --test phase212_pre_212_files_forbidden
        --test phase212_m12_example_shape` stays green post-sweep.
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
- [x] **210.F.3** `nros ws doctor` (+ siblings) — **LANDED** in
      nros-cli `4aeb464` (`feat(210.F.3): nros ws {list,status,clean,
      doctor} siblings`). Verified 2026-06-03 via
      `nros ws --help` — all four sibling commands shipped:
      `list` (kind/name/dir per pkg), `status` (one-line freshness
      summary), `clean` (rm `generated/` + nros-managed patch block),
      `doctor` (lint workspace pkgs: malformed `package.xml`, missing
      `<member_of_group>rosidl_interface_packages</member_of_group>`,
      stale patch blocks). Today's `nros doctor`
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
- [x] **210.F.4** Shadowing matrix verification. When a workspace pkg
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
      **Landed 2026-06-03** — fixture at
      `examples/templates/workspace-shadowing/` (`9d67bb541`):
      workspace `src/std_msgs/` carries `Marker.msg` (unique field
      `string shadowed_marker`), upstream AMENT `std_msgs` ships no
      `Marker.msg` → consumer's `#include "std_msgs/msg/marker.hpp"`
      only links when the workspace copy wins. Regression at
      `packages/testing/nros-tests/tests/phase210_f4_shadowing.rs`
      (`a41bb8206`) drives cmake configure + build + `nm -C` grep
      for `nros_cpp_serialize_std_msgs_msg_marker` +
      `std_msgs::msg::Marker` symbols; PASSED in 62.86s with
      `/opt/ros/humble` sourced. Book chapter
      `book/src/getting-started/your-own-msg-package.md` §Shadowing
      contract (`2a6127579`) documents the layer order +
      `message(STATUS nros: find_package(<pkg>) -> ...)` signal +
      the compile-time fail-safe + the fixture as the reference.

## Acceptance criteria

- [x] A standard ROS msg package (verbatim `package.xml` +
      `rosidl_generate_interfaces(...)` CMakeLists.txt) builds against
      nano-ros via `add_subdirectory(src/my_msgs)` with **zero** edits to
      the msg pkg. (Met by 210.A.4 local-msg-package fixture.)
- [x] A consumer writes `find_package(my_msgs REQUIRED)` +
      `target_link_libraries(my_node my_msgs::my_msgs)` (verbatim upstream
      shape); the smart Find-stub does the codegen. (Met by 210.A.2 +
      210.A.4.)
- [x] The same `src/` workspace builds with both `colcon build` and a
      nano-ros cmake build (different build systems, identical source).
      Closed by 210.F.2 (`.github/workflows/colcon-parity.yml` ships
      the CI gate against `examples/templates/local-msg-package/src/`).
- [x] An app's `CMakeLists.txt` drops the N per-pkg codegen lines to one
      optional `nros_workspace_interfaces()` call. (Met by 210.B.2.)
- [x] A consumer pulling msgs from BOTH the workspace AND AMENT-installed
      pkgs works via one `find_package(<pkg>)` shape. (Met by 210.F.1
      mixed-workspace fixture.)
- [x] `nros generate cpp --workspace ./` produces a full closure for a
      multi-pkg `src/`. **OBSOLETE per 210.C.1+C.2 supersession** —
      the closure ships via `nros_workspace_interfaces()` cmake fn
      (C++) and `nros ws sync` CLI (Rust), not a `nros generate cpp
      --workspace` verb. The contract (full topo-sorted closure +
      dual-header consumption) is satisfied; the verb shape isn't.
- [x] Book page `your-own-msg-package.md` walks the workflow end-to-end.
      (Met by 210.E.1.)
- [x] Rust nodes consume the same workspace via `build.rs` calling a
      `nros-build-codegen::workspace()` helper. Closed by Phase
      212.N.4 / 210.D landing: `nros-build` crate ships
      `pub fn generate_run_plan(launch_file)` (renamed from the
      earlier `workspace()` spec to match the Phase 212.N.9
      `nros::main!()` proc-macro shape). Every Entry pkg
      `build.rs` consumes it (see e.g.
      `packages/testing/nros-tests/fixtures/multi_pkg_workspace_freertos/
      firmware/build.rs`); native Rust pkgs consume via
      `nros ws sync`'s patch-block writer instead, which is the
      Cargo-native sibling shape.
- [x] CI gate proves colcon-parity stays unbroken. Closed by
      `.github/workflows/colcon-parity.yml` (210.F.2).

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
