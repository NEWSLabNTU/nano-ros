# Phase 287 — C/C++ consumption reshape: ament-aligned `find_package(nano_ros)`

Status: **In progress — 2026-07-11** (W1 + W2a landed as interim; design converged
on RFC-0048) · Implements #171 decision **D5** + **RFC-0048** · Informs RFC-0026
(examples), RFC-0018/0019 (C/C++ API), RFC-0014 (`nros setup`), RFC-0023 (ament
codegen) · Sibling of phase-288 (source bootstrap, #171 D1/D2, **complete**).

> **Goal.** A nano-ros C/C++ package is written in the **ament_cmake convention**
> and its `CMakeLists.txt` is **byte-identical across every platform** (native,
> FreeRTOS, NuttX, ThreadX, Zephyr). The per-package delta (board, RMW) lives in
> `package.xml` `<export>`; resolution is source-backed via `find_package(nano_ros)`
> + `nano_ros_ROOT` (no install). Full design: **RFC-0048**.
>
> ```cmake
> cmake_minimum_required(VERSION 3.24)
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

### W6 remaining — [migration] embedded / Zephyr / workspace / bespoke
- **Do:** embedded native leaves (freertos/nuttx/threadx — need W5 presets),
  Zephyr leaves (keep `find_package(Zephyr)` + Kconfig; the verb hides the
  add_library-into-`app`, but the leaf is NOT byte-identical to native — Zephyr
  owns the build), workspace roots + members (composition via `nros plan`), the 6
  bespoke/own-msg native leaves, and `nros new` emitting the ament shape.
- **Blocked-by:** W5 embedded presets (cross-compile leaves) + per-shape design
  confirmation (Zephyr non-uniformity, workspace verb mapping).

## Waves (RFC-0048 implementation)

Grouped by the four deliverables you asked for: **impl · migration · testing ·
old-path removal.**

### W3 — [impl] `nano_rosConfig.cmake` + the two verbs
- **Do:** ship an in-tree `nano_rosConfig.cmake` found via `nano_ros_ROOT`
  (exported by `activate.sh`). It wraps the W1 bootstrap (import + RMW/CXX),
  registers the msg-codegen redirect in `CMAKE_FIND_PACKAGE_REDIRECTS_DIR`
  (RFC-0048 §2), and defines `nano_ros_add_executable` (standalone entry — exe on
  native/FreeRTOS/NuttX/ThreadX, `add_library`-into-`app` on Zephyr) and
  `nano_ros_add_node` (workspace component library). `ament_target_dependencies`
  shim links the generated `*__nano_ros_<lang>`. `nano_ros_generate_interfaces`
  for msg pkgs (RFC-0048 §5). Bump the floor to `cmake_minimum_required(3.24)`.
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

### W5 — [impl] toolchain automation: `nros setup` presets + `nros init` — NEXT
- **Do:** `nros setup <board>` writes `~/.nros/presets/<board>.json`
  (`toolchainFile` + `nano_ros_ROOT`); new `nros init` verb generates the user
  project's `CMakePresets.json` including those fragments (RFC-0048 §6). Export
  `nano_ros_ROOT` from `activate.sh`.
- **Acceptance:** on a machine with only the pinned checkout + bootstrap, `nros
  setup <board>` → `nros init` → `cmake --preset <board>` cross-configures with no
  hand-set `CMAKE_TOOLCHAIN_FILE` / `-Dnano_ros_ROOT`.
- **Open design question (blocks the embedded arm):** the board→CMake-toolchain
  mapping is today **hardcoded per just-recipe**, and an embedded configure needs
  more than `toolchainFile` — `NUTTX_DIR` / `THREADX_DIR` / `NETX_DIR` / the
  provisioned-SDK config dirs / `_NANO_ROS_CODEGEN_TOOL` / (cyclone)
  `NROS_RMW_CYCLONEDDS_MSG_TO_IDL` all come from the SDK store the recipes resolve
  at build time. Where should this data live so `nros setup` can emit it — new
  `nros-board.toml` fields (`cmake_toolchain`, `cmake_cache_vars`), a table in the
  CLI, or read back from the `sdk_store` provision result? This is RFC-0014
  provisioning territory — resolve with the maintainer before implementing. The
  **native** preset (`nano_ros_ROOT` only, no toolchain) is unblocked and can land
  first; `nros init` for a native project is trivial.

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
- **Design + implement if cheap.** Evaluate lighter handles than the per-consumer
  `# nros-managed` `[patch.crates-io]` block (path deps written by `nros sync`, a
  git-source `[patch]`, a workspace `[patch]` inherited once, a generated
  `.cargo/config.toml` include) against #171 D2. Score on hand-written lines,
  robustness to a moved checkout, IDE ergonomics.
- **Acceptance:** comparison + recommendation recorded; the winner lands here or
  spawns a follow-up.

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
