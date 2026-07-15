# Repository Guidelines

## Project Structure & Module Organization

nano-ros is a Rust workspace for a `no_std` ROS 2 client stack with C/C++ integration. Core crates live under `packages/core/`; RMW backends under `packages/zpico/`, `packages/xrce/`, and `packages/dds/`; board/platform support under `packages/boards/` and `packages/platforms/`; drivers under `packages/drivers/`; and reusable integration tests under `packages/testing/nros-tests/`. Shell and smoke fixtures live in `tests/`. Examples are standalone copy-out projects under `examples/`, with the canonical shape `examples/<platform>/<language>/<example>/`; the RMW is selected at build time.

**Supported hosts: Linux (primary) and *BSD (POSIX path). macOS is NOT supported** (dropped 2026-06-18, phase-260): no macOS CI runner means macOS-specific link/section paths ship un-run, so the project does not carry them. Do not add `APPLE`/`target_os = "macos"`/`*-apple-darwin` branches to nano-ros source, CMake, or CI; embedded RTOS targets + the Linux host are the supported surface.

Reference and contributor docs live in `docs/`; user-facing mdBook docs live in `book/src/`; build orchestration lives in `justfile` and `just/*.just`.

## Design Documents (RFCs)

`docs/design/` is the design source of truth: numbered, living RFCs (`NNNN-slug.md`) with a
`status` of `Draft` / `Stable` / `Superseded`. [docs/design/ARCHITECTURE.md](docs/design/ARCHITECTURE.md)
is the finalized whole-system view; [docs/design/README.md](docs/design/README.md) is the index.
New RFC: copy `docs/design/0000-template.md`.

Two rules:

- **Design rationale goes in an RFC, never only in a phase doc.** Phase docs in `docs/roadmap/`
  are work breakdowns; they name the RFC they implement in an `Implements: RFC-NNNN` header.
- **Drift rule:** flipping an RFC to `status: Stable` requires updating the matching section of
  `ARCHITECTURE.md` in the same commit.

## Build, Test, and Development Commands

- `just --list`: show public recipes.
- `scripts/bootstrap.sh`: first-time entrypoint; installs/checks `just`, then prints setup choices.
- `scripts/bootstrap.sh base`: first-time native/ROS/zenoh quick-start setup.
- `scripts/bootstrap.sh all`: contributor/full-matrix setup; pulls and installs every supported SDK tier.
- `scripts/bootstrap.sh platform <platform>`: first-time focused setup for one platform.
- `just setup`: print setup choices; does not fetch/install.
- `just setup base`: install the base quick-start SDK/tooling tier.
- `just setup all` or `just setup tier=all`: install the full contributor/test-all tier.
- `just <platform> setup`: install a focused platform SDK/tooling tier.
- `just doctor` and `just doctor tier=all`: diagnose base or full setup readiness.
- `just build`: build the workspace plus generated bindings and transport artifacts.
- `just build-examples`: build the workspace and example matrix.
- `just build-test-fixtures`: prebuild binaries required by the full test matrix.
- `just build-all`: run the broad build tier; it auto-routes through the GNU make jobserver path when the pinned make/ninja tools are available.
- `just <platform> build`, `just <platform> build-examples`, `just <platform> build-fixtures`, `just <platform> build-all`: run platform-scoped tiers first when a platform-specific failure appears.
- `just test-unit`: fast workspace unit tests.
- `just test`: standard dev tier; skips heavy platform/ROS 2 groups.
- `just test-all`: full matrix, doctests, Miri, and C codegen tests. Run `just build-test-fixtures` first.
- `just check`: formatting and clippy checks across Rust, C, C++, and Python surfaces.
- `just ci`: `check` plus `test-all`.

Treat `<platform>` as target families such as `qemu`, `zephyr`, `freertos`, `nuttx`, `threadx_linux`, `threadx_riscv64`, `esp32`, or board groups. Support services such as `zenohd`, `cyclonedds`, and `xrce` are not platform scopes.

Codex sandbox notes:

- `just` may fail before running a recipe if the default runtime
  directory is read-only, with an error about creating a temporary
  directory under `/run/user/.../just`. In that environment, rerun with
  `XDG_RUNTIME_DIR=/tmp`.
- Cargo commands inside `just` may need to update the user's registry
  cache under `$HOME/.cargo`. If a recipe fails with read-only
  filesystem errors in `.cargo/registry`, rerun the same command with
  sandbox escalation rather than treating it as a project failure.
- A failed pre-nextest Cargo setup can leave an old
  `target/nextest/default/junit.xml` in place. If a recipe prints
  slow-test output after such a setup failure, verify whether nextest
  actually ran before trusting the timing report.

## Coding Style & Naming Conventions

Rust uses edition 2024 and `rustfmt.toml` with nightly-only formatting options. Use `cargo +nightly fmt` or `rustup run nightly cargo fmt`; stable rustfmt produces different output. C and C++ follow `.clang-format` based on LLVM, 4-space indentation, and a 100-column limit. **clang-format output drifts across major versions** (e.g. v17 vs v22 reformat `reinterpret_cast<T(*)[N]>` differently → spurious `just format`/`check-{c,cpp}-fmt` diffs), so the version is **pinned in `.clang-format-version`** and provisioned by `just setup-clang-format` as a PROJECT-LOCAL binary at `build/clang-format/bin/clang-format` (the standalone binary extracted from the exact-version PyPI `clang-format` wheel — no venv, no `pip install`, nothing user-wide; like `build/zenohd`/`build/qemu`). Run as part of `just setup`. The `format-*`/`check-*-fmt` recipes resolve that pinned binary via `scripts/dev/clang-format.sh` (`nros_clang_format`), falling back to a PATH `clang-format` with a loud version-skew warning. `just doctor` reports the pin status. Keep crate names and package paths in the existing `nros-*`, `zpico-*`, backend-specific, and platform-specific patterns.

Project naming:

- `nano-ros`: prose and docs.
- `nros`: crates, Rust/C identifiers, and `CONFIG_NROS_*`.
- `nano_ros`: C header dir, CMake targets, and CMake helpers such as `nros_generate_interfaces()`.

## Testing Guidelines

Prefer the narrowest tier that covers the change. Reusable Rust integration tests belong in `packages/testing/nros-tests/tests/`; shell tests belong in `tests/`; temporary tests can start as Bash and should be promoted when reused. Tests must fail on unmet preconditions with `assert!`, `bail!`, or the project skip helper; do not silently `eprintln!` and return from a test.

**No compilation inside tests.** A test must not invoke `cargo build`, `cmake --build`, `idf.py build`, `west build`, `nros generate` + compile, or any other compiler/build at run time. Compilation belongs in the **build stage** — `just build-test-fixtures` and the per-platform `build-fixtures` lanes (driven by `examples/fixtures.toml`). A test consumes a **prebuilt fixture artifact** and exercises its behavior. Reasons: in-test builds make the test wall-clock dominated by compile time (so they blow the per-test timeout under any load and report as spurious `timed out` failures), serialize on the cargo/cmake build locks, and conflate "does it build" with "does it behave". If a test's *intent* is to verify that something compiles (a macro form, a codegen output, an API shape), make it a **fixture in the build step** — add a row to `examples/fixtures.toml` (or a build-lane target) so the artifact is built once during `build-test-fixtures`, and have the test assert the fixture exists / inspect the built artifact, the same way the native C/XRCE tests consume their prebuilt CMake fixtures. The build either succeeds (fixture present → test checks it) or fails loudly in the build stage where it belongs.

**Test names describe behavior, not phase numbers.** Name a test for what it verifies, e.g. `zephyr_xrce_service_request_reply_e2e`, `rust_talker_to_cpp_listener_delivers`, `main_macro_accepts_no_arg_form`. Do **not** encode roadmap phase numbers in test names or test-file names (`phase212_n9_main_macro_forms`, `phase217_c_fvp_runtime`); phases are planning artifacts that go stale, and a phase-numbered name tells a future reader nothing about what broke. Cross-reference a phase in a doc-comment if useful, not in the identifier.

**Fixture prerequisites are provisioned by `nros setup`.** The build-stage test fixtures (`build-test-fixtures` + `scripts/build/compile-check-fixtures.sh`) need the cross toolchains, `play_launch_parser`, `corrosion`, `cmake`, `zenohd`, etc. These are installed by `nros setup` / `just setup all` (RFC-0014), NOT built ad-hoc. Before a fixture build or a `test-all` run, ensure they are present with `just doctor tier=all` (it lists every tier's prereqs as `[OK]` / `[MISSING]`); run `nros setup <board>` / `just setup all` to fill gaps. A fixture that can't build because a toolchain is absent is an environment gap to fix via setup — not a per-test workaround. (If you find a prereq a fixture needs that `nros setup` does not provision, add it to the SDK index / setup flow rather than hand-installing it.)

For platform failures, rerun the closest platform recipe first, for example `just zephyr build-all`, `just freertos build-fixtures`, or `just qemu build`, before spending time on root `just build-all`.

Native C/XRCE tests are runtime-only and consume prebuilt CMake fixtures under `examples/native/c/{talker,listener}/build-xrce/`. If `c_xrce_api` fails with `Test fixture binary not prebuilt`, run `just native build-fixtures` or `just build-test-fixtures` before debugging runtime behavior. A focused verification of phase 177.9.C passed with `just native test-c-xrce verbose` after fixture prebuild.

## SDK Environment Defaults

Keep repo-local SDK defaults centralized in `just/sdk-env.just`. This includes paths such as `FREERTOS_DIR`, `NUTTX_DIR`, `THREADX_DIR`, `PX4_AUTOPILOT_DIR`, `NROS_ESP_IDF_WORKSPACE`, `NROS_ESP_IDF_ENV_SHIM`, and `IDF_PATH`. Local overrides belong in `.env` or the caller environment.

Do not duplicate those defaults in package code, tests, examples, CMake, or scripts. Packages and examples must remain position-independent: they may read explicit environment variables and should skip or fail with a clear setup hint when a required SDK variable is absent, but they must not assume the checkout lives at a particular repo-relative path.

Shells that need the same defaults should source `scripts/sdk-env.sh`, which evaluates `just/sdk-env.just` and exports only missing variables. `.envrc`, `setup.bash`, and `setup.fish` all use that adapter. When a direct `cargo test` or `cargo nextest` run needs these defaults, either source `scripts/sdk-env.sh` first or run it through a `just` recipe so `just/sdk-env.just` is imported and exported to the child process. Prefer adding a focused `just` test helper over adding repo-path fallbacks inside `packages/`.

## Toolchain & SDK Provisioning

Design rationale → RFC-0014 (`docs/design/0014-nros-setup-toolchain-management.md`). Operational
contract:

Host toolchains/tools (`qemu`, cross-GCC, `zenohd`, `openocd`) are provisioned by `nros setup`,
not built ad-hoc. `nros-sdk-index.toml` (repo root) is the SSOT: each `[tool.*]` has a per-host
sha256-pinned prebuilt `dist` **and** a `[tool.*.source]` recipe; `[source.*]` build with the app;
`[gated.*]` (NVIDIA SPE, ARM FVP) are never fetched, only instructed. Prebuilt assets live on the
separate `NEWSLabNTU/nano-ros-sdk` repo's Releases (referenced by URL, not a submodule).

- `nros setup <board>` / `nros setup --tool <name> [--prefix <dir>]` installs to
  `$NROS_HOME/sdk/<tool>/<version>/` (`.nros-provenance` + `nros-sdk.lock` record it).
- **`nros` is the single SDK resolver:** platform builds lazy-install a board's index tools on
  first use (`setup::ensure_tools`; opt out via `NROS_NO_AUTO_SETUP`) and prepend the locked store
  `bin/` to the child PATH. (The former `nros build`/`deploy` verbs were retired in Phase 222 —
  `nros doctor` lints for leftovers.) **Non-`nros` scripts, the test harness, and
  CMake do NOT resolve SDK paths — they assume the SDK is given and only check + warn**
  (`nros doctor` / `just <plat> doctor`). Do not re-add store-path probing to test code.
- `just qemu setup-qemu` / `just zenohd setup` are **thin `nros setup --tool` callers** (install
  into `build/<tool>` where the harness looks). Do not reintroduce the in-tree configure/make or
  the `third-party/qemu` submodule build. `just <tool> build` still source-builds for devs.
- ESP32 = **ESP32-C3 (RISC-V)** (`riscv32imc-unknown-none-elf` via rustup + build-std, espflash,
  Espressif `qemu-system-riscv32` fork). Needs no index host-tool.
- CI gate: the `sdk-index` job in `.github/workflows/pr-checks.yml` sha256-verifies any index `dist` change. Bump a
  tool's `-nros<N>` suffix when rebuilding the same upstream version with different config.

## C/C++ Integration Shape

C and C++ consumers use source-tree CMake integration, not an installed package. The expected pattern is:

```cmake
set(NANO_ROS_PLATFORM <platform>)
set(NANO_ROS_RMW <zenoh|xrce|cyclonedds>)
add_subdirectory(<repo-root> nano_ros)
target_link_libraries(<app> PRIVATE NanoRos::NanoRos)
nros_platform_link_app(<app>)
```

Use `NanoRos::NanoRosCpp` for the C++ API where needed. There is no supported `find_package(NanoRos)` path and no `just install-local` flow. Per-platform CMake glue lives under `cmake/platform/`; RTOS-native integration shells live under `integrations/<rtos>/`.

Never hard-code project-relative paths in example CMake, package CMake, build scripts, or in-tree tooling. The outer build driver should pass SDK paths and selection via cache variables or environment variables such as `NANO_ROS_PLATFORM`, `NANO_ROS_RMW`, `CMAKE_TOOLCHAIN_FILE`, `<SDK>_DIR`, or board-specific config paths.

## Examples and Generated Content

Each `examples/` directory is a standalone copy-out template. Do not rely on workspace walk-up behavior from an example. Non-example test, benchmark, and smoke binaries belong under `packages/testing/{nros-tests/bins,nros-bench,nros-smoke}/`, not under `examples/`.

Do not modify vendored or generated content under `third-party/`, `packages/interfaces/*/generated/`, or build output directories unless the task explicitly requires regeneration. Generated message code should come from the nano-ros codegen tools, not hand-written edits.

## RMW and Platform Notes

Active RMW choices are `zenoh`, `xrce`, and `cyclonedds`; the legacy dust-DDS backend was retired. Platform choices include POSIX, Zephyr, bare-metal, FreeRTOS, NuttX, and ThreadX. RMW backend registration must be explicit on targets such as Zephyr/native_sim; do not assume POSIX-style Rust constructors or linker sections run there.

For Zephyr XRCE C++ service/action work, keep `nros_cpp_spin_once` routed through `executor.spin_once`. Do not reintroduce a `drive_io(0) + msleep` shortcut; that path starves reliable XRCE streams and skips arena dispatch.

CycloneDDS work is active. Native C++ action result/feedback paths have recent fixes, but stock ROS 2 interop and some embedded Cyclone paths remain ongoing work. Pure-Cargo Rust Cyclone examples are not the supported path; use the CMake/Corrosion route for Cyclone.

## Git and Worktree Rules

Preserve existing user changes in the worktree. Do not revert unrelated changes. Use linear history when integrating remote changes: `git pull --rebase` or `git fetch` plus `git rebase`; create merge commits only when explicitly requested.

When pulling or rebasing the superproject, inspect submodule changes. If a pull advances a submodule pointer and local work exists in that submodule, enter the submodule, fetch its remote, rebase local work onto the updated upstream commit, check out the commit expected by the superproject, and record the resulting submodule commit in the parent commit.

After rebasing over a remote submodule-pointer change, run `git submodule status --recursive <path>` and update the checkout to the commit recorded by `HEAD` before pushing. Recent pulls advanced `third-party/dds/cyclonedds`; leaving the worktree at the old detached commit made the superproject appear dirty even though the parent commit was correct.

## Practices & Pitfalls

### Agent Practices

- **Always `just ci` after a task.** Never `sudo` — tell the user.
- **Green CI locally BEFORE pushing** — run `just format` then `just ci`. CI stops at the first failing step; re-run until fully green. A toolchain bump can surface new pre-existing lints (e.g. rust-1.96 `unnecessary_cast` / `drop_non_drop` / `not_unsafe_ptr_arg_deref`) — fix them locally rather than discovering them remotely.
- **Always nightly for `rustfmt`** — `rustfmt.toml` enables nightly-only options; stable produces different output. Run `cargo +nightly fmt`.
- **Never merge in git.** Use `git pull --rebase` or `git fetch` + `git rebase`. Never create merge commits unless asked.
- **Submodule rebase on superproject pull:** if a pull advances a submodule pointer AND local work exists → enter it, fetch, rebase local onto upstream, check out the expected commit, record in parent. Never leave a submodule at an older commit when remote advanced.
- **Vendored-fork branch workflow** (cyclonedds, netxduo): land fixes with linear history. **Push the fork branch FIRST, then bump the superproject pointer.** By default, the agent does NOT push fork remotes (exfiltration guard) — the agent commits + rebases locally and leaves the branch ready; the maintainer pushes.
- **Don't modify vendored/generated:** `third-party/`, `packages/interfaces/*/generated/`, build output — unless the task explicitly requires regeneration. Preserve worktree changes.
- **Examples are standalone copy-out projects** (`examples/<plat>/<lang>/<example>/`); no workspace walk-up. Non-example bins under `packages/testing/{nros-tests/bins,nros-bench,nros-smoke}/`.
- **Unused vars:** `_name` + comment, or `#[allow(dead_code)]` for test struct fields.

### Platform Pitfalls

- **After clone, run ONE of** `direnv allow` / `source ./activate.sh` / `source ./activate.fish` — else `zpico-sys/build.rs` panics `"FREERTOS_PORT not set"`.
- **Zenoh pinned 1.7.2** (rmw_zenoh_cpp compat). zenohd from `third-party/zenoh/zenoh/`; zenoh-pico from `packages/zpico/zpico-sys/zenoh-pico/`. Tests auto-use `build/zenohd/zenohd`.
- **Rust edition 2024:** `unsafe extern "C" {}`, `#[unsafe(no_mangle)]`, explicit `unsafe {}` in `unsafe fn`. `nros-c` keeps `#![allow(unsafe_op_in_unsafe_fn)]`.
- **No POSIX-style Rust ctor sections on Zephyr/native_sim/RTOS** — backend registration is an explicit call. A pure-Rust image needs the REAL backend dep (`rmw-zenoh = ["dep:nros-rmw-zenoh"]`) — and a direct reference, or rustc's staticlib DCE drops the dep's `#[no_mangle]` export (symbol in the rlib, absent from the `.a`).
- **Domain ID:** compile-time on embedded (Kconfig / per-example `config.toml`), runtime env on native. `CONFIG_NROS_CYCLONE_DOMAIN_ID` defaults to `NROS_DOMAIN_ID` — never pin it to a literal in confs (the phase-180 split-brain silently ran every cyclone image on domain 0). Cyclone fixture pairs bake distinct domains (50–58) for parallel SPDP.
- **FreeRTOS:** `APP_TASK_STACK` 64 KB → "Invalid mbox" otherwise; IP-seeded `srand()`; poll-task priority ≥ 4; manual action server needs `try_handle_get_result()`.
- **Zephyr POSIX:** raise `CONFIG_MAX_PTHREAD_MUTEX_COUNT` (zenoh-pico needs ~8+; default 5 fails with -80).
- **Zephyr zsock serializes send/recv per-fd:** `Z_CONFIG_SOCKET_TIMEOUT` must stay 100 ms (5 s starves tx → lease death); intra-image pub→sub needs `Z_FEATURE_LOCAL_SUBSCRIBER=1`.
- **NuttX spin uses `sem_timedwait`** (pthread condvar hangs).
- **NetX Duo BSD `SO_RCVTIMEO` takes `nx_bsd_timeval*`, not `INT` ms** (deadlock otherwise).
- **smoltcp multicast:** join the GROUP addr, not `0.0.0.0`; LAN9118 needs promiscuous in QEMU.
- **QEMU:** `-icount shift=auto`; use `nros_tests::qemu::qemu_system_arm_cmd()`.
- **Embedded Cyclone:** transient samples use `ddsrt_{malloc,calloc,free}`, never libc — RTOS heap is separate.
- **XRCE:** flush `uxr_buffer_request_data` immediately; reliable `STREAM_HISTORY ≥ 2`.
- **Zephyr Rust allocator is picolibc `malloc`** — size `CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE` (default 16 KB), NOT `CONFIG_HEAP_MEM_POOL_SIZE`.
- **Manual native_sim pair repros need distinct `--seed`** — unseeded processes share entropy → identical GUIDs/ports → discovery sees the peer as itself → false-negative "no delivery".
- **Never clang-format `cmake/templates/*`** — reflow splits `@VAR@` configure_file tokens → generated TU fails "stray '@'". `.clang-format-ignore` guards.
- **Hand-mirrored FFI structs drift on append** (QoS `tx_express`, `callback_group` — 3×): mirror-only TU passes a SHORTER struct by value → tail field garbage. Gated: `check-ffi-struct-mirrors` (push lane) + cross-include TU in `check-c`. Include order is one-way: `nros_cpp_ffi.h` BEFORE `component.h`.
- **zpico shim + zenoh-pico library MUST share the generated zenoh config** — flag-gated struct fields (`Z_FEATURE_LOCAL_QUERYABLE`…) make mismatched TUs a silent ABI break. `build_c_shim` injects `ZENOH_GENERIC` + the OUT_DIR config. Local fixture binaries embed the shim — rebuild fixtures after zpico config changes.

### CMake Pitfalls

- **A cmake `include()` inside a FUNCTION scope drops the included file's normal
  variables when the frame pops** — only the function/macro *definitions* survive.
  Any module that captures its own dir (`set(_X_DIR "${CMAKE_CURRENT_LIST_DIR}")`)
  for later `configure_file` template lookups must use `CACHE INTERNAL` (the
  `_NROS_ENTRY_DIR` pattern). 287-W6: `_NROS_NODE_REGISTER_DIR` was a normal var,
  the workspace path includes NodeRegister inside `_nros_import_once()` (a
  function), and every FreeRTOS workspace member failed `configure_file` on
  `"/templates/freertos_entry_main_c_typed.cpp.in"` — posix never touches the
  templates, which hid it for months.
- **`find_program` HINTS are searched BEFORE the environment PATH.** A resolver
  that lists `~/.nros/bin` as a HINT lets a stale provisioned binary shadow the
  activate.sh-wired in-tree CLI (a June-era `nros` baked the retired pre-258
  bake shape and turned every `nros_system_generate` west fixture red). Use
  `PATHS` (searched AFTER PATH) for fallback locations, and keep
  `~/.nros/bin/nros` fresh after CLI-shape changes.
- **Case-normalize enum-ish cmake args at the function top**
  (`string(TOUPPER …)`) — the RFC-0048 verbs pass inferred values in lowercase
  (`cpp`), and a case-sensitive `STREQUAL "CPP"` chain silently falls into the
  wrong branch (287-W6: the Zephyr interface generator emitted C bindings for a
  C++ leaf → "std_msgs.hpp: No such file"). The canonical generator already
  normalizes; keep siblings in lockstep.
- **`cmake_parse_arguments` swallows positional args after a multi-value
  keyword** — a call like `nano_ros_add_node(n CALLBACK_GROUPS g src/A.cpp)`
  loses the source. Verbs accept an explicit `SOURCES` keyword for this; when
  extending a verb, add new multi-value keywords BEFORE the positional-sources
  convention breaks someone.
- **Old-shape CMake surface is retired** (287-W8 + post-287): `nano_ros_bootstrap`
  / `nano_ros_link` are `_nros_*` config internals, `nano_ros_deploy` /
  `nano_ros_application` / `nano_ros_component_register` are gone. The ament
  shape (`find_package(nano_ros REQUIRED)` + `nano_ros_add_executable` /
  `nano_ros_add_node`, deploy tuple in package.xml) is the only leaf shape;
  `example_shape.rs`'s FORBIDDEN list gates regressions.

### Rust Consumption (RFC-0048 W9)

- **`nros sync` owns each Rust leaf's `.cargo/config.toml` managed surface**: one
  `include = ["…/nros-patch.toml"]` line (the central, gitignored, absolute-path
  patch file at the checkout root) + the leaf-local `generated/*` and
  platform-specific `# nros-managed` patch lines. Never hand-edit them; a moved
  checkout needs one `nros sync` to re-point the central file.
- **Central-patch membership rule:** a crate may live in `nros-patch.toml` only
  if it is registry-named in EVERY consumer's dependency graph — cargo emits
  "patch `X` was not used in the crate graph" per unused entry, and the file is
  shared by all leaves. That limits it to the universal trio
  (`nros`/`nros-core`/`nros-serdes`); RMW/board/driver crates are NOT universal
  (verified: a freertos entry's slim graph lacks the cyclone/xrce crates).
- cargo's `config-include` is STABLE (verified on 1.96) — no nightly gate.

### Test Pitfalls

- **Tests must fail on unmet preconditions** (`assert!`/`bail!`/`nros_tests::skip!`). A bare `eprintln!`+`return` reports PASS. Same rule at runtime: panic, never silent early-return.
- **No compilation inside tests.** Compile in the build stage; test consumes prebuilt fixture.
- **Fixture mtime treadmill:** any pull/rebase — and any `git stash push`/`pop`, which rewrites tracked files just the same — refreshes source mtimes → EVERY prebuilt fixture reads STALE. Rebase once → rebuild affected fixtures → test WITHOUT pulling again. Core-crate or repr(C)-struct changes ⇒ wipe workspace build dirs (incremental mixes pre/post-append objects). Long-unrebuilt families "pass" on museum binaries — trust only a fresh full sweep.
- **Bare `cargo nextest` counts `nros_tests::skip!` panics as FAILURES.** Only `just test-all`'s junit rewrite converts the `[SKIPPED]` panic into a skip. When triaging a bare-run red, read the panic text first — "fixture not built"/`[SKIPPED]` reds are skips in CI semantics, not regressions.
- **Full-sweep QEMU lanes flake under load.** With the whole suite fanned out, concurrent QEMU boots miss readiness banners (287-W7: all six nuttx C/C++ rtos lanes failed 3/3 retries in-sweep, then passed solo). Retest a QEMU-lane red SOLO before filing an issue from it.
- **Build-side stale probes and test-side stale gates must watch the SAME inputs** (issue 0196): the native-rust fixture probe missed `generated/**`, so a month-old museum binary passed every "native OK" sweep while the test gate failed it. When adding a fixture family, source one shared staleness helper.
- **Test greps use `nros_tests::output::*` constants, never literal strings** — example banners/markers get slimmed (phase-277 broke ~10 tests grepping `"Result:"`/`"[OK]"`/old banners). If a test times out, FIRST diff the grep pattern against what the fixture actually prints.
- **Test names describe behavior, not phase numbers** — cross-reference a phase in a doc-comment, never the identifier.

### Multi-Session / Shell Pitfalls

- **Parallel agent sessions push to `main` concurrently.** `git fetch` + check
  `origin/main`'s highest issue id (including `archived/`) immediately before
  filing a `docs/issues/` entry; expect `docs/issues/README.md` rebase conflicts
  (merge both sides, renumber only your own files). Stash-wrap local-only files
  (`packages/zpico/zpico-sys/c/include/zpico.h`-style) around every rebase.
- **Write full logs of background builds/tests to files** and grep afterwards;
  `cmd | tail -N` swallows the mid-log error that explains the failure.
- **`pkill -f <pattern>` matches your OWN wrapper shell** when the pattern
  appears in the command string you are currently running (the agent shell's
  `zsh -c 'bash -c "… just check …"'` self-killed with exit 144). Kill by PID,
  or pick a pattern the current command line cannot contain.
- **zsh gotchas in agent shells:** unmatched globs abort the whole compound
  command (`rm -rf foo* && build` never builds — use `find`), and unquoted
  `$var` does NOT word-split (a loop over `$FILES` sees one giant argument —
  use `xargs` or explicit arrays).

## CLI Install & Submodule Operations

CLI install:

* `~/.cargo/bin/nros` + `~/.nros/bin/nros` are STALE shadows from pre-Phase-218 install paths — remove if present.
* Canonical install: `git submodule update --init packages/cli/third-party/<one-by-one as needed>` (NOT `--recursive`), then `just setup-cli`, then `source ./activate.sh`. PATH wires `nros`, `play_launch_parser`, `zenohd` from `~/.nros/sdk/*/bin/`.
* `just doctor` FAILs (not warns) on stale shadows + missing `play_launch_parser`.

Agent-dispatch contract:

* Every `just <plat>` invocation needs `source ./activate.sh` first; dispatch templates MUST source it. The pre-218 `export PATH="$HOME/.nros/bin:$PATH"` is INSUFFICIENT (misses `play_launch_parser`). CLAUDE.md “Practices” carries this.

Submodule init landmine:

* Never `git submodule update --init --recursive` from a worktree — the transitive closure pulls QEMU → OpenSSL → pyca-cryptography (~30 min). Init only what the task needs.
