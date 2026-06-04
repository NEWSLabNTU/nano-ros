# Repository Guidelines

## Project Structure & Module Organization

nano-ros is a Rust workspace for a `no_std` ROS 2 client stack with C/C++ integration. Core crates live under `packages/core/`; RMW backends under `packages/zpico/`, `packages/xrce/`, and `packages/dds/`; board/platform support under `packages/boards/` and `packages/platforms/`; drivers under `packages/drivers/`; and reusable integration tests under `packages/testing/nros-tests/`. Shell and smoke fixtures live in `tests/`. Examples are standalone copy-out projects under `examples/`, with the canonical shape `examples/<platform>/<language>/<example>/`; the RMW is selected at build time.

Reference and contributor docs live in `docs/`; user-facing mdBook docs live in `book/src/`; build orchestration lives in `justfile` and `just/*.just`.

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

Rust uses edition 2024 and `rustfmt.toml` with nightly-only formatting options. Use `cargo +nightly fmt` or `rustup run nightly cargo fmt`; stable rustfmt produces different output. C and C++ follow `.clang-format` based on LLVM, 4-space indentation, and a 100-column limit. Keep crate names and package paths in the existing `nros-*`, `zpico-*`, backend-specific, and platform-specific patterns.

Project naming:

- `nano-ros`: prose and docs.
- `nros`: crates, Rust/C identifiers, and `CONFIG_NROS_*`.
- `nano_ros`: C header dir, CMake targets, and CMake helpers such as `nros_generate_interfaces()`.

## Testing Guidelines

Prefer the narrowest tier that covers the change. Reusable Rust integration tests belong in `packages/testing/nros-tests/tests/`; shell tests belong in `tests/`; temporary tests can start as Bash and should be promoted when reused. Tests must fail on unmet preconditions with `assert!`, `bail!`, or the project skip helper; do not silently `eprintln!` and return from a test.

For platform failures, rerun the closest platform recipe first, for example `just zephyr build-all`, `just freertos build-fixtures`, or `just qemu build`, before spending time on root `just build-all`.

Native C/XRCE tests are runtime-only and consume prebuilt CMake fixtures under `examples/native/c/{talker,listener}/build-xrce/`. If `c_xrce_api` fails with `Test fixture binary not prebuilt`, run `just native build-fixtures` or `just build-test-fixtures` before debugging runtime behavior. A focused verification of phase 177.9.C passed with `just native test-c-xrce verbose` after fixture prebuild.

## SDK Environment Defaults

Keep repo-local SDK defaults centralized in `just/sdk-env.just`. This includes paths such as `FREERTOS_DIR`, `NUTTX_DIR`, `THREADX_DIR`, `PX4_AUTOPILOT_DIR`, `NROS_ESP_IDF_WORKSPACE`, `NROS_ESP_IDF_ENV_SHIM`, and `IDF_PATH`. Local overrides belong in `.env` or the caller environment.

Do not duplicate those defaults in package code, tests, examples, CMake, or scripts. Packages and examples must remain position-independent: they may read explicit environment variables and should skip or fail with a clear setup hint when a required SDK variable is absent, but they must not assume the checkout lives at a particular repo-relative path.

Shells that need the same defaults should source `scripts/sdk-env.sh`, which evaluates `just/sdk-env.just` and exports only missing variables. `.envrc`, `setup.bash`, and `setup.fish` all use that adapter. When a direct `cargo test` or `cargo nextest` run needs these defaults, either source `scripts/sdk-env.sh` first or run it through a `just` recipe so `just/sdk-env.just` is imported and exported to the child process. Prefer adding a focused `just` test helper over adding repo-path fallbacks inside `packages/`.

## Toolchain & SDK Provisioning (Phase 187 — landed)

Host toolchains/tools (`qemu`, cross-GCC, `zenohd`, `openocd`) are provisioned by
`nros setup`, not built ad-hoc. `nros-sdk-index.toml` (repo root) is the SSOT:
each `[tool.*]` has per-host prebuilt `dist` (sha256-pinned) **and** a
`[tool.*.source]` recipe; `[source.*]` build with the app; `[gated.*]` (NVIDIA
SPE, ARM FVP) are never fetched, only instructed. Prebuilt assets live on the
**separate** `NEWSLabNTU/nano-ros-sdk` repo's Releases (not a submodule —
referenced by URL); `ci/nano-ros-sdk/` is the drop-in seed for that repo.

- `nros setup <board>` / `nros setup --tool <name> [--prefix <dir>]` installs to
  `$NROS_HOME/sdk/<tool>/<version>/` (identical layout whether prebuilt-fetched
  or source-built; `.nros-provenance` + `nros-sdk.lock` record it).
- **Method A:** `nros build`/`deploy` lazy-install the board's tools, then
  prepend the locked store `bin/` dirs to the child PATH — `nros` is the single
  SDK resolver. **Non-`nros` scripts, the test harness, CMake do NOT resolve SDK
  paths — assume the SDK is given and only check + warn** (`nros doctor` /
  `just <plat> doctor`). Do not re-add store-path probing to test code.
- `just qemu setup-qemu` and `just zenohd setup` are **thin `nros setup --tool`
  callers** (install into `build/<tool>` where the harness already looks; zenohd
  symlinks the flat path). Do not reintroduce the in-tree configure/make or the
  2.7 GB `third-party/qemu` submodule build. Tool versions live only in the
  index. `just <tool> build` still source-builds for devs who want it.
- CI: `.github/workflows/sdk-index-gate.yml` (read-only sha256 verify of any
  index dist change); `nano-ros-sdk`'s `build-tool.yml` seeds assets via
  `workflow_dispatch` (Ubuntu 22.04 baseline). Bump a tool's `-nros<N>` suffix
  when rebuilding the same upstream version with different config.
- Validated with the prebuilt `nros2` qemu (the flipped `just qemu setup-qemu`
  fetches it): `just qemu test-lan9118` 5/5, and the full networked pub/sub e2e
  `nros-tests::emulator::test_qemu_rtic_pubsub_e2e` (two ARM baremetal instances
  over LAN9118 + slirp + zenohd, published=1/received=1). The qemu-recipe flip
  is safe end-to-end.
- ESP32 in nano-ros is **ESP32-C3 (RISC-V)**: `riscv32imc-unknown-none-elf` via
  the rustup target + build-std (rust-lld, no external gcc), espflash runner,
  Espressif `qemu-system-riscv32` fork. It needs **no index host-tool** —
  `resolve_packages` no longer maps esp32 to a (nonexistent) xtensa toolchain.
  An `esp-qemu` index tool (the Espressif qemu fork) is the only future SDK
  candidate here.
- **Open follow-up:** maintainer adds branch-protection requiring the
  `sdk-index-gate` check (Settings → Branches, or `gh api -X PUT
  repos/NEWSLabNTU/nano-ros/branches/<branch>/protection/...`). cross-gcc (apt
  system prereq) and openocd (`nros setup --tool openocd`) have no per-module
  `just setup` recipe to flip — no action needed. See
  `docs/roadmap/archived/phase-187-*`.

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

## Work in Progress — Handover

### Phase 221.A — `zpico-sys/build.rs` refactor (NOT started, mapping only)

Branch `phase-221-a-zpico-build-refactor` exists with **no commits** (empty,
forked from `main`). No source changed. The goal is Track A.1 of
`docs/roadmap/phase-221-antipattern-audit-findings.md`: extract logic from
`packages/zpico/zpico-sys/build.rs` (1978 LOC) into a new sibling helper crate
`packages/zpico/nros-zpico-build`, shrinking `build.rs` to a thin wrapper
< 200 LOC, with helpers covered by unit tests. Mirror the existing
`packages/core/nros-build-paths` / `nros-sizes-build` helper-crate pattern;
register the new crate in the root `Cargo.toml` `[workspace] members` list.

Critical correctness constraint discovered during mapping:

- **Any fn that emits `println!("cargo:...")` MUST stay executed from
  `build.rs`** — `cargo:` directives are inert when emitted from a
  non-build-script crate. So `env_usize`, the `*::from_env` constructors,
  `build_c_shim`, `build_zenoh_pico_unified`, `probe_net_type_sizes`, and
  `use_system_zenoh_pico` stay in `build.rs` (or keep their directive emission
  there). Only pure data/parse/generate fns move.
- Make the `generate_*` fns pure (`-> String` returning the file body) so they
  are unit-testable; `build.rs` keeps the `fs::write` to `OUT_DIR`.
- Movable (pure): `is_embedded_target`, `extract_function_name`,
  `extract_typedef_name`, `is_plausible_generated_header`, `post_process_header`,
  `arch_matches`, `apply_arch`, `add_c_sources_recursive`,
  `add_zenoh_pico_core_sources`, `detect_riscv_compiler`, `get_picolibc_sysroot`,
  `has_picolibc_specs`, `read_symbol_size`, the `ShimConfig` / `ZenohBufferConfig`
  struct data + their pure methods (`generate_rust_consts` → return String,
  `apply_to_cc`, `generate_config_header`), and the header/`.pc`/version
  generators.

Verification gate (env must be active — `source ./activate.sh`, sets
`FREERTOS_PORT` etc.): `cargo check -p zpico-sys` is **green at baseline**
(~21 s, default features) — run it after every extraction step. Default features
exercise only one platform path; broad platform coverage relies on `just ci`.

Next step was writing the implementation plan
(`docs/superpowers/plans/`), then executing the extraction as sequential,
`cargo check`-gated steps (the moves share the new crate, so they are not
parallelizable). A full structural map of `build.rs` (every fn, line range, the
MOVE/STAY split, env vars, and OUT_DIR outputs) was produced but not yet
written to a file — regenerate it with a read-only `Explore` pass over
`build.rs` if resuming in a fresh session.
