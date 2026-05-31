# Phase 210 — ROS-convention codegen + workspace discovery

**Goal.** A standard ROS 2 msg package — verbatim `package.xml` +
`msg/*.msg` + the canonical `rosidl_generate_interfaces(...)` CMakeLists.txt
— builds against nano-ros **unmodified**, regardless of whether it lives in
the user's local `src/<pkg>/` workspace or in an ament-installed prefix on
`AMENT_PREFIX_PATH`. We roll our own codegen, but the source layout and the
CMake call shape are ROS's, so the same `src/` tree builds under both
`colcon build` (rosidl's bindings) and a nano-ros build (ours). Subsumes the
Phase 209.E bulk-codegen item.

**Status.** MVP DONE (2026-05-30). Mixed-workspace cmake path proved
(2026-05-31, `0ddcc60fc`). Open work tracked under D, E.2/.3, F.

In-tree (landed on main):
* 210.A.1/.2/.3/.4 — rosidl wrapper + smart Find-stub + per-pkg delegators
  + local-msg-package fixture.
* 210.B.1/.2 — `NROS_INTERFACE_SEARCH_PATH` + `nros_workspace_interfaces()`
  bulk orchestrator with topo-sort.
* 210.E.1 — book page `your-own-msg-package.md`.
* 210.E.4 — deprecation comments on legacy `nros_generate_interfaces` +
  `nros_find_interfaces`.
* 210.F.1 — mixed-workspace fixture (workspace + AMENT msg deps in one
  consumer, multi-level dep closure cache `_NROS_PKG_<pkg>_*`).

nros-cli (`github.com/NEWSLabNTU/nros-cli` branch `phase-210-workspace-
codegen`, commit `41177dd`, pushed 2026-05-31):
* 210.B.3 — `nros ws env [<dir>] [--shell posix|fish]` (originally landed
  as `nros workspace env`; subcommand to be renamed to `nros ws env` as
  part of the 210.D ws-namespace lockdown — see Stage D below). Ships
  in next nros release.

Already-shipped:
* 210.C.3 — `<pkg>/msg/<name>.hpp` alias header (Phase 123.B.8).

Open (concrete acceptance below):
* 210.C.1 / C.2 — `nros codegen resolve-deps --workspace` +
  `nros generate cpp --workspace`. Blocked on 210.D needing them.
* 210.D.1/.2/.3 — `nros ws sync` subcommand + convert example +
  Rust mixed-workspace fixture sibling. **Design locked 2026-05-31**
  (no `build.rs` helper crate; `nros ws sync` writes auto-managed
  `[patch.crates-io]` block into the patch authority Cargo.toml).
* 210.E.2 — book page `porting-a-cpp-node.md` migration to the new
  shape.
* 210.E.3.a/.b/.c/.d — in-tree migration of native / qemu / zephyr /
  rust per-example codegen call sites.
* 210.F.2 — colcon-parity CI gate.
* 210.F.3 — `nros ws doctor` + `list` + `status` + `clean` siblings.
* 210.F.4 — shadowing matrix smoke fixture + book doc.

**CLI surface lockdown (2026-05-31):** workspace-level commands live
under the `nros ws` namespace (shortened from `nros workspace`). Planned
siblings: `env`, `sync`, `list`, `status`, `clean`, `doctor`. The
already-pushed `nros workspace env` (commit `41177dd` in nros-cli) gets
renamed in the same PR that adds `sync` (210.D.1).

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

* User runs **plain `cargo build`** — no `cargo nros build` wrapper, no
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

#### 210.D.1 — `nros ws sync` subcommand (nros-cli)

```
nros ws sync [<workspace>]
    [--build-dir <dir>]     # default ./build
    [--dry-run]
    [--check]               # exit non-zero if any patch table is stale
```

Behavior:
1. Resolve workspace root (cwd or first ancestor containing `src/`).
2. Scan `src/*/package.xml` for msg pkgs (member_of_group=rosidl_interface_
   packages OR msg/srv/action dirs).
3. Recurse + topo-sort, pulling AMENT_PREFIX_PATH msg pkgs that workspace
   pkgs depend on into the same closure.
4. For each pkg, codegen its Rust crate into
   **`build/<pkg>/nros_generator_rs/<pkg>/rust/`** (colcon convention —
   verified from real colcon build: see `build/<pkg>/rosidl_generator_rs/
   <pkg>/rust/{Cargo.toml,src/lib.rs}`; nano-ros uses `nros_generator_rs`
   prefix to co-exist with colcon's `rosidl_generator_rs` in the same
   `build/` dir without collision).
5. Auto-detect patch authority per Rust consumer pkg:
   * Standalone pkg (`[workspace]` empty marker in own Cargo.toml) → patch
     goes in that file.
   * Cargo workspace member → walk up, patch goes in the workspace root
     `Cargo.toml`.
6. Write/refresh a **delimited `[patch.crates-io]` block** in the patch
   authority. Block is clearly marked + idempotent — only content
   between the markers is rewritten.

Block shape:
```toml
# === BEGIN nros-managed [patch.crates-io] (2026-05-31T10:23:45Z) ===
[patch.crates-io]
local_msgs    = { path = "build/local_msgs/nros_generator_rs/local_msgs/rust" }
extra_msgs    = { path = "build/extra_msgs/nros_generator_rs/extra_msgs/rust" }
std_msgs      = { path = "build/std_msgs/nros_generator_rs/std_msgs/rust" }
geometry_msgs = { path = "build/geometry_msgs/nros_generator_rs/geometry_msgs/rust" }
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

#### 210.D.2 — Convert one rust example

`examples/native/rust/talker` migrates from the per-example hand-managed
`.cargo/config.toml [patch.crates-io]` chunk to: user runs `nros ws
sync && cargo build`. The patch table in Cargo.toml is auto-managed.
Deprecate the ad-hoc `fixtures-build.sh` rust codegen loop for pkgs
that adopt the new shape.

#### 210.D.3 — Rust mixed-workspace fixture sibling

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

- [ ] `nros ws sync` from `examples/templates/local-msg-package/` codegens
      4 msg pkgs into `build/<pkg>/nros_generator_rs/<pkg>/rust/` and
      writes the patch block into `src/rust_consumer/Cargo.toml`.
- [ ] `cd src/rust_consumer && cargo build` (plain cargo, no wrapper) links
      against all four msg families via the patch table — `nm` shows
      `local_msgs::*`, `extra_msgs::*`, `std_msgs::*`, `geometry_msgs::*`.
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
- [ ] **210.E.2** Update existing
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
      (<pkg>)` call sites (sample: `examples/native/{cpp,rust}/*/`,
      `examples/qemu-arm-*/{cpp,rust}/*/`) to the
      `find_package(<pkg>) + target_link_libraries` shape. Incremental —
      examples that explicitly want the bundled-pkg form keep it.
      **Sub-items**, each one PR-sized:
      * **210.E.3.a** — Native fixtures: `examples/native/cpp/{talker,
        listener,service-*,action-*}/`. Switch from
        `nros_generate_interfaces(std_msgs LANGUAGE CPP SKIP_INSTALL)` to
        `find_package(std_msgs REQUIRED)` + `target_link_libraries(...
        std_msgs::std_msgs)`. Acceptance: `just native test` green.
      * **210.E.3.b** — QEMU embedded fixtures: `examples/qemu-arm-{baremetal,
        freertos,nuttx}/{cpp,rust}/...`. Same swap. Acceptance: per-platform
        `just <plat> build-fixtures` green.
      * **210.E.3.c** — Zephyr fixtures: `examples/zephyr/cpp/...`. Same
        swap; the zephyr module exposes `NROS_REPO_DIR` so the smart
        Find-stub include path resolves. Acceptance: `just zephyr
        build-cpp` green.
      * **210.E.3.d** — Rust examples: deprecate the per-example
        `.cargo/config.toml` `[patch.crates-io]` chunk in favour of the
        `nros-build-codegen` build.rs helper (210.D). Migrate
        `examples/native/rust/*/` first; QEMU/Zephyr Rust last.
        Acceptance: `cargo build` in each migrated example pulls
        bindings via the build.rs only — no manual patch table.
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
- [ ] **210.F.2** colcon-parity CI gate. The fixture's `src/` tree is
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
