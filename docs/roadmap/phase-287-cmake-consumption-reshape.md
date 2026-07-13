# Phase 287 — C/C++ consumption reshape: ament-aligned `find_package(nano_ros)`

Status: **In progress — 2026-07-12** · Implements #171 decision **D5** +
**RFC-0048** · Informs RFC-0026 (examples), RFC-0018/0019 (C/C++ API), RFC-0014
(`nros setup`), RFC-0023 (ament codegen) · Sibling of phase-288 (source bootstrap,
#171 D1/D2, **complete**).

**Landed:** W1+W2a (interim bootstrap) · W3 (find_package + verbs) · W4 (package.xml
deploy tuple) · W5 (nros setup presets + nros init, shape C′) · W6 native (all 27
standalone leaves, incl. the 6 bespoke/own-msg) + `nros new` C/C++ ament shape ·
**W6 embedded (49 canonical leaves migrated to the native-identical shape;
`find_package(nano_ros)`, 0 on the old bootstrap/entry shape)** · W7 native-shape
lint (example_shape Test 8) · W9 (evaluated → recommend option E, follow-up).

**Remaining (verified against origin 2026-07-13):**
- **W6 Zephyr** — `examples/zephyr/*` still on the old shape (0 `find_package(nano_ros)`);
  keeps `find_package(Zephyr)` + Kconfig (NOT byte-identical — Zephyr owns the build),
  the `nano_ros_add_executable` verb hides the `add_library`-into-`app` arm.
- **W6 workspace** — IN PROGRESS. The **6 C-workspace node members** are migrated
  (`9c20918fc`): `nano_ros_workspace_pkg_guard()` + `nano_ros_node_register()` →
  `find_package(nano_ros)` + `nano_ros_add_node(<name> CLASS <c> [TYPED] <srcs>)`.
  **Key unblocker landed:** `nros plan`/`nros metadata` statically parses the CMake
  verbs to build the composition graph — `workspace.rs::parse_add_node_call` now
  recognises `nano_ros_add_node`'s positional grammar (without it `nros plan` fails
  "missing source metadata"). The C workspace CONFIGURES (posix, `CFG_RC=0`).
  **Remaining:** the cpp + other workspaces' members (same mechanical pattern, now
  that the verb + parser support it); the workspace **root**
  (`nano_ros_workspace()` orchestrator) + **Entry pkgs** (`nano_ros_entry(...
  LAUNCH ...)` multi-node carriers — the composition layer, a distinct slice; the
  `nano_ros_add_executable` verb would need LAUNCH/TYPED passthrough + an entry
  parser); the component `scaffold_c/cpp` template
  (`cargo-nano-ros/src/scaffold.rs`); and a full workspace BUILD (only *configure*
  is verified so far).
- **W7 full cross-matrix** — `just build-test-fixtures && just test-all` on an idle
  box (all cross-toolchains are provisioned: `just doctor tier=all` all `[OK]` after
  `just rmw_zenoh setup`). Not a correctness gate.
- **W8 (old-path removal)** — retire the public `nano_ros_bootstrap` / `nano_ros_link`
  / `nano_ros_deploy` macros + `NanoRosBootstrap.cmake`; BLOCKED until W6 Zephyr +
  workspace stop calling them (`git grep` the markers before removing).
- **W9 impl** — option E (single `include` of a `nros sync`-generated central
  `nros-patch.toml`); independent follow-up.

> **Goal.** A nano-ros C/C++ package is written in the **ament_cmake convention**
> and its `CMakeLists.txt` is **byte-identical across every platform** (native,
> FreeRTOS, NuttX, ThreadX, Zephyr). The per-package delta (board, RMW) lives in
> `package.xml` `<export>`; resolution is source-backed via `find_package(nano_ros)`
> + `nano_ros_ROOT` (no install). Full design: **RFC-0048**.
>
> ```cmake
> cmake_minimum_required(VERSION 3.22)
> project(freertos_c_talker LANGUAGES C CXX)
> find_package(nano_ros REQUIRED)
> find_package(std_msgs REQUIRED)
> nano_ros_add_executable(freertos_c_talker src/Talker.c)   # exe; lib-into-app on Zephyr
> ament_target_dependencies(freertos_c_talker std_msgs)
> install(TARGETS freertos_c_talker DESTINATION lib/${PROJECT_NAME})
> ament_package()
> ```

## Why (verified 2026-07-11)

233 example `CMakeLists.txt` carried a ~10-line `NANO_ROS_ROOT` guard **drifted
24–34 lines apart**, a per-leaf `enable_language(CXX)` micro-option, and **three
different shapes** (native `nano_ros_entry` + hand-named msg lib; embedded
`set(NANO_ROS_PLATFORM/BOARD)` + `nano_ros_deploy()`; Zephyr Kconfig/west). A ROS 2
porter recognises none of it. RFC-0048 collapses all three to the ament shape.

## Landed interim (W1 + W2a — 2026-07-11)

Shipped before the design converged on `find_package`. Their **machinery is
reused**, their **user-facing spelling is superseded** by RFC-0048.

- **W1 (`a356811bb`)** — `cmake/NanoRosBootstrap.cmake`: `nano_ros_bootstrap()`
  (root resolve + workspace import + hidden RMW/CXX) and `nano_ros_link()`
  (auto-link `NROS_GENERATED_INTERFACE_LIBS` + platform). → becomes the
  **internals of `nano_rosConfig.cmake`** (RFC-0048 §1).
- **W2a (`9fa51e1b3`)** — 25 native leaves migrated to `nano_ros_bootstrap()` via
  `scripts/docs/migrate-example-cmake.py`. → **re-migrated** to the ament shape in
  W6; the transform is extended, not thrown away.

## Landed (W3 + W4 — 2026-07-11)

- **W3 (`ce15b3a37`)** — `nano_rosConfig.cmake` (repo root, found via
  `nano_ros_ROOT`) + `cmake/NanoRosVerbs.cmake` (`nano_ros_add_executable` /
  `nano_ros_add_node` / `nano_ros_generate_interfaces`). `find_package(<msg>)` is
  **validate-only** under `find_package(nano_ros)` (gated by
  `NROS_FIND_PACKAGE_VALIDATE_ONLY`); the verb owns codegen in the source-inferred
  language via `nros_find_interfaces` (the `nros codegen resolve-deps` path that
  resolves well-known ROS pkgs with no in-tree bundle). **Verified:**
  `native/{c,cpp}/talker` in the full RFC-0048 ament shape configure + build +
  link (C bindings for the C leaf, C++ for the C++ leaf).
- **W4 (`1be4fb1b8`)** — `cmake/NanoRosPackageXml.cmake`
  (`nano_ros_read_package_export`) parses `<export><nano_ros deploy= board=
  rmw=/>`; the config reads it before importing nano-ros (deploy→platform,
  rmw→RMW) and the verbs default DEPLOY/BOARD from it. **Verified:** a native leaf
  builds with the tuple in `package.xml` and **no `DEPLOY` in CMakeLists**.

  Design notes captured in agent memory (`project_phase287_w3_ament_cmake`): the
  language snapshot from `ENABLED_LANGUAGES` is fragile — source inference in the
  verb is the reliable signal.

## Landed (W5 native slice + W6 native canonical — 2026-07-11)

- **W5 native (`07860c905`)** — `activate.{sh,fish}` export `nano_ros_ROOT`; a
  sourced shell's `find_package(nano_ros)` resolves via CMake's `<pkg>_ROOT` env
  with no `-Dnano_ros_ROOT`. The embedded-preset arm remains blocked on the
  data-location decision (above).
- **W6 native canonical (`07860c905`)** — 21 native standalone leaves migrated to
  the ament shape via `scripts/docs/migrate-example-cmake-ament.py` (conservative:
  only exact-canonical bodies; bespoke + own-msg leaves skipped). Platform delta in
  `package.xml <export><nano_ros deploy="native"/>`. **Verified** via the real
  `just native build-c` / `build-cpp` recipe paths (both green).

  **Scope finding:** the "byte-identical CMakeLists across every example" goal
  holds for the CANONICAL role leaves (talker/listener/action-*/service-*), but the
  pedagogical `custom-*` examples deliberately carry extra CMake (custom platform
  ref lib, custom transport threads, safety compile flags) and cannot be
  byte-identical — they migrate by hand or stay bespoke. Own-interface packages
  (`custom-msg`) are the `nano_ros_generate_interfaces` (§5) case.

## In flight (W6 embedded — 2026-07-12, option A decided)

**Shape decision (maintainer, 2026-07-12): embedded canonical role leaves are
NATIVE-IDENTICAL** — same `CMakeLists.txt` AND same portable `src/main.{c,cpp}`
as the native counterpart; the platform delta is ONLY the package.xml
`<nano_ros deploy= board= rmw=/>` tuple. Component-model pedagogy stays in the
workspace examples + phase-242 POCs.

What makes one source portable (all landed):
- `NROS_HOST_POSIX` was never defined by any build path — the root cause of
  every native C main hardcoding `NROS_APP_MAIN_REGISTER_POSIX()` (a copy-out
  to a board then died at link with undefined `app_main`). The posix
  `nros_platform_link_app` now defines it; the auto `NROS_APP_MAIN_REGISTER()`
  picks POSIX on host / VOID (`app_main`) on FreeRTOS/NuttX/ThreadX / Zephyr's
  `int main` under `__ZEPHYR__`.
- Portable connect seam: `NROS_ENTRY_LOCATOR` + `NROS_ENTRY_DOMAIN_ID` defaults
  in `<nros/app_main.h>` (C) and applied inside `nros::init` (C++). Precedence:
  explicit arg > env (`$NROS_LOCATOR`/`$ROS_DOMAIN_ID`, hosted) > baked macro >
  local default. The NanoRosEntry board gate bakes the locator (QEMU slirp
  `tcp/10.0.2.2:7447`, threadx-linux loopback) and ferries `-DNROS_DOMAIN_ID`
  to `NROS_ENTRY_DOMAIN_ID`.
- Native cpp mains converted to `nros_app_main` + `NROS_APP_MAIN_REGISTER()`.

**Migrated (49 leaves)** via `scripts/docs/migrate-embedded-example-native-shape.py`:
qemu-arm-freertos (12), qemu-arm-nuttx (12), qemu-riscv-nuttx (1),
qemu-riscv64-threadx (12), threadx-linux (12) — c+cpp × role leaves. Old
component sources git-rm'd; harness exe names de-prefixed
(`freertos_c_talker` → `c_talker`, …) in `fixtures/binaries/*.rs`; e2e markers
were already the native strings ("I heard:", "Waiting for messages"). Plus 3
easy bespoke native leaves (logging, safety-listener c+cpp) → ament shape.

**Verified (zenoh fixture matrix + rtos e2e, 2026-07-12).** All four lanes
BUILD green (freertos / nuttx-arm / threadx-linux / threadx-riscv64, c+cpp;
nuttx-riscv talker too). rtos e2e (nuttx serialized `-j 1` — parallel cold-boot
QEMU flake):

| lane (c/cpp)     | pubsub | service | action |
|------------------|--------|---------|--------|
| freertos         | ✓✓     | ✓✓      | ✗✗ #179 |
| nuttx            | ✓✓     | ✓/✗cpp  | ✗✗ #179 |
| threadx-linux    | ✓✓     | ✓✓      | ✗✗ #179 |
| threadx-riscv64  | ✓✓     | ✓✓      | (no c/cpp action tests) |

Every ✓ lane above is delivery the harness could never see before — the old
role images baked `tcp/10.0.2.2:7447` while the harness listens per-(variant,
lang) (`7551`–`7675`), so pre-287 these C/C++ rtos_e2e lanes could not even
connect. #179 (embedded get-result reply deserialize, shared rmw-zenoh-cffi
path) is the one open runtime bug; cyclone side-lanes blocked by #177
(threadx-linux dup ts symbols) — both filed.

Landed fixes the migration forced out (all load-bearing beyond it):
- `nano_rosConfig.cmake` writes the package.xml tuple into the CACHE —
  reconfigures used to fall back to root's cached `posix` and cross leaves
  died at `Threads_FOUND`.
- `NanoRosEntry` gate: `_nra_board_active` accepts board-name (workspace) OR
  deploy/platform-token (tuple) spellings, normalizing legacy
  `threadx_linux`-style `-DNANO_ROS_PLATFORM` values.
- Platform link is now `nros_platform_link_app_once` + DEFERRED to leaf-scope
  end (wrappers in NanoRosEntry.cmake): the double gate+`nano_ros_link` call
  was fatal on NuttX (dup `<name>_build` target), and an immediate call ran
  before `ament_target_dependencies` → NuttX's include/lib text files missed
  the interface closure (`std_msgs.h: No such file`).
- Per-cell fixture identity: `NROS_ENTRY_LOCATOR` baked for 48 cells
  (freertos rehosted to `tcp/192.0.3.1:<port>` + rtos_e2e freertos switched to
  the board-net slirp launcher — default slirp never answers the 192.0.3.1
  gateway ARP, pcap-proven); pair members get distinct IP/MAC last octets
  (freertos `@NROS_ENTRY_IP_LAST@` template param; threadx-rv64 the existing
  `NROS_APP_NET_IP_LAST`) — identical baked identities seed identical PRNGs →
  identical zenoh ZIDs → the router collapses the pair to ONE peer and
  delivery silently dies.
- `nros::init` locator precedence fixed: arg > env (hosted) > baked
  `NROS_ENTRY_LOCATOR` > local default — the hosted branch's eager
  `tcp/127.0.0.1:7447` had shadowed threadx-linux's baked port.
- Minimal-libcpp / freestanding portability: `enable_language(ASM)` in the
  riscv64-qemu board overlay (leaf no longer declares ASM; cmake silently
  dropped the port `.S` files), `#ifdef _IOLBF` around `setvbuf`, global C
  spellings for stdio/signal/strtoll in the cpp mains and
  `nros-rmw-cyclonedds/src/descriptors.cpp`, host-only gates for
  env/argv parsing.

### W6 remaining — [migration] embedded / Zephyr / workspace / bespoke
- **Do:** embedded native leaves (freertos/nuttx/threadx — need W5 presets),
  Zephyr leaves (keep `find_package(Zephyr)` + Kconfig; the verb hides the
  add_library-into-`app`, but the leaf is NOT byte-identical to native — Zephyr
  owns the build), workspace roots + members (composition via `nros plan`), the 6
  bespoke/own-msg native leaves, and `nros new` emitting the ament shape.
- **Blocked-by:** ~~W5 embedded presets~~ (landed 2026-07-12 — cross-compile
  leaves are unblocked) + per-shape design confirmation (Zephyr non-uniformity,
  workspace verb mapping).

## Waves (RFC-0048 implementation)

Grouped by the four deliverables you asked for: **impl · migration · testing ·
old-path removal.**

### W3 — [impl] `nano_rosConfig.cmake` + the two verbs
- **Do:** ship an in-tree `nano_rosConfig.cmake` found via `nano_ros_ROOT`
  (exported by `activate.sh`). It wraps the W1 bootstrap (import + RMW/CXX),
  prepends the compat find-stubs so `find_package(<msg>)` validates (RFC-0048 §2 —
  the verb owns codegen; **not** the 3.24 redirect mechanism), and defines
  `nano_ros_add_executable` (standalone entry — exe on native/FreeRTOS/NuttX/
  ThreadX, `add_library`-into-`app` on Zephyr) and `nano_ros_add_node` (workspace
  component library). `ament_target_dependencies` shim links the generated
  `*__nano_ros_<lang>`. `nano_ros_generate_interfaces` for msg pkgs (RFC-0048 §5).
  Floor stays `cmake_minimum_required(3.22)` (`nano_ros_ROOT` is 3.12+).
- **Acceptance:** `find_package(nano_ros REQUIRED)` + `find_package(std_msgs)` +
  `nano_ros_add_executable` builds `native/c/talker` (zenoh, xrce, cyclonedds) and
  `native/cpp/talker`; a Zephyr leaf builds via the `add_library`-into-`app` arm;
  a workspace member builds via `nano_ros_add_node`.

### W4 — [impl] `package.xml <export>` deploy tuple
- **Do:** define + parse `<export><nano_ros deploy=… board=… rmw=…/></export>`
  (RFC-0048 §4); `find_package(nano_ros)` + the verbs read it from the invoking
  package. `deploy="native"` omits board.
- **Acceptance:** an embedded leaf builds with an EMPTY-of-platform CMakeLists
  (all platform data in `package.xml`); switching `board=` in `package.xml`
  reconfigures the toolchain path (via the preset, W5) with no CMake edit.

### W5 — [impl] toolchain automation: `nros setup` presets + `nros init` — LANDED (`07a2fdc64`, shape C′)
- **Design resolved (shape C′, RFC-0048 §6):** no `${repo}` templating (rejected —
  complicates parsing, assumes the tree layout). One board-intrinsic field
  `[board.cmake] toolchain_file` in `nros-board.toml` (a plain in-repo relative
  path `nros` resolves against its own root); the SDK `*_DIR` cache-vars stay
  inside the platform CMake modules (default from `${CMAKE_CURRENT_LIST_DIR}`);
  `nros setup` emits the preset with **literal absolute paths** (repo root + store
  bin dir substituted at emit time), the store compiler bin carried on the preset's
  `environment.PATH`.
- **Do:**
  1. nuttx platform module: default `NUTTX_DIR` / `NUTTX_FFI_CRATE_DIR` from
     `${CMAKE_CURRENT_LIST_DIR}` (mirror the threadx module) so they leave the -D
     set. (native W5 slice — `activate.{sh,fish}` `nano_ros_ROOT` export — landed.)
  2. `BoardDescriptor` (`board_descriptor.rs`): add `cmake: Option<BoardCmake>`
     with `toolchain_file`; add `[board.cmake]` to the cross-compile board tomls
     (freertos-mps2-an385, nuttx-qemu-arm, riscv nuttx, threadx-qemu-riscv64).
  3. `nros setup <board>`: after provisioning, write `~/.nros/presets/<board>.json`
     (toolchainFile abs, `nano_ros_ROOT`, `CMAKE_BUILD_TYPE`, `environment.PATH`
     store bin). Native boards emit the toolchain-less variant.
  4. new `nros init` verb: generate the project `CMakePresets.json` that `include`s
     `~/.nros/presets/*`.
- **Acceptance:** on a machine with only the pinned checkout + bootstrap, `nros
  setup <board>` → `nros init` → `cmake --preset <board>` cross-configures with no
  hand-set `CMAKE_TOOLCHAIN_FILE` / `-Dnano_ros_ROOT`. Native preset + `nros init`
  verified end-to-end; embedded presets verified by emitted-JSON shape + (where a
  toolchain is provisioned) a configure.

### W6 — [migration] every example to the ament shape
- **Do:** extend `scripts/docs/migrate-example-cmake.py` to emit the RFC-0048
  shape and cover all classes: re-migrate the 25 native leaves off
  `nano_ros_bootstrap`; migrate embedded (freertos/nuttx/threadx), Zephyr,
  workspace roots + members, and interface pkgs. Move each leaf's deploy config
  from CMake into `package.xml <export>`. Fold the shape into `nros new` so new
  leaves emit it.
- **Acceptance:** every example CMakeLists is the RFC-0048 shape; `git grep
  'NOT DEFINED NANO_ROS_ROOT\|nano_ros_bootstrap\|set(NANO_ROS_PLATFORM'
  examples/` empty.

### W7 — [testing] shape gate + full-matrix rebuild
- **Do:** update the `example_shape` lint — a leaf must `find_package(nano_ros)`
  (no guard block, no `nano_ros_bootstrap`, no in-CMake `nano_ros_deploy`);
  deploy tuple present in `package.xml`. Rebuild the fixture matrix across every
  provisioned platform lane; a native copy-out per language builds standalone
  (the RFC-0026 contract, as re-proven in #170) via `nros init` + `cmake
  --preset`.
- **Acceptance:** the new lint is red when a leaf hand-rolls the old shape
  (verified by removal, not just green); every platform fixture lane builds.

### W8 — [old-path removal] retire the superseded machinery
- **Do:** once W6 lands, delete the now-dead user-facing paths: the guard-resolve
  support in `NanoRosWorkspace.cmake` app-entry usage, the `nano_ros_deploy()`
  CMake call surface (→ package.xml), and the interim `nano_ros_bootstrap()` /
  `nano_ros_link()` **public** macros (keep the logic inside `nano_rosConfig`).
  Update RFC-0026 + the book C/C++ pages + `docs/reference/c-api-cmake.md` to the
  ament shape (no publish/future-work prose, #171 D7).
- **Acceptance:** `git grep 'nano_ros_bootstrap\|nano_ros_link\|nano_ros_deploy('
  -- ':!docs/roadmap/archived'` returns only the config internals; docs describe
  only the ament shape.

### W9 — [impl] Rust consumption UX (`[patch.crates-io]`) — semi-independent

**Evaluated 2026-07-12.** Today `nros sync` writes ~12 `# nros-managed`
`[patch.crates-io]` lines into each Rust leaf's `.cargo/config.toml`, each a
RELATIVE path (`../../../../packages/core/nros`). The Cargo.toml stays
registry-style (`nros = { version = "*" }`), so a leaf reads like a stock crate.

| Option | Hand lines | Moved checkout | Copy-out (moved leaf) | Offline / D2 | IDE |
| --- | --- | --- | --- | --- | --- |
| **A. current — N relative-path patches** (sync-written) | 0 (sync) | re-run sync | breaks depth → re-run sync | ✅ | ✅ path deps |
| B. path deps in Cargo.toml | ~12 | edit manifest | breaks (path in manifest) | ✅ | ✅ |
| C. git-source `[patch]` | ~1 | n/a | ✅ | ❌ needs a git URL + network | ✅ |
| D. workspace `[patch]` inherited | ~1 (root) | re-point root | ❌ copy-out has no workspace | ✅ | ✅ |
| **E. single `include` of a sync-generated central patch** | 1 (the include) | re-run sync (one file) | one fragile include line vs 12 | ✅ | ✅ |

**Recommendation: E.** Keep the sync-managed source-path patches (A's D2 +
offline + IDE strengths), but consolidate them: `nros sync` generates ONE central
`nros-patch.toml` (absolute paths to the checkout) and each leaf's committed
`.cargo/config.toml` carries a single `include = ["…/nros-patch.toml"]`. Net: the
committed per-leaf surface drops from ~12 fragile lines to 1, and a checkout move
re-points one generated file that every leaf shares (vs re-syncing each). B/C/D
are rejected — B/D break the standalone copy-out contract (RFC-0026), C violates
#171 D2 (offline source distribution). The include line keeps A's relative-path
fragility, but 1 line ≪ 12.

**Status: recommendation recorded; implementation is a FOLLOW-UP** (not "cheap" —
it changes `nros sync`'s emit, rewrites every Rust leaf's `.cargo/config.toml`, and
needs a cargo-build sweep across the Rust example matrix to verify `[patch]`
resolution through the `include`). Filed as its own slice so it doesn't gate the
C/C++ waves. Independent of W3–W8.

## Non-goals

- Distribution / how a user *obtains* nano-ros — phase-288 (#171 D1/D2).
- Reviving Phase-140 `install()` on a system prefix — resolution is source-backed
  via `nano_ros_ROOT` (RFC-0048).
- `component-poc` / `component-node-poc` / `transform-poc` — phase-242.
- Book prose about publishing / future work (#171 D7).

## Acceptance (phase)

- Every example CMakeLists is the RFC-0048 ament shape, byte-identical across
  platforms; deploy in `package.xml`; the old grep-set empty.
- `nros setup` presets + `nros init` make `cmake --preset <board>` work with no
  hand-set toolchain.
- `example_shape` lint gates the shape (verified failing on the old shape).
- Fixture matrix builds; native copy-out per language builds standalone.
- `nros new` emits the ament shape.

## Sequencing

W3 (config + verbs) → W4 (package.xml) → W5 (presets/init) form the impl; they
gate W6 (migration). W7 (testing) after W6. W8 (removal) last, once nothing uses
the old paths. W9 (Rust UX) is independent — any time after W3.
