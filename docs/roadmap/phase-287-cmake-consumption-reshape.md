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

### W5 — [impl] toolchain automation: `nros setup` presets + `nros init`
- **Do:** `nros setup <board>` writes `~/.nros/presets/<board>.json`
  (`toolchainFile` + `nano_ros_ROOT`); new `nros init` verb generates the user
  project's `CMakePresets.json` including those fragments (RFC-0048 §6). Export
  `nano_ros_ROOT` from `activate.sh`.
- **Acceptance:** on a machine with only the pinned checkout + bootstrap, `nros
  setup <board>` → `nros init` → `cmake --preset <board>` cross-configures with no
  hand-set `CMAKE_TOOLCHAIN_FILE` / `-Dnano_ros_ROOT`.

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
