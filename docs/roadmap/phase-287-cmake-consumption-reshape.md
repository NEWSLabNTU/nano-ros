# Phase 287 — C/C++ consumption reshape: one-line bootstrap + uniform example CMake

Status: **In progress — 2026-07-11** (W1 done) · Implements #171 decision **D5** · Informs
RFC-0026 (examples), RFC-0018/0019 (C/C++ integration) · Sibling of phase-288
(source-distribution bootstrap, #171 D1/D2).

> **Goal.** A nano-ros C/C++ package's `CMakeLists.txt` should read the **same
> regardless of RMW or platform** — no per-leaf boilerplate, no micro-options
> the user must get right. Collapse the ~30-line `NANO_ROS_ROOT` guard +
> `enable_language(CXX)` RMW dance that is copy-pasted (and drifts) across 233
> example leaves into a single `nano_ros_bootstrap()` call, then migrate every
> example to the new shape. Same pass evaluates the Rust-side
> `[patch.crates-io]` consumption UX and picks the best handle.

## Why (verified 2026-07-11)

- `examples/native/c/talker/CMakeLists.txt` (representative) opens with a
  ~10-line `if(NOT DEFINED NANO_ROS_ROOT) … endif()` guard (three-tier resolve:
  `-DNANO_ROS_ROOT` → `$NROS_REPO_DIR` → relative walk-up), an
  `include(NanoRosWorkspace.cmake)` + `nano_ros_workspace_pkg_guard()`, and a
  hand-written `if(NROS_RMW STREQUAL "cyclonedds") enable_language(CXX) endif()`
  — a **micro-option**: the user must know Cyclone is C++ and that a C app
  linking it needs CXX enabled in this directory scope.
- **The boilerplate is not even uniform.** Diffing the guard block of three
  leaves against `native/c/talker` gives 24–34 differing lines each (walk-up
  depth, Cyclone branch present/absent, comments). It has already drifted — the
  worst state for copy-paste scaffolding.
- **278** tracked `examples/**/CMakeLists.txt`; **233** carry the guard.

The knobs the user should NOT own: root resolution, when to `enable_language(CXX)`,
which helper file to `include`. All are derivable.

## Target shape

Every leaf `CMakeLists.txt`, C or C++, zenoh or xrce or cyclonedds, native or
any embedded target, reduces to:

```cmake
cmake_minimum_required(VERSION 3.22)
project(c_talker LANGUAGES C CXX)

nano_ros_bootstrap()                 # resolve root, include helpers, CXX/RMW setup

nros_find_interfaces(LANGUAGE C)     # msg bindings closure
nano_ros_entry(NAME c_talker SOURCES src/main.c DEPLOY native)
nano_ros_link(c_talker)             # deps + platform link in one call
```

`nano_ros_bootstrap()` folds: the three-tier `NANO_ROS_ROOT` resolve, the
`include(NanoRosWorkspace.cmake)` + `nano_ros_workspace_pkg_guard()`, and the
RMW-conditional `enable_language(CXX)` (driven by the resolved `NROS_RMW`, not a
hand-written branch). `nano_ros_link()` folds the `target_link_libraries(<msg
bindings>)` + `nros_platform_link_app()` pair. `project()` stays the user's (it
names their target + languages) — everything else is the framework's.

## Waves — each is DESIGN then MIGRATE then VERIFY

### W1 — `nano_ros_bootstrap()` macro
- **Do:** add `nano_ros_bootstrap([ROOT <path>])` to a `cmake/` helper. It runs
  the three-tier root resolve (explicit `ROOT`/`-DNANO_ROS_ROOT` →
  `$NROS_REPO_DIR` → walk-up), guards a double-include, includes
  `NanoRosWorkspace.cmake`, calls `nano_ros_workspace_pkg_guard()`, and enables
  CXX **iff** the resolved RMW needs it (Cyclone; keep a documented escape for a
  project that wants CXX regardless). Idempotent; safe both solo and as a
  workspace sub-package (where the parent already provided the helpers).
- **Do:** add `nano_ros_link(<target>)` wrapping the msg-binding link +
  `nros_platform_link_app()`; the msg libs come from the interfaces resolved by
  `nros_find_interfaces` (already known), so the user need not name
  `std_msgs__nano_ros_c` by hand.
- **Acceptance:** one hand-written example (native/c/talker) rebuilds via the
  new 5-line shape, solo copy-out AND inside a workspace; C, C++, and all three
  RMW backends.
- **Done (2026-07-11):** `cmake/NanoRosBootstrap.cmake` ships
  `nano_ros_bootstrap([ROOT])` (root guard + `nano_ros_workspace_pkg_guard` +
  the RMW-conditional `enable_language(CXX)`) and `nano_ros_link(<target>)`
  (auto-links every `NROS_GENERATED_INTERFACE_LIBS` + `nros_platform_link_app`,
  so the user no longer names `<pkg>__nano_ros_<lang>`). Migrated + built
  `native/c/talker` (zenoh **and** cyclonedds — the CXX branch) and
  `native/cpp/talker` (zenoh), all rc=0, binaries link + run. The leaf went
  53→36 lines with the RMW/CXX micro-option and the hand-named msg lib gone; the
  root-resolve prelude is now identical for every leaf (a depth-agnostic
  walk-up, no per-leaf `../../../..`). `include_guard(GLOBAL)` makes the
  workspace-member case a no-op (the guard already returns early when nano-ros
  is imported).

### W2 — migrate every example leaf
- **Do:** rewrite the 233 example `CMakeLists.txt` to the target shape. Because
  the old boilerplate has drifted, this is a **generator/transform**, not a
  blind sed: parse each leaf's existing `nano_ros_entry(...)` / link args,
  re-emit the uniform head. Preserve any genuinely per-example lines (extra
  sources, custom `nros_find_interfaces LANGUAGE`, the cyclonedds descriptor TU
  hook) below the bootstrap.
- **Do:** fold the migration into the example scaffolder (`nros new`) so new
  leaves emit the new shape and never regrow the guard.
- **Acceptance:** `git grep 'NOT DEFINED NANO_ROS_ROOT' examples/` empty; a
  shape lint (sibling of `example_shape`) fails a leaf that hand-rolls the guard
  instead of calling `nano_ros_bootstrap()`.

### W3 — build-verify the migrated tree
- **Do:** rebuild the fixture set across platforms (native + the cross lanes
  that are provisioned) to prove the reshape is behaviour-preserving.
- **Acceptance:** the example fixtures build green; at least one copy-out per
  language (native) builds standalone with the new CMakeLists (the RFC-0026
  contract, as re-proven in #170).

### W4 — Rust consumption UX (`[patch.crates-io]`)
- **Design only in this phase; implement if cheap.** `[patch.crates-io]` works
  but is heavy on the consumer (a `# nros-managed` block in every
  `.cargo/config.toml`, `NROS_REPO_DIR` threading). Evaluate lighter handles
  against #171 D2 (pinned-source pull → manifest points at the entry manifest):
  path deps written by `nros sync`, a `[patch]` on a git source, a workspace
  `[patch]` inherited once at the consumer root, or a generated
  `.cargo/config.toml` include. Score on: lines the consumer writes by hand,
  robustness to a moved checkout, and IDE/`cargo` ergonomics.
- **Acceptance:** a short comparison in this doc + a recommendation; the winning
  handle either lands here (if a small change) or becomes W1 of a follow-up.

## Non-goals

- Distribution / bootstrap / how a user *obtains* nano-ros — phase-288 (#171
  D1/D2).
- Restoring `find_package(NanoRos)` + `install()` (retired Phase 140). D2 is a
  source-tree include model; not reintroduced.
- `component-poc` / `component-node-poc` / `transform-poc` — phase-242 owns them.
- Any book prose about publishing/future work (#171 D7).

## Acceptance (phase)

- Every example leaf's CMakeLists is the uniform shape; `NOT DEFINED
  NANO_ROS_ROOT` grep empty; new lint gates it.
- `just format` clean; example fixtures build; native copy-out per language
  builds standalone.
- `nros new` emits the new shape.
- W4 recommendation recorded.

## Sequencing

W1 (macro) → W2 (migrate, the bulk) → W3 (verify) → W4 (Rust UX, semi-independent;
can start after W1). Land W1+W2 together so the tree never carries two shapes.
