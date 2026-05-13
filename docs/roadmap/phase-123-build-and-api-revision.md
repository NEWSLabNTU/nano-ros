# Phase 123 — Build distribution + C++ API revision

**Goal.** Reduce time-to-first-publish for an rclcpp engineer trying
nano-ros. Two threads: how the library reaches the user (source vs
SDK) and how the C++ API looks once they have it.

**Status.** In flight (branch `phase-123-build-and-api-revision`).

**Priority.** P1 — gating the migration-guide chapter.

**Depends on.** Phase 122 (closed). Builds on the install layout
introduced in 119.3 + the two-layer API surface frozen in 122.

## Why now

Walk-through (see session log 2026-05-13) of "rclcpp engineer
tries nano-ros":

- `cmake -S . -B build && cmake --install` works end-to-end on
  POSIX, but ships ~284 MB of artefacts (all variants) and
  requires a Rust nightly toolchain on the user machine.
- A new project needs `find_package(NanoRos)` + two
  `nros_generate_interfaces` calls + `target_link_libraries` —
  acceptable.
- The runtime C++ API has 10+ friction points vs rclcpp: silent
  error logging, mandatory `NROS_APP_MAIN_REGISTER_POSIX()`,
  manual `nros::init(locator, domain_id)`, out-param
  `create_node` / `create_publisher`, raw `void*` timer
  callbacks, hand-rolled `while (ok()) spin_once(100)` loop, no
  QoS in `create_publisher`.

These are answerable. The phase splits into two work streams.

## Stream A — Build distribution (locked design)

After three rounds of trade-off review (session 2026-05-13) the
shape is **source-ship via git, no prebuilt SDK matrix**. Rationale
captured in [§Stream A decisions](#stream-a-decisions) below.

### Stream A decisions

1. **Source ship only.** Git clone is the user entry. No tarball
   release, no prebuilt SDK archives, no `crates.io` publishing
   for nros umbrella crates. Rust runtime + zenoh-pico/cyclonedds
   /micro-XRCE C deps live in submodules — `crates.io` can't ship
   them cleanly.
2. **Pinned shallow clone is the recommended path.**
   `git clone --depth=1 --branch=vX.Y.Z` against a release tag.
3. **Static `.a` only.** No `.so`. RTOS / bare-metal targets need
   static; POSIX users tolerate the same.
4. **Three-archive split per target.** Builds inside the user's
   install prefix produce three orthogonal pieces (per Phase 121
   canonical platform-cffi + RMW-cffi ABI work):
   - `libnros_c.a` + `libnros_cpp.a` (per target × C/C++ API) —
     platform-agnostic, RMW-agnostic.
   - `libnros_platform_<plat>.a` (per target × platform).
   - `libnros_rmw_<rmw>.a` (per target × rmw).
   User CMake links exactly one of each via
   `nano_ros_link_platform(target)` + `nano_ros_link_rmw(target)`.
5. **CMake function form** for linking. Not transparent targets.
   Functions hide the `--start-group / --end-group` ordering.
6. **`nros` CLI is the single source of truth** for setup.
   Replaces the per-platform `just <plat> setup` recipes. Justfile
   stays as contributor convenience that calls `nros setup`. Users
   never need `just`.
7. **Selective submodule fetch.** `config/submodule-deps.toml`
   maps each submodule to the `(target, platform, rmw)` set that
   needs it. `nros setup --target=X --platform=Y --rmw=Z` fetches
   only the required subset. `.gitmodules` + git gitlinks own
   URL + SHA (standard git tooling).
8. **Pattern A workspace layout** is the recommended integration
   shape — nano-ros sits as a colcon-discoverable package inside
   the user's workspace's `src/`. One nano-ros source tree per
   workspace, never duplicated per user package.
9. **Workspace-shared codegen cache** via `NANO_ROS_GEN_CACHE_DIR`.
   `std_msgs__nano_ros_{c,cpp}` static libs and the `std_msgs`
   cargo crate generated once per workspace, reused by every user
   package.
10. **Three audiences, one entry.** rclcpp / rclc / rclrs users
    share the same `git clone` + `tools/setup.sh` + `colcon build`
    flow. Per-language differences are 5–10 lines of CMake or
    Cargo.toml.

### Single user entry point

```bash
mkdir -p ~/ros2_ws/src && cd ~/ros2_ws/src
git clone --depth=1 --branch=v1.0.0 https://github.com/NEWSLabNTU/nano-ros.git
cd ~/ros2_ws
./src/nano-ros/tools/setup.sh --target=posix --rmw=zenoh
colcon build
source install/setup.bash
```

`tools/setup.sh` is the bootstrap:

```bash
#!/bin/bash
# 1. Install rustup + nightly if missing.
command -v cargo >/dev/null || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain none --profile minimal
# 2. Install or update the nros CLI.
command -v nros >/dev/null || cargo install --path packages/codegen/packages/nros-cli --locked
# 3. Delegate.
exec nros setup "$@"
```

`nros setup` (the canonical entry):

```
nros setup [--target=TRIPLE] [--platform=PLAT] [--rmw=RMW] [--rust-workspace]
nros setup --doctor
nros setup --list-targets
nros setup --add-rmw=xrce       # extend an existing setup
```

Reads `config/submodule-deps.toml`, fetches required submodules
via `git submodule update --init --depth=1 <path>`, installs
cross-toolchains for the chosen target if missing, optionally
writes a workspace `Cargo.toml` for Rust users.

### Stream A work items

- [x] **123.A.1 — Binary self-containedness audit.** Done.
  Findings: `docs/research/phase-123-A1-binary-audit.md`.
  Verdict: **API contract decoupled** (platform-cffi + RMW-cffi
  vtables wired); **physical archive monolithic** (one big
  `staticlib` per RMW combo, 28 MB unstripped, embeds zenoh-pico
  C + compiler_builtins + nros-* in 468 objects). Decoupling
  requires Cargo build-output refactor into three separate
  `staticlib` crates (new sub-item **123.A.1.x** below, ordered
  before A.3). Source-path leakage also identified; mitigation
  is `RUSTFLAGS=--remap-path-prefix=$HOME=.` + post-build
  `strip --strip-debug` (28 MB → 16 MB, leakage from many lines
  to 12 panic strings).
- [ ] **123.A.1.x — Cargo staticlib split.** Refactor
  `packages/core/nros-c/Cargo.toml` to drop RMW + platform
  feature flags from its own dep graph; carve
  `nros-rmw-<x>-staticlib` and `nros-platform-<y>-staticlib`
  wrapper crates each emitting their own `.a`. Resolve
  `compiler_builtins` ODR (likely `--allow-multiple-definition`
  on the linker or feature-version-locking). Pre-req for the
  matrix collapse advertised in A.3 + A.10.
- [x] **123.A.2 — `config/submodule-deps.toml`.** Authored.
  21 submodules classified across axes `required` (codegen) +
  `rmw.{zenoh,xrce,dds,cyclonedds}` + `platform.{posix,freertos,
  threadx,nuttx,zephyr,bare-metal,esp32}` + `reference.{px4,
  tracing}`. Dev-only paths (zenohd router, XRCE agent, ESP32
  QEMU fork) gated behind `--with-dev`; reference paths
  (PX4 1GB, Tonbandgeraet) opt-in via `--with-reference`.
  Cross-checked against `just/*.just` `git submodule update`
  invocations. Drives A.3 (`nros setup` CLI), A.4
  (`tools/setup.sh`).
- [ ] **123.A.3 — `nros setup` CLI.** Implementation crate
  `packages/codegen/packages/nros-cli/src/setup/`. Argument
  parser, manifest reader, submodule fetcher, cross-toolchain
  installer, optional Cargo workspace writer.
- [ ] **123.A.4 — `tools/setup.sh` bootstrap.** ~30-line bash.
  Auto-rustup + `cargo install` + `exec nros setup`.
- [ ] **123.A.5 — `cmake/bootstrap.cmake`.** CMake auto-runs
  the same logic when invoked without `setup.sh` first. Same
  one-shot rustup install, idempotent.
- [x] **123.A.6 — `nano_ros_link_platform` / `_link_rmw`
  CMake functions.** Landed. New module
  `packages/core/nros-c/cmake/NanoRosLink.cmake` exposes:
    - `nano_ros_link_platform(target [PLATFORM <plat>])`
    - `nano_ros_link_rmw(target [RMW <rmw>])`
  Resolution chain: explicit arg → `NANO_ROS_DEFAULT_*`
  workspace cache var → `NANO_ROS_*` from the install. Fails
  loudly with a clear error if user asks for a value the
  installed NanoRos wasn't built with (today's single-
  combined-archive constraint). Sets a per-target property
  `NANO_ROS_PLATFORM` / `NANO_ROS_RMW` for downstream tooling.
  When the decoupled three-archive layout lands (Stream A
  follow-ups), the function body switches from the no-op
  validate-only path to linking
  `NanoRos::Platform::<plat>` + `NanoRos::Rmw::<rmw>` without
  user CMakeLists changing.
  Pulled into `NanoRosConfig.cmake` so `find_package(NanoRos)`
  exposes the functions automatically. Installed alongside
  the other CMake config files.
  Verified: walkthrough talker rebuilt with the function
  form (`nano_ros_link_platform(my_cpp_talker)` +
  `nano_ros_link_rmw(my_cpp_talker)`); mismatched-RMW request
  fails with a guided error; workspace-default cache var
  resolution works.
- [ ] **123.A.7 — Workspace-shared codegen cache.** Honour
  `NANO_ROS_GEN_CACHE_DIR` in `NanoRosGenerateInterfaces.cmake`
  + `cargo-nano-ros`. Per-workspace singletons for
  `std_msgs__nano_ros_{c,cpp}` libs and `std_msgs` cargo crate.
- [ ] **123.A.8 — Migrate `just <plat> setup` recipes** to call
  `nros setup --target=...` instead of duplicating logic.
- [ ] **123.A.9 — `installation.md` rewrite.** Pattern A as the
  default, source-build-via-git-clone as the only path. Drop
  references to tarballs / SDK archives.
- [ ] **123.A.10 — Multi-package workspace example.** Add
  `examples/multi-package-workspace/` with mixed C / C++ / Rust
  packages sharing one `src/nano-ros/`. Real working
  `colcon build`.

### Open question (Stream A)

- **Publish `nros-core` to crates.io?** That subset is pure Rust,
  no C deps — type-level types, codegen scaffolds, message trait.
  Lets third-party Rust crates depend on `nros-core = "0.1"`
  without the git-clone dance. Full `nros` stays git-only.
  Recommend yes for `nros-core` only; lock in v1.

## User workflows (expected, locked)

All three audiences share the same outer flow: git clone +
`tools/setup.sh` + `colcon build`. Per-language deltas are minimal
and visible inside individual `package.xml` / `CMakeLists.txt` /
`Cargo.toml` files.

### A — Workspace bootstrap (one-time per workspace)

```bash
mkdir -p ~/ros2_ws/src && cd ~/ros2_ws/src
git clone --depth=1 --branch=v1.0.0 https://github.com/NEWSLabNTU/nano-ros.git
cd ~/ros2_ws
./src/nano-ros/tools/setup.sh --target=posix --rmw=zenoh
# Rust users only:
./src/nano-ros/tools/setup.sh --target=posix --rmw=zenoh --rust-workspace
```

Step-by-step:

1. `tools/setup.sh` detects no rustup → installs nightly.
2. `cargo install nros-cli --locked` puts the `nros` CLI on
   `$PATH`.
3. `nros setup --target=posix --rmw=zenoh` reads
   `config/submodule-deps.toml`, fetches only
   `third-party/zenoh-pico/` (shallow), installs no extra
   cross-toolchain (POSIX = host).
4. `--rust-workspace` (optional) writes `~/ros2_ws/Cargo.toml`
   with `[workspace] + [workspace.dependencies] + [patch.crates-io]`
   so user Rust packages see `nros = { workspace = true }`.

For embedded targets:

```bash
./src/nano-ros/tools/setup.sh --target=esp32 --rmw=zenoh
# CLI installs xtensa-esp32-none-elf rust target + the
# xtensa GCC toolchain + fetches the ESP-IDF submodule.
```

Same one-liner — only the args change.

### B — Per-package skeleton (after bootstrap)

#### C++ package (rclcpp-shaped audience)

**Directory:**

```
src/pkg_a/
├── package.xml
├── CMakeLists.txt
└── src/main.cpp
```

**`package.xml`**:

```xml
<?xml version="1.0"?>
<package format="3">
  <name>pkg_a</name>
  <version>0.1.0</version>
  <description>My rclcpp-shaped node</description>
  <maintainer email="you@example.com">You</maintainer>
  <license>Apache-2.0</license>
  <depend>nano-ros</depend>
  <depend>std_msgs</depend>
  <export>
    <build_type>cmake</build_type>
  </export>
</package>
```

**`CMakeLists.txt`** (8 lines):

```cmake
cmake_minimum_required(VERSION 3.16)
project(pkg_a LANGUAGES CXX)
find_package(NanoRos REQUIRED CONFIG)
nano_ros_generate_interfaces(std_msgs LANGUAGE CPP SKIP_INSTALL)
add_executable(my_node src/main.cpp)
nano_ros_link_platform(my_node)
nano_ros_link_rmw(my_node)
target_link_libraries(my_node PRIVATE std_msgs__nano_ros_cpp NanoRos::NanoRosCpp)
```

**`src/main.cpp`** (~25 lines for a 1 Hz pub/sub talker, with
lambda timer + `NROS_INFO` + `nros::spin()` from Stream B —
see Stream B for the full snippet).

#### C package (rclc-shaped audience)

**Directory + `package.xml`:** identical to C++ except no
`<build_type>` overrides.

**`CMakeLists.txt`** (8 lines, 3 token swaps from the C++ form):

```cmake
cmake_minimum_required(VERSION 3.16)
project(pkg_a LANGUAGES C)
find_package(NanoRos REQUIRED CONFIG)
nano_ros_generate_interfaces(std_msgs LANGUAGE C SKIP_INSTALL)
add_executable(my_node src/main.c)
nano_ros_link_platform(my_node)
nano_ros_link_rmw(my_node)
target_link_libraries(my_node PRIVATE std_msgs__nano_ros_c NanoRos::NanoRosC)
```

The C codegen produces a separate, source-incompatible binding
format from upstream rclc (the `ROSIDL_GET_MSG_TYPE_SUPPORT`
macro shape isn't mirrored). rclc users get first-class C
support; existing rclc source files are NOT drop-in.

#### Rust package (rclrs-shaped audience)

**Directory:**

```
src/pkg_a/
├── package.xml
├── Cargo.toml
└── src/main.rs
```

**`package.xml`**: same `<depend>nano-ros</depend>` +
`<depend>std_msgs</depend>` declarations.

**`Cargo.toml`** (workspace dependency form — feature set defined
at workspace level):

```toml
[package]
name = "pkg_a"
version = "0.1.0"
edition = "2024"

[dependencies]
nros = { workspace = true }
std_msgs = { path = "../../build/nros-gen-cache/std_msgs" }
log = "0.4"
env_logger = "0.11"
```

**`src/main.rs`** uses `nros::Executor` + `register_timer` +
`spin_blocking` (existing two-layer API; documented in
[Two-Layer API](../../book/src/concepts/two-layer-api.md)).

**Workspace `Cargo.toml`** (auto-generated by `nros setup
--rust-workspace`):

```toml
[workspace]
resolver = "2"
members = ["src/pkg_a", "src/pkg_b"]

[workspace.dependencies]
nros = { path = "src/nano-ros/packages/core/nros",
         default-features = false,
         features = ["rmw-zenoh-cffi", "platform-posix", "ros-humble"] }

[patch.crates-io]
# Auto-generated. One [patch.crates-io] entry per nano-ros
# workspace crate. Refreshed by `nros setup --refresh-cargo-patches`.
```

### C — Build + run

```bash
cd ~/ros2_ws
colcon build
source install/setup.bash
ros2 run pkg_a my_node                              # for C / C++ packages
cargo run --bin pkg_a                               # alternative for Rust
```

`colcon build` walks `src/`, builds packages in dependency order
(nano-ros first, then user packages), respects
`<depend>nano-ros</depend>` in each user `package.xml`. The
shared codegen cache means `std_msgs__nano_ros_{c,cpp}` and the
`std_msgs` cargo crate are each built **once** per workspace.

### D — Multi-language workspace (real-world pattern)

```
~/ros2_ws/src/
├── nano-ros/                      ← one source tree
├── motor_driver/                  ← C (rclc-shape firmware module)
│   ├── package.xml                ← <depend>nano-ros</depend>
│   ├── CMakeLists.txt             ← LANGUAGE C, NanoRos::NanoRosC
│   └── src/main.c
├── controller/                    ← C++ (rclcpp-shape control loop)
│   ├── package.xml                ← <depend>nano-ros</depend>
│   ├── CMakeLists.txt             ← LANGUAGE CPP, NanoRos::NanoRosCpp
│   └── src/main.cpp
└── safety_monitor/                ← Rust (rclrs-shape safety check)
    ├── package.xml                ← <depend>nano-ros</depend>
    ├── Cargo.toml                 ← workspace = true
    └── src/main.rs
```

All three link the same `install/nano-ros/lib/libnros_platform_posix.a`
+ `libnros_rmw_zenoh.a`. Three API archives (`libnros_c.a` +
`libnros_cpp.a` + `nros` rlib) live side-by-side in the install
prefix and don't conflict.

colcon-cargo-ros2 handles Rust packages; colcon's ament_cmake
handles C/C++ packages. Both call the same shared codegen cache.

### E — Switching RMW / platform later

```bash
# Inside a workspace already set up for zenoh:
~/ros2_ws/src/nano-ros/tools/setup.sh --add-rmw=xrce
# Fetches third-party/micro-XRCE-DDS-Client/, rebuilds nano-ros
# to install both libnros_rmw_zenoh.a + libnros_rmw_xrce.a.
```

Per-package CMakeLists.txt overrides default:

```cmake
nano_ros_link_rmw(my_node RMW xrce)   # this target uses xrce
```

Other packages in the workspace keep using zenoh. Constraint:
**Rust workspaces fix the RMW + platform at workspace level**
because Cargo's feature unification doesn't allow mixed-RMW
linking. Document this explicitly; recommend separate Cargo
workspaces for mixed-RMW Rust use cases.

### F — Limitation matrix

| Scenario | C/C++ | Rust |
|---|---|---|
| One workspace, multiple packages, same RMW | ✅ | ✅ |
| One workspace, multiple packages, mixed RMW | ✅ per-target | ❌ split workspaces |
| One workspace, multiple packages, mixed platform | ✅ per-target | ❌ split workspaces |
| Mix C, C++, Rust packages | ✅ | ✅ |
| Embedded target | ✅ | ✅ |
| `crates.io` published reuse | n/a | `nros-core` only (planned) |

## Stream B — C++ API revision

### Friction inventory (rclcpp → nros)

| # | Friction | Concrete cost to user | Proposed fix |
|---|---|---|---|
| B.1 | `NROS_TRY_LOG` silent unless user `#define`s it | First-run silent failures | Default the macro to `fprintf(stderr, ...)`. Opt-OUT via `#define NROS_TRY_LOG(...) (void)0` for embedded. |
| B.2 | `NROS_APP_MAIN_REGISTER_POSIX()` boilerplate | Bottom-of-file magic; "why?" | Provide a default `main()` in a header-only optional include; user opts in by including `<nros/posix_main.hpp>` and defining `nros_app_main`. Today's macro stays for embedded `_start` injection. |
| B.3 | Hardcoded `tcp/127.0.0.1:7447` fallback in `nros::init` | Production code reads `getenv` itself | Make `nros::init()` (no args) read `$NROS_LOCATOR` / `$ROS_DOMAIN_ID` itself. Match rclcpp's "init from environment" mental model. |
| B.4 | Out-param `create_*` style | `nros::Node n; nros::create_node(n, "name");` vs `auto n = rclcpp::Node::make_shared("name")` | Keep out-param for zero-alloc. Add a `nros::Node::make("name")` value-return convenience that constructs into an `aligned_storage` slot. Same for `Publisher<M>::make(node, "/topic")`. |
| B.5 | Manual spin loop | `while (nros::ok()) nros::spin_once(100);` | Add `nros::spin(node, options)` blocking entry (mirror of `rclcpp::spin`). Internally drives the existing loop. |
| B.6 | Timer takes `void*` + C fn pointer | Hand-roll a context struct, cast | Add a `node.create_timer(period, [&]() { ... })` overload that captures into a typed callback box; falls back to the C-pointer form on `NROS_CPP_NO_STD` builds. |
| B.7 | No QoS argument in `create_publisher` | Defaults baked in; user has to set after | Add overload `create_publisher(pub, "/topic", QoS::reliable().keep_last(10))`. Already supported by FFI — just surface it. |
| B.8 | Generated header naming `std_msgs.hpp` (flat) | Inconsistent with rclcpp `<std_msgs/msg/int32.hpp>` | Codegen emits per-message headers `std_msgs/msg/int32.hpp` + an umbrella `std_msgs.hpp` that includes them. Migration is the cargo subcommand's job. |
| B.9 | No `RCLCPP_INFO` logging macro | Users mix `std::printf` / `fprintf` | Add `NROS_INFO(...)`, `NROS_ERROR(...)`, etc. that route through `NROS_TRY_LOG`'s sink. |
| B.10 | `Result` vs exception | rclcpp throws; users forget to check | Document loudly; the new `NROS_TRY` family + auto-`NROS_TRY_LOG` covers this. |

### Work items

- [x] **123.B.1 — Default `NROS_TRY_LOG` to stderr.** Landed.
  `nros/result.hpp` now defaults the macro to a `fprintf(stderr,
  ...)` formatter when `NROS_CPP_STD` or `__STDC_HOSTED__` is
  set; embedded `__STDC_HOSTED__=0` falls through to the silent
  cast-to-void. Override semantics unchanged (still opt-out via
  user `#define NROS_TRY_LOG(...) ((void)0)`).
- [x] **123.B.2 — `nros::spin()` blocking entry.** Landed.
  New free function in `nros/nros.hpp` overloads the existing
  `spin(duration_ms, ...)` — no-arg form blocks until
  `nros::ok()` returns false. Matches `rclcpp::spin(node)`.
  Friend decl added to `Node`.
- [x] **123.B.3 — Env-aware `nros::init()`.** Landed.
  On hosted builds (`NROS_CPP_STD` or `__STDC_HOSTED__`),
  the existing `init(locator = nullptr, domain_id = 0)`
  overload falls through to `$NROS_LOCATOR` /
  `$ROS_DOMAIN_ID` when its args are null/zero. Hard-coded
  fallback `tcp/127.0.0.1:7447` kept for the
  no-env-set case. `cstdlib` only pulled in under the
  hosted gate.
- [x] **123.B.6 — `create_publisher` QoS overload.** Already
  present (`Result Node::create_publisher(out, topic, qos)`
  default-arg, plus fluent `nros::QoS::reliable().keep_last(N)`
  / sensor_data / services presets). Verified, no code change
  needed — promoted to documented requirement.
- [x] **123.B.7 — `NROS_INFO` / `NROS_WARN` / `NROS_ERROR` /
  `NROS_DEBUG` macros.** New `nros/log.hpp`. Same hosted /
  embedded split as `NROS_TRY_LOG`; routes through a single
  `NROS_LOG_SINK(level, file, line, fmt, ...)` macro that
  the user can override. `NROS_DEBUG` is a no-op under
  `NDEBUG`. Pulled into the umbrella `nros/nros.hpp` so
  `#include <nros/nros.hpp>` is enough.
- [ ] **123.B.4 — `Publisher<M>::make` / `Node::make` convenience.**
  Deferred. `Node` is already movable; the out-param + Result
  pattern works. A value-returning factory needs either a
  `Result<T>` template (the current `Result` is non-generic) or
  a tagged-union return. Punted to a follow-up phase once the
  std_compat layer grows an `expected`-like wrapper. Out-param
  remains the canonical zero-alloc API for embedded.
- [x] **123.B.5 — Lambda-capable timer + subscription callbacks.**
  Already shipped via `nros/std_compat.hpp` —
  `nros::create_timer(node, timer, std::chrono::ms, [&](){…})`
  and `create_timer_oneshot` + `create_guard_condition` take
  `std::function<void()>` and box the closure inline. Activated
  by `-DNROS_CPP_STD=1`. Verified with the walkthrough talker
  (37-line lambda variant). Documented in the migration guide.
  Subscription lambda variant not yet shipped — tracked as
  follow-up.
- [x] **123.B.6 — `create_publisher` QoS overload.** Surfaced.
- [x] **123.B.7 — `NROS_INFO` / `NROS_ERROR` macros.** Shipped.
- [ ] **123.B.8 — Per-message codegen headers
  (ROS-style aliases).** Deferred. Codegen today writes
  `nano_ros_cpp/std_msgs/msg/std_msgs_msg_int32.hpp` (flat with
  package prefix). ROS-2-conventional `<std_msgs/msg/int32.hpp>`
  needs a generator pass that emits a one-line alias header
  per message that `#include`s the prefixed file. Self-contained
  but requires plumbing through `GeneratedCppPackage` +
  `cargo-nano-ros` writer + tests. Punted to its own commit
  once the migration-guide chapter surfaces a real
  user-facing need.

## Stream order

Stream B (API ergonomics) lands first — 6 / 8 items already
shipped on this branch (B.1, B.2, B.3, B.5, B.6, B.7). B.4 / B.8
deferred with rationale. Stream B is additive; existing examples
already work.

Stream A (build distribution) follows after the platform-cffi /
RMW-cffi canonical-ABI work on phase-121 merges. Order within
Stream A:

1. **A.1** — binary audit (gate). Confirm decoupling works.
2. **A.2** — author `config/submodule-deps.toml`.
3. **A.6** — `nano_ros_link_platform` / `_link_rmw` CMake
   functions (unblocks Pattern A docs).
4. **A.3** — `nros setup` CLI (the load-bearing piece).
5. **A.4** — `tools/setup.sh` bootstrap.
6. **A.5** — `cmake/bootstrap.cmake` auto-rustup.
7. **A.7** — workspace-shared codegen cache.
8. **A.8** — migrate justfile recipes.
9. **A.10** — multi-package workspace example.
10. **A.9** — installation.md rewrite.

## Acceptance criteria

1. A fresh rclcpp / rclc / rclrs engineer runs three commands
   (`git clone --depth=1`, `tools/setup.sh`, `colcon build`)
   and has a 1 Hz publisher running in under 12 minutes
   cold-cache.
2. The minimal user package is ≤ 10 lines of CMake (or
   `Cargo.toml`) + ≤ 30 lines of `main.cpp` (or `main.c` /
   `main.rs`) — same line count across all three languages.
3. One nano-ros source tree per workspace. No per-package
   duplication.
4. Source build is the only distribution path. Pattern A
   (in-workspace colcon package) is the documented default.

## Notes

- `package.xml` stays. Required for codegen; aligns with ROS
  convention. `colcon-cargo-ros2` already builds on it.
- Stream B changes are additive — existing examples don't
  need a sweep. The migration-guide chapter consumes the new
  ergonomics in a follow-up phase.
- Two limitations documented for users up-front: (1) mixed-RMW
  Rust workspaces require splitting into two cargo workspaces
  (Cargo feature unification), (2) `crates.io` publishing for
  full `nros` is blocked by C/C++ deps — `nros-core` may
  publish as the pure-Rust subset.
