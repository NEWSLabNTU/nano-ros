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
6. **`tools/setup.sh` is the single source of truth** for setup
   (pivoted 2026-05-14 from a Rust `nros setup` CLI — see A.3 for
   why). Bash orchestrates submodule fetch + rustup + apt.
   `just setup` is a one-line shim that `exec`s it; per-platform
   `just <plat> setup` recipes likewise. Users never need `just`.
7. **Selective submodule fetch.** `config/submodule-deps.toml`
   maps each submodule to the `(target, platform, rmw)` set that
   needs it. `tools/setup.sh --target=<plat>-<rmw>` fetches only
   the required subset. `.gitmodules` + git gitlinks own URL +
   SHA (standard git tooling).
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
./src/nano-ros/tools/setup.sh --target=posix-zenoh
colcon build
source install/setup.bash
```

`tools/setup.sh` shape:

```
tools/setup.sh [--target=<plat>-<rmw>] [--rust-workspace]
                [--with-dev] [--with-reference=<name>]
                [--doctor] [--list-targets]
```

- Reads `config/submodule-deps.toml`.
- Splits the target into `<platform>-<rmw>` and unions the
  `required` + `platform.<plat>` + `rmw.<rmw>` path sets.
- Runs `git submodule update --init --depth=1 <path>` for each.
- Installs rustup (if missing) + the target's Rust triple via
  `rustup target add`.
- On Linux, ensures the right apt cross-toolchain packages
  (`gcc-arm-none-eabi`, etc.) via `apt-get` or surfaces a
  manual-install message.
- With `--rust-workspace`, writes a workspace `Cargo.toml` for
  the colcon-package layout.

No Rust binary required; bash + standard POSIX tools only.
TOML parsing uses a minimal grep/sed reader because the manifest
schema is intentionally flat (key-only sections + `paths = [...]`
arrays).

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
- [~] **123.A.1.x — Decouple platform + RMW from `nros-c` archive.**
  Multi-step. Phase 121 already migrated 4 RTOS platforms +
  XRCE/Cyclone RMWs to standalone C-port `.a` files; this
  sub-plan finishes the migration so `libnros_c_*.a` is the
  pure C-API archive and platforms/RMWs ship as separate
  `lib<…>.a`s linked at the user's CMake site via
  `nano_ros_link_platform` / `nano_ros_link_rmw`.

    - [x] **123.A.1.x.1 — Install POSIX C-port standalone.**
      Added `install-platform-posix-c` recipe to `justfile`
      that builds `packages/core/nros-platform-posix-c` and
      drops `libnros_platform_posix.a` (32 KB, 3 objects, 73
      `nros_platform_*` T-symbols) into `build/install/lib/`
      plus a CMake config under
      `lib/cmake/NrosPlatformPosixC/`. Hooked into the
      top-level `install-local` target. Coexists with the
      still-bundled Rust-shim symbols inside
      `libnros_c_zenoh.a` (next sub-item swaps them).

    - [x] **123.A.1.x.2 — Drop Rust platform crate; rename
      C-port to `nros-platform-posix`.** Done. Deleted
      `packages/core/nros-platform-posix/` (Rust crate, 2398
      LOC across `lib.rs` + `net.rs` + `serial.rs`); renamed
      `packages/core/nros-platform-posix-c/` →
      `packages/core/nros-platform-posix/` so POSIX matches
      the FreeRTOS/NuttX/ThreadX/Zephyr C-port-only directory
      layout established in 121.3. Touched 30+ files:
        * `nros-platform`'s `platform-posix` feature now
          resolves to `dep:nros-platform-cffi` only — same as
          the four RTOSes.
        * `NET_SOCKET_SIZE` / `NET_ENDPOINT_SIZE` re-export
          replaced by inline `core::ffi::c_int` /
          `*mut c_void` consts in `nros-platform/src/resolve.rs`.
        * 3 host DDS tests migrated from `PosixPlatform` →
          `CffiPlatform`; `nros-rmw-dds[dev-dependencies]`
          swapped to `nros-platform-cffi[posix-c-port]`
          (build.rs compiles canonical C-port into the test
          binary). All 5 tests still pass.
        * Renamed CMake project + target export to drop the
          `-C` suffix (`NrosPlatformPosixTargets`,
          `lib/cmake/NrosPlatformPosix/`); added
          `NrosPlatformPosixConfig.cmake.in` with
          `find_dependency(Threads)` so the imported target
          resolves out-of-tree.
        * `NanoRos::NanoRos` now `find_dependency(NrosPlatformPosix)`
          + `target_link_libraries(... NrosPlatformPosix::nros_platform_posix)`
          on POSIX, same pattern as the existing XRCE wiring.
      Result on `build/install/lib/libnros_c_zenoh.a`:
      0 defined `nros_platform_*` T-symbols (was 73),
      ~32 KB smaller, undefined refs now resolved by linking
      the 32 KB standalone `libnros_platform_posix.a`.
      End-to-end smoke: `examples/native/c/zenoh/talker`
      builds + runs with 0 unresolved `nros_platform_*` in
      the final binary.

    - [~] **123.A.1.x.3 — Install C-ports for FreeRTOS / NuttX /
      ThreadX / Zephyr / ESP-IDF.** Partially done — FreeRTOS +
      ThreadX standalone install live. Remaining 3 platforms
      sketched but deferred (NuttX delegates to POSIX so
      `libnros_platform_nuttx.a` is redundant when the NuttX
      build system inlines POSIX source; Zephyr ships as a west
      module so `find_package` doesn't fit the parent build's
      discovery model; ESP-IDF registers as an IDF component
      `idf_component_register`, also not a `find_package`
      target).

      Done in this sub-step:
        * Dropped `-c` suffix from FreeRTOS / NuttX / ThreadX /
          Zephyr CMake project name, EXPORT name, install dir,
          and `_C_INSTALL` / `_C_WITH_NET` option names.
        * Added `Config.cmake.in` template for each of the four
          RTOSes so `find_package(NrosPlatform<X> CONFIG)`
          resolves the imported target out-of-tree.
        * Created `tools/install-platform/{freertos,threadx}/`
          install scaffolds — thin CMake projects that pull in
          the in-tree kernel headers, add the platform crate as
          subdirectory, scrub source-tree paths from the
          imported target, and run install. Yields
          `libnros_platform_freertos.a` (16 KB, 46 symbols) and
          `libnros_platform_threadx.a` (70 KB, 48 symbols)
          alongside `libnros_platform_posix.a` in
          `build/install/lib/`.
        * New `just freertos::install-platform` /
          `threadx_linux::install-platform` recipes,
          toolchain-gated (no-op when cross-toolchain missing).
        * Wired into top-level `install-local` ahead of the
          existing per-RTOS `install` recipes.
        * `net.c` deliberately omitted from the standalone
          installs (depends on lwIP / NetX Duo headers that the
          standalone scaffold can't pull in without copying half
          the application build). Apps that need it compile
          `net.c` into their binary via the board crate's
          build.rs — the existing path.
        * Updated `tests/{freertos,threadx,zephyr}-c-smoke/`
          CMakeLists + 3 just module comments to use the
          renamed paths / options.

      Deferred to a follow-up:
        * NuttX standalone `.a` install (low value — NuttX build
          system already inlines `nros-platform-posix/src/*.c`).
        * Zephyr standalone install (west module discovery; out
          of scope for the find_package pattern).
        * ESP-IDF standalone install (IDF component model;
          parent IDF build already pulls the source in via
          `idf_component_register`).

    - [~] **123.A.1.x.4 — Standalone RMW archives for zenoh
      and dds.** Partially done — wrapper staticlibs land
      alongside `libnros_c.a`. Final `nros-c` shrinkage
      deferred to A.1.x.4.b.

      A.1.x.4.a done in this sub-step:
        * New `nros-rmw-zenoh-staticlib` + `nros-rmw-dds-staticlib`
          wrapper crates (`crate-type = ["staticlib"]`) under
          `packages/zpico/` and `packages/dds/`. Each one
          re-exports the parent crate's public surface to keep
          the `#[unsafe(no_mangle)] nros_rmw_<x>_register()`
          symbol exported.
        * Cargo builds emit
          `libnros_rmw_zenoh_staticlib.a` (~27 MB, 463 objs,
          `nros_rmw_zenoh_register` defined `T`) and
          `libnros_rmw_dds_staticlib.a` (~25 MB, 341 objs,
          `nros_rmw_dds_register` defined `T`).
        * Tiny CMake scaffolds under
          `tools/install-rmw/{zenoh,dds}/` package the cargo
          output as `libnros_rmw_<x>.a` + emit a STATIC IMPORTED
          target via `find_package(NrosRmw<X> CONFIG)`.
        * `just install-rmw-zenoh` / `install-rmw-dds`
          recipes, hooked into top-level `install-local`.
        * Per-archive compiler_builtins duplication accepted;
          downstream link will need `--allow-multiple-definition`
          (GNU ld / lld) when both archives co-link with
          `libnros_c.a`.

      A.1.x.4.b done — final cutover landed:
        * `nros-c[cffi-zenoh-cffi]` + `cffi-dds-cffi` now
          resolve to `["rmw-cffi"]` only — same shape as
          `cffi-xrce-c`. Drops `nros/rmw-{zenoh,dds}-cffi`
          + `dep:nros-rmw-{zenoh,dds}` from `nros-c`'s
          Cargo tree.
        * `NanoRos::NanoRos`'s `INTERFACE_LINK_LIBRARIES` for
          `NANO_ROS_RMW={zenoh,dds}` now
          `find_dependency(NrosRmw<X>)` and inserts the
          standalone archive BETWEEN nros-c and
          NrosPlatformPosix (static-archive link order
          matters: `libnros_c.a` needs
          `nros_rmw_<x>_register` from
          `libnros_rmw_<x>.a`, which in turn needs
          `nros_platform_*` from `libnros_platform_posix.a`).
        * `-Wl,--allow-multiple-definition` added to
          `INTERFACE_LINK_OPTIONS` on Linux/macOS — reconciles
          per-archive copies of `compiler_builtins` +
          duplicated `nros-rmw-cffi` rlib content between
          `libnros_c.a` and the standalone RMW archive.
        * Audit (post-rebuild): `libnros_c_zenoh.a` 27.7 MB
          → 22.6 MB (-18%); `nros_rmw_zenoh_register` now
          UNDEFINED in the archive (resolved at final link
          from `libnros_rmw_zenoh.a`). Same shape for dds.
        * E2E smoke: `examples/native/c/zenoh/talker` +
          `c/dds/talker` build + run with 0 unresolved
          `nros_*` references in the final binary.

    - [x] **123.A.1.x.5 — Switch `nano_ros_link_platform` /
      `_link_rmw` to real link targets.** Done.
        * `nano_ros_link_platform(target [PLATFORM <p>])` now
          looks up `NrosPlatform<X>::nros_platform_<short>`
          via a `_nano_ros_platform_targets` lookup table
          (multiple input tags collapse to one CMake package,
          e.g. `freertos_armcm3` + `freertos_armcm4` →
          `NrosPlatformFreertos::nros_platform_freertos`),
          `find_dependency`s the package if needed, and
          `target_link_libraries(... PRIVATE ...)`. Mismatch
          guard dropped — nros-c is platform-agnostic at the
          symbol level (A.1.x.2), so per-target platform
          override is now safe.
        * `nano_ros_link_rmw(target [RMW <r>])` resolves to
          `NrosRmw<X>::NrosRmw<X>` similarly and appends
          `-Wl,--allow-multiple-definition` on Linux/macOS.
          Mismatch guard kept — nros-c's `nros_support_init`
          calls `extern "C" fn nros_rmw_<rmw>_register()` baked
          at Cargo feature-flag time, so a per-target RMW
          mismatch produces an unresolved-symbol link error
          today. (A.1.x.4.c follow-up could land runtime-
          pluggable register dispatch.)
        * E2E smoke: `/tmp/nano-ros-link-test/` consumer
          calls both helpers; properties
          `NANO_ROS_PLATFORM=posix` / `NANO_ROS_RMW=zenoh`
          land on the target; executable builds + links;
          mismatch guard fires correctly when `RMW=dds` is
          requested against a zenoh install.
- [x] **123.A.2 — `config/submodule-deps.toml`.** Authored.
  21 submodules classified across axes `required` (codegen) +
  `rmw.{zenoh,xrce,dds,cyclonedds}` + `platform.{posix,freertos,
  threadx,nuttx,zephyr,bare-metal,esp32}` + `reference.{px4,
  tracing}`. Dev-only paths (zenohd router, XRCE agent, ESP32
  QEMU fork) gated behind `--with-dev`; reference paths
  (PX4 1GB, Tonbandgeraet) opt-in via `--with-reference`.
  Cross-checked against `just/*.just` `git submodule update`
  invocations. Consumed by A.3 (`tools/setup.sh`) + A.8
  (per-platform `just <plat> setup` shims).
- [x] **123.A.3 — `tools/setup.sh` orchestration script.** Done.
  Single bash impl at `tools/setup.sh`. Reads
  `config/submodule-deps.toml` via minimal awk extractor, unions
  `required` + `platform.<plat>` + `rmw.<rmw>` (+ optional
  `dev_paths` under `--with-dev`, +
  `reference.<n>` under `--with-reference=<n>`), runs
  `git submodule update --init --depth=1 --recursive <path>`
  for each missing path. Installs rustup via the standard
  upstream installer if missing, adds the target's Rust triple
  via `rustup target add`, surfaces missing apt cross-toolchain
  packages on Linux (never auto-sudo). Supports `--dry-run`,
  `--doctor`, `--list-targets`. Verified on `posix-zenoh`,
  `freertos-xrce`, `threadx-dds` (dry-run); invalid target
  produces guided error.

  **Design pivot (locked 2026-05-14).** Original plan had a
  separate Rust `nros setup` CLI behind a `tools/setup.sh`
  bootstrap. Collapsed to one bash layer because:
    * Bootstrap "install nros to run nros setup" is a chicken
      -and-egg detour with no real value for our Linux/macOS/
      WSL2 audience.
    * Submodule fetch + rustup orchestration is bash-amenable
      — no complex parsing, no portability beyond what
      `tools/setup.sh` already needs.
    * The `nros` CLI keeps its actual value-add (codegen
      `generate-rust` / `generate-cpp`) — setup is pure
      orchestration and doesn't justify the Rust binary.
    * Two layers (`tools/setup.sh` + `just setup` shim) <
      three layers (bash + Rust CLI + just shim).

- [x] **123.A.4 — `just setup [target]` shim.** Done.
  `justfile`'s top-level `setup target=""` recipe now: if
  `target` non-empty (e.g. `just setup posix-zenoh`), `exec`s
  `tools/setup.sh --target=<target>`; otherwise runs the
  existing contributor-everything orchestrator. Same recipe
  name handles both flows.
- [ ] **123.A.5 — `cmake/bootstrap.cmake`.** CMake auto-runs
  `tools/setup.sh` when invoked without it first. Idempotent
  (no-op if submodules already populated). Mostly a usability
  win for users who jump straight to `cmake -B build` without
  reading the README.
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
- [x] **123.A.7 — Workspace-shared codegen cache.** Done (CMake
  side). `NanoRosGenerateInterfaces.cmake` honours
  `NANO_ROS_GEN_CACHE_DIR` (CMake var or env var):
    * Redirects `_output_dir` from
      `${CMAKE_CURRENT_BINARY_DIR}/nano_ros_{c,cpp}/<pkg>` to
      `${NANO_ROS_GEN_CACHE_DIR}/nano_ros_{c,cpp}/<pkg>`.
    * Umbrella include dir `${_umbrella_dir}` follows the same
      redirect so cross-package include resolution works
      (`<builtin_interfaces/msg/time.h>` from inside
      `std_msgs_msg_header.h`).
    * Per-target FFI crate dir
      (`nano_ros_cpp_ffi_<pkg>`) moves into the cache too.
    * `nano_ros_generate_<lang>_args__<pkg>.json` moves into the
      cache so the content-comparison mtime preservation works
      across packages.
    * args.json only rewritten when content changes — keeps
      mtime stable so `add_custom_command` sees outputs
      up-to-date.

  Verified: package B that consumes std_msgs after package A
  shows **0 `nros-codegen` invocations** during B's build —
  full cache hit. B's executable still links + runs.

  Cargo-side (`cargo-nano-ros generate-rust`) still emits to the
  per-package target dir — deferred. Today's cargo workflow
  already shares the `std_msgs` crate across workspace members
  via `[workspace.dependencies]`, so the cache pressure is
  CMake-side only.

  Concurrency caveat: colcon `--parallel-workers=N` can race two
  packages on the same codegen target. Mitigation: declare an
  explicit dependency between packages in package.xml so colcon
  serializes them. Documented in installation.md (A.9).
- [x] **123.A.8 — Migrate `just <plat> setup` recipes.** Done.
  Migrated to `tools/setup.sh --platform=<plat>` /
  `--rmw=<rmw>` shims:
    * `just freertos::setup` → `tools/setup.sh --platform=freertos`
    * `just nuttx::setup` → keeps the kernel-build step;
      submodule fetch delegated to `--platform=nuttx`
    * `just threadx_linux::setup` /
      `just threadx_riscv64::setup` → `--platform=threadx`
    * `just cyclonedds::setup` → keeps the SDK build; submodule
      fetch delegated to `--rmw=cyclonedds`
    * `just rmw_zenoh::setup` left inline (pulls only the
      `rmw_zenoh` dev fixture from
      `rmw.zenoh.dev_paths` — `--with-dev` would over-fetch).

  Added two `tools/setup.sh` modes to support the shims:
    * `--platform=<plat>` — fetch only `required` +
      `platform.<plat>` paths (no RMW).
    * `--rmw=<rmw>` — fetch only `required` + `rmw.<rmw>` paths
      (no platform).
    * `--skip-rustup` / `--skip-apt-check` for shims that don't
      want the toolchain side effects (cyclonedds already
      handles its own; rmw recipes don't need rustup).
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
./src/nano-ros/tools/setup.sh --target=posix-zenoh
# Rust users only:
./src/nano-ros/tools/setup.sh --target=posix-zenoh --rust-workspace
```

Step-by-step:

1. `tools/setup.sh` parses `--target=posix-zenoh` → `platform=posix`
   + `rmw=zenoh`. Reads `config/submodule-deps.toml`. Unions
   `required` (codegen) + `platform.posix` (none) + `rmw.zenoh`
   (`zenoh-pico` + `mbedtls`).
2. Runs `git submodule update --init --depth=1 <path>` for each.
3. Detects no rustup → installs via the standard installer.
   `rustup target add x86_64-unknown-linux-gnu` (host = POSIX
   no-op).
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

**Workspace `Cargo.toml`** (auto-generated by `tools/setup.sh
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
# workspace crate. Refreshed by `tools/setup.sh --refresh-cargo-patches`.
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
4. **A.3** — `tools/setup.sh` orchestration script (the
   load-bearing piece; pivoted from a Rust CLI design).
5. **A.4** — `just setup` shim over `tools/setup.sh`.
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
