# nano-ros

Lightweight ROS 2 client for embedded RTOS (Zephyr, FreeRTOS, NuttX, ThreadX). `no_std`.

## Naming
- **nano-ros** — project name (prose, docs)
- **nros** — code shorthand (crates, Rust/C idents, `CONFIG_NROS_*`)
- **nano_ros** — C header dir, CMake targets (`NanoRos::NanoRos`), CMake fn (`nros_generate_interfaces()`)

## Workspace
`packages/{core,zpico,xrce,dds,boards,drivers,interfaces,testing,verification,reference,codegen}/`, `examples/`, `third-party/` (gitignored SDKs), `zephyr/` module. Run `ls packages/` for current crate list.

`packages/drivers/` covers three sibling categories that settled during Phase 80: transport-bridge crates (`nros-smoltcp` — smoltcp ↔ zenoh-pico, formerly inside `zpico-sys`), peripheral / MAC drivers (`lan9118-smoltcp`, `openeth-smoltcp`, `cmsdk-uart`, `stm32f4-usart`, `virtio-net-netx`, `nvidia-ivc`, `nsos-netx`), and vendor `*-sys` bindings (`freertos-lwip-sys`, `threadx-netx-sys`, `zephyr-posix-sys`, `nuttx-sys`). Board crates pick the bridge + driver(s) they need; platform crates stay free of networking code.

## Build
- `just setup` / `just doctor` / `just check` / `just ci` (check + test-all) / `just verify` (Kani+Verus) / `just generate-bindings`
- `just <module> setup`: workspace, verification, qemu, freertos, nuttx, threadx_linux, threadx_riscv64, esp32, zephyr, xrce, zenohd, rmw_zenoh, orin_spe, cyclonedds, platformio, esp_idf, px4

**SDK tiers** (Phase 142, `just setup tier=<tier>`, defaults to `default` or `NROS_SETUP_TIER` env):
- `minimal` — workspace, verification, zenohd (Rust-only contributors)
- `default` — minimal + qemu, freertos, nuttx, threadx_{linux,riscv64}, esp32, zephyr, xrce, rmw_zenoh, orin_spe, cyclonedds, platformio (full `just ci` coverage)
- `extended` — default + esp_idf, px4 (every Phase 139 integration shell runnable)

Tiers are strict supersets; never move a module between tiers without bumping `docs/development/sdk-tiers.md` AND the orchestrator switch in `justfile::_orchestrate`. Policy: a module joins `default` iff ≤ 500 MB / ≤ 5 min wall-clock install AND exercised by `just test-all` AND idempotent. ARM FVP, NVIDIA SDK Manager, license-gated installs stay opt-in entirely (run `just <module> setup` out-of-band).

**Build tiers** (each strict superset): `build` (workspace + transports) ⊂ `build-examples` ⊂ `build-all` (= `build-examples` + `build-test-fixtures`). Per-platform tier: `just <plat> build` ⊂ `build-examples` ⊂ `build-fixtures` ⊂ `build-all`. Orchestration in `justfile` + `just/*.just`.

**Build parallelism** (Phase 165.perf + 176): one knob `NROS_BUILD_JOBS` (default nproc) scales every parallel recipe — `build-test-fixtures` runs a divided platform pool + zephyr solo at full budget; each recipe reads `${NROS_BUILD_JOBS:-N}`. Never re-introduce a hardcoded `parallel --jobs <n>` without threading the budget through. **Unified jobserver** (Phase 176, `just build-all-jobserver`): one GNU-make fifo jobserver spans cargo + build-script cc + ninja-via-west + cmake (dynamic token allocation, no static split) — needs the pinned `make ≥4.4` + `ninja ≥1.13` from `just workspace install-make` / `install-ninja` (apt's 4.3/1.10 lack the fifo jobserver; `.envrc` puts third-party/{make,ninja} on PATH, incl a `gmake`→make-4.4 alias). Recipes drop their explicit `-j`/`--parallel`/`CMAKE_BUILD_PARALLEL_LEVEL` under `NROS_JOBSERVER=1` so tools inherit the pool. **`just build-all` auto-routes to the jobserver path when the pinned make 4.4 + ninja 1.13 are present** (same artifacts), falling back to the static split without them; `NROS_NO_JOBSERVER=1` forces static. See `docs/roadmap/archived/phase-176-unified-jobserver-build-orchestration.md` + `phase-174-build-performance.md`.

**Test tiers** (each strict superset): `test-unit` (~5s) ⊂ `test-integration` (~30s) ⊂ `test` ⊂ `test-all` (+ heavy QEMU/Zephyr/ROS-interop + `test-doc` + `test-miri` + C codegen).

Per-platform: `just <plat> test|test-all|ci`. `<plat>` = target families (`qemu`, `zephyr`, board groups). Support services (`zenohd`, `cyclonedds`) are NOT platform scopes. Platform-specific build fail → rerun narrow `just <plat> build|build-examples|build-fixtures|build-all` before root `just build-all`. GNU `parallel` auto-used; `RUSTC_WRAPPER=sccache` auto-detected.

## Environment
`.env` (gitignored). **Run `direnv allow` once after clone** else `zpico-sys/build.rs` panics `"FREERTOS_PORT not set"`.

Runtime: `ROS_DOMAIN_ID` (0), `ZENOH_LOCATOR` (`tcp/127.0.0.1:7447`), `ZENOH_MODE`.

SDK paths auto from `third-party/<sdk>/`; override `<SDK>_DIR` env. See `docs/reference/environment-variables.md`.

## Practices
- **Always `just ci` after task.** **Never `sudo`** — tell user.
- **`just format` before broad changes** (Rust + C/C++ + Python).
- **Always use nightly for `rustfmt` / `cargo fmt`.** `rustfmt.toml` enables nightly-only options (`imports_granularity = "Crate"`, `format_code_in_doc_comments = true`); the stable toolchain warns and skips them, producing a different output than CI. Run `cargo +nightly fmt` (or `rustup run nightly cargo fmt`).
- **C/C++ style:** `.clang-format` LLVM-based, 4-space indent, 100-col limit.
- **Linear history:** `git pull --rebase` or `git fetch` + `git rebase`. Never merge commits unless user explicitly asks.
- **Submodule rebase on superproject pull:** if pull advances submodule pointer AND local work exists in submodule → enter submodule, fetch remote, rebase local work onto updated upstream, check out superproject's expected commit, record resulting submodule commit in parent. Never leave submodule at older local commit when remote pointer advanced.
- **Vendored-fork branch workflow (cyclonedds, netxduo, …):** each vendored fork is a NEWSLabNTU repo tracking a long-lived integration branch — for cyclonedds that's `nano-ros` on `github.com/NEWSLabNTU/cyclonedds`. Land fixes there with **linear history**: commit inside the submodule, `git fetch origin` + `git remote prune origin` (the fork has a dir/file ref clash — a stale `origin/nano-ros/zephyr-nsos-patches` blocks `origin/nano-ros`; prune clears it), `git rebase origin/<branch>` (never merge), then push the branch. **Order matters:** push the fork branch FIRST, *then* bump the superproject submodule pointer to the pushed commit — never record a pointer to an unpushed fork commit (breaks every clone's `submodule update`). **By default the agent does NOT push fork remotes** (they sit outside the trusted nano-ros repo → data-exfiltration guard): the agent commits + fetch/prune + rebase locally and leaves the branch ready, and the **maintainer runs the fork push** (`git -C third-party/dds/<fork> push origin <branch>`). **The agent MAY run the push itself only when the user has explicitly permitted it** via a scoped Bash allow-rule in `.claude/settings.*` (e.g. `Bash(git -C <submodule-path> push:*)`) — scope it to the specific submodule, never a blanket `git push:*`, and the rule must match the exact `git -C <path> push …` form the agent invokes. Either way the fork push lands FIRST, *then* the superproject pointer is bumped to the pushed commit. **colcon-nano-ros (`packages/codegen` submodule → `github.com/NEWSLabNTU/colcon-nano-ros`):** the orchestration CLI + colcon extension + codegen live here, NOT in the superproject — edits to `packages/codegen/**` (e.g. `nros-cli-core`, `colcon_nano_ros`) are submodule commits. Its integration branch is **`main`** (not `nano-ros`). Every commit must **eventually land on `main` with linear history, and `main` is kept up-to-date with `origin/main`**: `git -C packages/codegen fetch origin`, `git checkout main`, `git merge --ff-only origin/main` (refuse-and-stop if it can't fast-forward — a stale/diverged local `main` is the signal), then `git merge --ff-only <feature>` (or rebase your commit) so `main = origin/main + your work`, no merge commits. The agent commits + rebases locally and leaves `main` ready; the push (`git -C packages/codegen push origin main`) is run by the **maintainer** by default, or by the **agent** when the user has configured the scoped allow-rule `Bash(git -C packages/codegen push:*)`. The superproject pointer is bumped to the pushed `main` commit only after the push lands — never to an unpushed commit.
- **Don't modify vendored/generated:** `third-party/`, `packages/interfaces/*/generated/`, build output dirs — unless task explicitly requires regeneration. Preserve worktree changes.
- Unused vars: `_name` + comment, or `#[allow(dead_code)]` for test struct fields.
- Reusable tests → `packages/testing/nros-tests/tests/` (Rust) or `tests/` (sh). Temp tests → Bash, then promote.
- **Tests must fail on unmet preconditions.** `assert!()`/`bail!()` for missing env/binary. `nros_tests::skip!` panics with `[SKIPPED]` (OK). Bare `eprintln!`+`return` reports PASS — never. Same rule runtime: panic, not silent early-return. Exception: `rstest #[values]` matrix unsupported via `skip_reason()`.
- JUnit XML at `target/nextest/default/junit.xml`. Non-nextest → `tests/run-test.sh` → `test-logs/latest/`.
- Temp files in `$project/tmp/` (gitignored), not `/tmp`. Use Write/Edit, not heredoc. Repeated multi-step → `tmp/*.sh` script.
- `.gitignore`: every workspace-excluded crate has per-dir `.gitignore` with `/target/` (and `/generated/` if codegen). Native C++ examples need `/build/`. Always leading `/`. Add `--target-dir` paths.
- **Parallel build isolation:** nextest tests with different features building same example MUST use `--target-dir` (e.g. `target-safety/`, `target-zero-copy/`). See `fixtures/binaries.rs`.
- **Narrow platform build first.** Platform-specific failure → run `just <platform> build-all` (e.g. `just zephyr build-all`) before root `just build-all`. Closest variant (`just esp32 build`, `just qemu build`) if no `-all`.
- **Submodule rebase on pull.** Inspect submodules after pull/rebase. Remote pointer advanced + local submodule work → enter submodule, fetch, rebase local onto upstream, check out superproject's expected commit, record in parent commit. Never leave submodule at older local commit when remote moved.
- **No POSIX-style Rust constructor sections on Zephyr/native_sim.** `nros-cpp` ships weak `nros_app_register_backends` default; `nros_cpp_init` explicitly registers linked CFFI backend (Phase 127.C.4, `ffdde60f`). Don't assume `ctor`/linker-set registration runs on Zephyr — wire backend init explicitly.

### QEMU Networked Tests
- Slirp networking (no TAP/sudo/bridges).
- Per-platform zenohd ports in `nros_tests::platform`: baremetal=7450, freertos=7451, nuttx=7452, threadx-riscv=7453, esp32=7454, threadx-linux=7455, zephyr=7456. `ZenohRouter::start(platform::FREERTOS.zenohd_port)`.
- Bridge-net (threadx-linux veth): `ZenohRouter::start_on("0.0.0.0", port)`.
- Subscriber first, then publisher. 5–10s stabilization.
- Per-platform nextest groups (`max-threads = 1`); platforms parallel.
- **Domain ID: compile-time on embedded, runtime env on native (host).** Embedded targets (Zephyr, FreeRTOS, NuttX, ThreadX, bare-metal, ESP32) bake `ROS_DOMAIN_ID` at build time — Zephyr via Kconfig `CONFIG_NROS_DOMAIN_ID`, the others via each example's `config.toml` `domain_id` → generated `app_config.h` (read by `nros::init`/`.domain_id(config.domain_id)`). A runtime `ROS_DOMAIN_ID` env does NOT reach an embedded backend (e.g. native_sim libc `getenv` has no host trampoline; a cmdline arg is un-embedded). For **Cyclone** (RTPS ports = `7400 + 250*domain`), parallel test fixtures bake a *distinct* domain per communicating role-set — a talker+listener / server+client pair shares a domain, unrelated sets differ — so concurrent QEMU/native_sim processes don't collide on ports; see Phase 177.37 for the per-`(lang,variant)` baking pattern (`just <plat> build-fixtures` passes `-DCONFIG_NROS_DOMAIN_ID=` / per-fixture `config.toml`). **Native/host is the exception:** host Cyclone reads `ROS_DOMAIN_ID` from the env at *runtime* via `nros_tests::unique_ros_domain_id()` (collision-free per concurrent test using `NEXTEST_TEST_GLOBAL_SLOT`; Phase 177.33/177.35).
- **Patched `qemu-system-arm`** (Phase 143): `just qemu setup-qemu` builds `third-party/qemu/qemu` @ stable-11.0 + `third-party/qemu/patches/` into `build/qemu/bin/qemu-system-arm`. Test harness picks it up automatically via `nros_tests::qemu::qemu_system_arm_path()` — no env-var needed. System `qemu-system-arm` is the fallback. New test code MUST use `nros_tests::qemu::qemu_system_arm_cmd()` instead of `Command::new("qemu-system-arm")`. New justfile recipes MUST gate through `{{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }}`. See `book/src/internals/qemu-patched-binary.md`.

### Examples = Standalone Projects
**Each `examples/` dir is self-contained, copy-out template.**
- Canonical shape `examples/<plat>/<lang>/<example>/` (collapsed — RMW selected at build time via Cargo features for Rust + cmake `-DNROS_RMW=<rmw>` for C/C++ + Kconfig `prj-<rmw>.conf` overlay on Zephyr). Phase 118 + 168 collapsed every `<plat>/<lang>/<rmw>/<example>/` triple onto the single dir; legacy `<rmw>/<case>/` siblings deleted on Zephyr (Phase 168.6.C). Sibling categories: `examples/bridges/<name>/` (cross-RMW gateways), `examples/templates/<name>/` (multi-platform copy-out recipes — Pattern A workspaces etc.). Carve-outs: `examples/zephyr/cpp/cyclonedds/talker-aemv8r/` (one-board-one-RMW reference, not collapsed).
- **Non-example binaries live elsewhere.** Tests/benches/smokes are NOT in `examples/`:
  - `packages/testing/nros-bench/<name>/` — perf, fairness, stress, large-msg
  - `packages/testing/nros-smoke/<name>/` — driver/board bringup (no nros API)
  - `packages/testing/nros-tests/bins/<name>/` — fixture binaries built by integration tests
  Each is a standalone Cargo package with empty `[workspace]` table (they nest under the `nros-tests` member dir).
- Variant naming: suffix form (`talker-rtic`, `service-client-async`, `talker-rtic-mixed`) so variants sort with peers.
- No shared example-only helpers in `nros-cpp`/`nros-c` — boilerplate IS lesson.
- `*_DIR` env / `-D` injection = SDK-path contract. Example cmake accepts env or `-D` only — never project-tree heuristics.
- Per-example `Cargo.toml` + `.cargo/config.toml` + `CMakeLists.txt` build in isolation. No workspace reliance, no walk-up.
- **C/C++ consumption shape (Phase 137 / 140 / 144 / native examples):** `set(NANO_ROS_PLATFORM <plat>) + set(NANO_ROS_RMW <rmw>) + add_subdirectory(<repo-root> nano_ros) + target_link_libraries(<app> PRIVATE NanoRos::NanoRos) + nros_platform_link_app(<app>)`. Per-platform CMake glue lives at `cmake/platform/nano-ros-<plat>.cmake`; per-board overlays at `cmake/board/nano-ros-board-<board>.cmake`. **There is no `find_package(NanoRos)` path** — Phase 140 deleted it along with `just install-local`, the `build/install/` layout, every `install(...)` rule and every `Config.cmake.in` template. Cross-RTOS users (Zephyr, ESP-IDF, PlatformIO, NuttX, PX4) consume via the Phase 139 shells at `integrations/<rtos>/` — those shells re-export the same root CMake under each RTOS's native package manager.
- **Coverage matrix lives in `examples/README.md` ("Coverage matrix" + "Intentionally empty cells" sections) — authoritative for which `<plat>/<lang>/<rmw>` triples exist.** Deliberately empty cells: `{qemu-arm-baremetal, qemu-esp32-baremetal, esp32, stm32f4}/{c,cpp}/*` (no bare-metal C/C++ harness — `nros-c`/`nros-cpp` assume hosted RTOS for startup/heap/libc) and `px4/{c,rust}/*` (PX4 is uORB-only, and Phase 115.K.4 collapsed uORB to a C++-only port — `examples/px4/cpp/uorb/nros-register-check/` is the canonical surface). Do not add directories to these cells without first lifting the underlying constraint; Phase 118 lint blocks untriaged cells.

### CMake Path Convention
- Never hard-code project-relative paths in example cmake **or in
  `packages/<crate>/CMakeLists.txt`, `cmake/*.cmake` modules, build.rs,
  or any in-tree script**. Each subproject (`packages/dds/<name>`,
  `packages/core/<name>`, `examples/<dir>`) must build standalone — no
  walking up the source tree.
- No `../../../cmake/...`, no project-root heuristics, no
  `${_ROOT}/external/<sdk>` defaults, no `$<source_dir>/../../../scripts/...`
  in `install(...)` rules.
- **Drivers pass NANO_ROS_PLATFORM / NANO_ROS_RMW (+ board-specific
  cache vars) before `add_subdirectory(<repo-root>)`.** The
  `just`-recipe / outer build script knows the layout and supplies
  it via cmake `-D…=$PWD/...` or env var:
  - `-DNANO_ROS_PLATFORM=`, `-DNANO_ROS_RMW=`, `-DCMAKE_TOOLCHAIN_FILE=`,
    `-D<SDK>_DIR=`, `-D<BOARD>_CONFIG_DIR=`.
  - Third-party SDKs (Cyclone DDS, NetX Duo, FreeRTOS-Kernel) still
    pass `-DCMAKE_PREFIX_PATH=` to their own install prefixes — the
    deletion was about NanoRos's own install prefix, not third-party
    libraries the project depends on.
- **Find-program / find-package fallbacks may use install-relative
  paths.** Once installed, a CMake config at `<prefix>/lib/cmake/<Pkg>/`
  legitimately knows that companion files live at
  `<prefix>/share/<pkg>/` — `${CMAKE_CURRENT_LIST_DIR}/../../share/<pkg>`
  resolves inside the install layout, not the source tree, and is
  fine. The forbidden pattern is the source-tree variant
  (`${CMAKE_CURRENT_LIST_DIR}/../../../../scripts/<dir>`).

### Roadmap Docs
`docs/roadmap/`: header (Goal/Status/Priority/Depends on) → Overview → Architecture → Work Items + `### N.M — Title` + `**Files**` → Acceptance → Notes. `- [x]` done. Completed → `archived/`.

## Key Patterns
- **Zenoh pinned 1.7.2** (rmw_zenoh_cpp compat). zenohd from `third-party/zenoh/zenoh/`; zenoh-pico from `packages/zpico/zpico-sys/zenoh-pico/`. Tests auto-use `build/zenohd/zenohd`.
- **Rust edition 2024**: `unsafe extern "C" {}`, `#[unsafe(no_mangle)]`, explicit `unsafe {}` in `unsafe fn`. `nros-c` keeps `#![allow(unsafe_op_in_unsafe_fn)]` (420+ FFI ops).
- **API**: Rust mirrors rclrs 0.7.0; C mirrors rclc. `create_publisher/subscription/service/client/action_*`. `Publisher<M>`. Error `RclrsError`.
- **`no_std`**: all core crates `#![no_std]` + optional `std`/`alloc`.
- **Messages**: `nros generate-rust` from `package.xml`. **Never hand-write.** Example `generated/` gitignored. Only `packages/interfaces/rcl-interfaces/generated/` in git (`nros-` prefix). Bundled at `packages/codegen/interfaces/`. `nros-core` re-exports `heapless`.
- **C API** (`docs/reference/c-api-cmake.md`): **thin wrapper** delegates to `nros-node`, no logic re-impl. cbindgen 0.29 → `nros_generated.h`. `#[repr(C)]` fields `pub`. Hand-written: `visibility.h`, `platform.h`, `types.h`. Platform FFI uses `/// cbindgen:ignore`.
- **C++ API**: `nros-cpp` freestanding C++14 over typed extern "C" FFI to `nros-node`. Mirrors rclcpp. Error `nros::Result` + `NROS_TRY`. Codegen `nros generate cpp` or CMake `nano_ros_generate_interfaces(... LANGUAGE CPP)`. Std opt-in via `NROS_CPP_STD`. Zephyr `CONFIG_NROS_CPP_API=y`. Action/service C/C++ clients are executor-spin-driven (no blocking `zpico_get`) since Phase 77 (archived); the old FreeRTOS-QEMU hang is fixed (`LWIP_NETCONN_SEM_PER_THREAD=1`). Native **cyclonedds** C++ action `get_result` is fixed in `28e9e6502` + the follow-up Cyclone service/result path fixes (archived Phase 171.0.b): cpp+cpp now runs goal→accept→feedback→result e2e. Root cause was C++ action result/feedback framing plus Cyclone's dynamic-sequence service/result CDR bridge: `complete_goal_raw` stores result **fields** only, while C++ serializers/deserializers expose normal CDR buffers with the 4-byte header. `nros_cpp_action_server_complete_goal` strips the CDR header before storing, `cpp_result_trampoline` and the feedback trampoline/stream path re-add headers before C++ delivery, and the Cyclone service bridge no longer injects a second nested CDR header for `GetResult_Response`. The Cyclone publisher path manually bridges Fibonacci feedback and `GoalStatusArray_` dynamic sequences and waits for a matched DDS reader before volatile feedback/status writes. Verified 2026-05-21: fresh native cpp+cpp CycloneDDS action pair passes with feedback payload + result; native C+C still passes; `cargo test -p nros-cpp` passes. Remaining action work moved to Phase 177.2: Zephyr cyclonedds actions plus cross-impl validation, not native cpp+cpp get_result/feedback.
- **cbindgen output as canonical FFI** (Phase 118.D): nros-cpp `*.hpp` headers `#include "nros_cpp_ffi.h"` directly; per-file hand-written `extern "C"` redeclaration blocks have been removed (drift broke Phase 112 once). `qos.hpp` keeps a fallback redef under `#ifndef NROS_CPP_FFI_H` for callers that don't pull the cbindgen header. Three exceptions: `parameter.hpp` cross-references nros-c's `<nros/parameter.h>`; `action_{client,server}.hpp` `reinterpret_cast` `goal_id` at FFI callsites because cbindgen renders `*const [u8; 16]` as ptr-to-array. `set_callbacks` excluded from cbindgen via `[export.exclude]` (Rust `Option<extern "C" fn>` becomes opaque `Option_*` struct C++ can't construct); declared locally with plain fn-ptr typedefs. cbindgen variants prefixed with enum name (`prefix_with_name = true`) to avoid C++ name collisions.
- **Probe-only opaque sizes** (Phase 118.B / 87.6): `EXECUTOR_OPAQUE_U64S` etc. derived from `nros::sizes::EXECUTOR_SIZE` via `nros_sizes_build` rlib probe — no hand-math upper bound. Per-consumer `const _: () = assert!(size_of::<Ty>() <= STORAGE_SIZE …)` enforces compile-time correctness. Probe=0 only on `cargo check --no-default-features` (warns + 1-word placeholder; resulting rlib must not be linked). `CppContext` adds explicit `CPP_CONTEXT_OVERHEAD = 8` (u32 domain_id + alignment padding) on top of `Executor`.
- **Wrapper timing** (Phase 118.C.b): `Future::wait()`, `Stream::wait_next()`, `Executor::spin(duration_ms)` budget by wall-clock via `nros_cpp_time_ns()`. Iteration-count loops collapse on early-wake from signaled condvars (keep-alives, discovery gossip).

### Platform Backends
Three orthogonal axes (compile-time mutual excl, zero on axis OK):
- **RMW**: `rmw-zenoh|rmw-xrce|rmw-cyclonedds` (`rmw-dds`/dust-dds retired Phase 169)
- **Platform**: `platform-{posix,zephyr,bare-metal,freertos,nuttx,threadx}` — `[platform.bare-metal]` in `zenoh_platforms.toml` carries `arch = ["cortex-m3", "riscv32imc"]`; build.rs's first-match dispatch picks the right arch per target triple (so `qemu-arm-baremetal` + `ESP32-C3` share the same platform entry). Phase 148.
- **ROS edition**: `ros-{humble,iron}`

RMW backend host-language policy (frozen 2026-05-07; dust-dds row retired 2026-05-19 per Phase 169): see `book/src/internals/rmw-backends.md`. Rule: backend's host language matches its underlying library's native language unless overridden. Today: cyclonedds=C++, XRCE=Rust→C (115.K.2), zenoh-pico=Rust (deferred), uORB=Rust (won't-do). dust-dds=Rust **retired** (Phase 169 — repeated bring-up failures on every embedded target; consolidating DDS on Cyclone).

Default `std`. Cross-cutting: `unstable-zenoh-api` (zero-copy receive).

### Spin/Yield
`zpico_spin_once` event-driven wake:
- POSIX/Zephyr: `_z_condvar_wait_until` on `g_spin_cv`
- FreeRTOS: `xSemaphoreTake(g_spin_sem, …)`
- NuttX: `sem_timedwait(&g_spin_sem_posix, …)` (pthread condvar hangs — Phase 55.12)
- Bare-metal: single-thread `zp_read` loop

Cooperative yield (`PlatformYield`): POSIX/NuttX `sched_yield()`, Zephyr `k_yield()`, FreeRTOS `vPortYield()`, ThreadX `tx_thread_relinquish()`, bare-metal `core::hint::spin_loop()` default, opt-in `cortex_m::asm::wfi()` via `BoardIdle`. RTOS yields not ISR-safe; `spin_loop()` is.

### smoltcp Multicast (bare-metal)
- `Interface::join_multicast_group(addr)` needs multicast addr; smoltcp 0.12 returns `Unaddressable` for `0.0.0.0`. Pass GROUP (`239.255.0.1`).
- `set_recv_timeout(_, 0)` in `define_smoltcp_platform!` = non-blocking poll.
- LAN9118 emulator filter rejects multicast unless `MAC_CR.MCPAS`; promiscuous (`PRMS`) recommended for QEMU `-nic socket,…`.
- `MAX_UDP_SOCKETS` default 4. RTPS needs 3/participant; zenoh/xrce 0..=1.

### NetX Duo BSD (ThreadX)
- `SO_RCVTIMEO` takes `struct nx_bsd_timeval *`, NOT `INT` ms. Wrong type → `wait_option = NX_WAIT_FOREVER` → deadlock. Use `nros-platform-threadx::set_recv_timeout_ms`.
- `fcntl(F_SETFL, O_NONBLOCK)` works (toggles `NX_BSD_SOCKET_ENABLE_OPTION_NON_BLOCKING`).
- NSOS-NetX shim translates `SO_RCVTIMEO` for threadx-linux. Accepts INT-ms and `nx_bsd_timeval`.

### Board Transport Features
`ethernet` (default MPS2-AN385/STM32F4/ESP32-QEMU) or `wifi` (ESP32) → TCP/UDP via `zpico-smoltcp`; `serial` → UART via `zpico-serial` (bare-metal) or zenoh-pico built-in (ESP32, Zephyr). `Config` fields `#[cfg(feature)]`-gated. ≥1 transport required (`compile_error!`). Coexist OK (locator selects). ESP32/ESP32-QEMU use zenoh-pico's serial (no `zpico-serial`).

### Parameter Services
`param-services` feature in `nros-node` → `~/get_parameters`, `~/set_parameters`, etc. Uses `nros-rcl-interfaces`. Handlers return `Box<Response>`.

### XRCE Embedded Build (Phase 115.K.2 + 118)
`nros-rmw-xrce-cffi` (C FFI shim) gates `UCLIENT_PROFILE_{UDP,TCP,SERIAL}` + `UCLIENT_PLATFORM_POSIX` + `transport_posix_{udp,serial}.c` source files on `target_os = linux|macos|*bsd`. Bare-metal targets (`target_os = "none"`) get only `UCLIENT_PROFILE_{DISCOVERY,CUSTOM_TRANSPORT,STREAM_FRAMING}` and must inject their own custom transport. `just check-workspace-embedded` excludes `nros-rmw-xrce{,-cffi,-cffi-staticlib}` (header-only K.2 backend's `internal.h` references UDP types unconditionally — upstream design issue; the staticlib sibling needs panic_handler resolution at compile time which only works on hosted targets). Phase 160.L added the `-staticlib` sibling so Corrosion can import a real `staticlib` target without forcing the cffi rlib crate to emit one.

### Verification
- Kani: 160 bounded harnesses. `just verify-kani` (~3 min)
- Verus: 102 unbounded proofs. `just verify-verus` (~1 sec)
- Verus: `external_type_specification` w/o `external_body` = transparent enum; with = opaque. Never `verify = true` on production crates with fn pointers/closures. See `docs/guides/verus-verification.md`.

### ROS 2 Interop
rmw_zenoh-compatible. Key `<domain>/<topic>/<type>/TypeHashNotSupported`. See `docs/reference/rmw_zenoh_interop.md`.

## Phases
Active in `docs/roadmap/`, completed in `docs/roadmap/archived/`. Run `ls docs/roadmap/` for status.

Phase 117 (Cyclone DDS RMW + Autoware safety-island): Cyclone DDS submodule pinned tag `0.10.5` at `third-party/dds/cyclonedds/` (matches `ros-humble-cyclonedds` 0.10.5). `nros-rmw-cyclonedds` standalone CMake project at `packages/dds/nros-rmw-cyclonedds/` (NOT a Cargo crate); registers C++ vtable via `nros_rmw_cffi_register`. **Goal: full wire-compat with stock `rmw_cyclonedds_cpp`**. `NANO_ROS_RMW=cyclonedds` CMake option auto-pulls Cyclone backend + flips on `NROS_RMW_CYCLONEDDS=1` macro that triggers register call inside `nros::init`. Driver: `just cyclonedds {setup,build,build-rmw,test,doctor,clean}`. Pub/sub + services + raw-CDR data plane wired (117.1–117.9 done). Stock-RMW interop pending (117.X.1 rosidl_adapter codegen → 117.X.2 topic prefix conventions `rt/`/`rq/`/`rr/` → 117.X.3 replace `ServiceEnvelope` with upstream `cdds_request_header_t` → 117.X.4 type-name mangling verification → 117.X.5 service QoS alignment → 117.12 POSIX E2E vs stock RMW). Interim 117.7.B `ServiceEnvelope` works for nano-ros↔nano-ros only; 117.X.3 supersedes. **Cyclone service QoS = RELIABLE + VOLATILE** (`src/qos.cpp`): a request written before the client writer matches the server's request reader is silently dropped (VOLATILE ⇒ no delivery to a reader matched after the write). Archived Phase 171.0.a patched `service.cpp` to gate the first write on `dds_get_publication_matched_status(writer).current_count > 0` — inline wait in `call_raw`, buffer-and-flush-on-match in `send_request_raw`/`try_recv_reply_raw`. Local Cyclone service roundtrip passes; stock ROS 2 interop still fails in `just cyclonedds test` and remains Phase 117 / Phase 177 work.

Phase 124 (RMW zero-copy + dispatch + ABI extensions): six `nros_rmw_vtable_t` slot additions, each with a matching Rust trait method + C/C++ wrapper + routing test (cross-language discipline from Phase 122). Threads: **A** zero-copy (`pub_loan`/`pub_commit`/`pub_discard`/`sub_borrow`/`sub_release` + arena fallback when NULL); **B** wake-callback + condvar (`set_wake_callback` supersedes `set_wake_signal` — deleted, no alias; `Executor.wake_cv` condvar-blocked spin; guard-condition C/C++ surface; ISR-safe `nros_platform_condvar_signal_from_isr`); **C** `service_server_available`; **D** `try_recv_sequence` (burst-take, runtime emits `try_recv_raw` loop fallback when NULL); **E** `publish_streamed` (size_cb + chunk_cb, staging-buffer fallback when NULL); **F** `ping_session`. NULL-slot policy: every slot has a runtime fallback or surfaces `RET_UNSUPPORTED` — no backend obligation creep. Backend native impls: zenoh-pico has E.3 (`z_bytes_writer` streamed publish) + F.2 (`zp_send_keep_alive` ping) + A.4 (native loan). Deferred: D.3 zenoh ring-buffer batch (upstream `rmw_zenoh` loops too — single-slot `SubscriberBuffer` would need a ring rewrite), Cyclone/dust-dds native take, XRCE E.3/F.2 (header-only K.2 backend needs a Rust-side `XrceRmw` adapter). Phase keeps wait semantics in the platform layer — no upstream `rmw_wait`/waitset, no RTOS stubs.

Phase 128 + 129 (RMW selection cleanup + platform-agnostic backends, archived 2026-05-17): RMW pick driven by Cargo manifest + CMake `target_link_libraries`, no code-level selection. `NROS_RMW` env-var fallback for runtime multi-RMW pick; bridge mode via `Executor::open_multi(&[SessionSpec])` + `create_node_on(name, rmw)` + `nros-bridge` crate (TOML loader behind `config` feature; C/C++ surface in `<nros/bridge.h>`/`<nros/bridge.hpp>`). Per-backend cmake interface libs `NanoRos::Rmw::<name>`. Backend linker-section discovery via `linkme` (with fallback `nros_rmw_register_backend!` macro for unsupported targets like NuttX). Phase 129 retired `zpico-platform-shim` + `xrce-platform-shim`: every `z_*`/`_z_*`/`uxr_*` symbol now comes from C alias TUs (`zpico-sys/c/zpico/platform_aliases.c`, `nros-rmw-xrce/src/platform_aliases.c`) that forward to canonical `nros_platform_*` ABI. IVC link-layer carved into standalone `zpico-link-ivc` crate. Generic platform header (`nros_zenoh_generic_platform.h`) types `z_clock_t = uint64_t`, opaque storage for `_z_task_t`/`_z_mutex_t`/`_z_condvar_t`. `nros-rmw-xrce-cffi`'s `transport_nros_udp.c` superseded per-platform `transport_posix_udp.c`/`transport_zephyr_udp.c`. `link-tcp`/`link-udp-unicast` cargo features deleted (vendor always compiles those transports; locator picks at runtime). `NET_*_SIZE` / `NET_*_ALIGN` exported unconditionally from `nros-platform`. **Active follow-ups:** phase 131 (examples tree revision) + phase 133/134/135 (CI sweep findings from first clean run on `main`).

Phase 131 (examples tree revision, superseded by Phase 118 collapse): canonical example shape is now `examples/<plat>/<lang>/<example>/`, with RMW selected at build time. Sibling categories remain `examples/bridges/<name>/` (cross-RMW gateways) + `examples/templates/<name>/` (multi-platform copy-out recipes — Pattern A workspaces). Tests/benches/smokes moved OUT of `examples/` into `packages/testing/{nros-tests/bins,nros-bench,nros-smoke}/`. Variant naming uses suffix form (`talker-rtic`, `service-client-async`, `talker-rtic-mixed`). See `docs/roadmap/phase-131-examples-tree-revision.md` for migration history; older nested `async-*`/`rtic-*` parent dirs were renamed in 131.D.

Phase 175 (Cyclone DDS build path for pure-Rust examples): a plain `cargo build --features rmw-cyclonedds` can't link — `nros_rmw_cyclonedds_register` lives only in the C++/CMake backend. **175.A landed 2026-05-21** — `examples/native/rust/{talker,listener}/CMakeLists.txt` are the cyclonedds build path: `find_package(CycloneDDS)` + `add_subdirectory(packages/dds/nros-rmw-cyclonedds)` + `nros_rmw_cyclonedds_generate_from_msg(std_msgs msg/Int32.msg)` (host `idlc` at `build/cyclonedds/bin/idlc`, descriptor whole-archived so its static-init register TU survives) + `corrosion_import_crate(NO_DEFAULT_FEATURES FEATURES rmw-cyclonedds)`; ddsc linked via `corrosion_add_target_local_rustflags` `$<TARGET_FILE:CycloneDDS::ddsc>` (corrosion mangles the namespaced imported SHARED target's `-l`). zenoh/xrce variants still build with plain `cargo build` — the CMakeLists is cyclonedds-only. **Native Cyclone Rust talker/listener now exchange user data** (2026-05-21): Cyclone is a poll-only backend (`set_wake_callback = NULL`), so `session_drive_io(timeout_ms)` must pace the executor; hosted POSIX now sleeps for `timeout_ms` with `std::this_thread::sleep_for` (Zephyr keeps `k_msleep`). Verified build→link→boot→publish→receive (`Published: 0/1`, `Received: 0/1`) on loopback. `rmw-cyclonedds` still stays out of pure-cargo fixture loops; use the CMake/Corrosion path. 175.B (embedded ddsrt RTOS port) deferred (research-grade).

**Embedded Cyclone runtime (FreeRTOS + ThreadX, Phase 177.22, `db0e4fbb5`):** transient publish samples MUST use Cyclone's `ddsrt_{malloc,calloc,free}` (`<dds/ddsrt/heap.h>`), never libc `std::malloc/free` — `dds_stream_free_sample` + `dds_alloc` buffers free through the ddsrt heap, and on RTOS that heap is separate from libc (mixing = corruption). FreeRTOS + ThreadX share the embedded `kEmbeddedCycloneConfig` (`session.cpp`); ThreadX adds `<AllowMulticast>false</AllowMulticast>` (NetX BSD multicast limits — peer interop is a separate follow-up) and disables the optional `opt_size_xcdr1/2` CDR fast-path precompute (its ops-walker trapped). **Caveat (Phase 177.23.A):** the opt_size disable is gated on Cyclone-internal `DDSRT_WITH_THREADX` (from generated `dds/config.h`), not the target-`PRIVATE` `NROS_PLATFORM_THREADX` the rest of the file uses — fails open if `config.h` leaves the include chain. C talker registers its descriptor explicitly (no constructor reliance, same as Zephyr Phase 127.C.4).

## Quick Reference
`book/src/reference/build-commands.md`: manual testing, ROS 2 interop, Docker, QEMU, Zephyr. Build book: `just book`.

Docs: `book/src/` (user, mdbook) — getting-started, user-guide, porting, reference, concepts, internals. `docs/` (contributor) — reference, design, research, roadmap.
