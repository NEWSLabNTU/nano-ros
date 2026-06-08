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

## Handover Notes (2026-06-08, session 2)

Active branch: `main`. The seven local commits from session 1 were pushed to
`origin/main` (`50248367a..35b28a091`, clean fast-forward, no divergence).
Session 2 then landed Phase 225.O groundwork; `bf308a0b1` is committed locally
on `main` and **not yet pushed**. Worktree dirty only on `AGENTS.md` (this
handover update).

### Phase 225 current state

Phase 225 still not fully closed. The three 225.O checkboxes remain unchecked —
all are infra-blocked, now narrowed by a parallel investigation (see the
refreshed "Remaining blockers" in
`docs/roadmap/phase-225-workspace-fixture-migration.md`):

- `qemu_nuttx_entry` — libc-patch gap **wired** in
  `workspace-fixtures-build.sh` (necessary, no-op until a NuttX row exists).
  Still blocked by: ws-sync renders one merged `.cargo/config.toml` and the
  NuttX board template's global `[build] target`/`[unstable] build-std` would
  poison `native_entry` + force build-std everywhere (CLI/board-template fix);
  and the standalone `nros::main!` NuttX deploy shape is unverified (all
  existing NuttX Rust examples are `libapps.a` staticlib Components).
- Zephyr Entry — tractable but multi-day. Recommended Approach A: emit a
  workspace-Entry leaf from `zephyr-fixture-leaves.sh` so the existing
  `zephyr-fixture-run-one.sh` west path builds it (the workspace lane has no
  west branch and a different codegen contract).
- ESP32 Entry — not tractable in one pass. A latent macro bug was **fixed**
  (`main_macro.rs` esp32 -> `Esp32C3`, was nonexistent `Esp32`). Still blocked
  by `NullNodeRuntime` in the bare-metal driver (awaiting 212.N.4), WiFi-only
  board with no CI-runnable OpenETH `BoardEntry`, and nightly + scoped
  `-Z build-std` plumbing.

Everything else in the phase doc is checked. Product examples and CLI workflow
fixtures no longer use product-facing `NodeId`, `EntityId`, `CallbackId`,
`ComponentContext`, `ComponentResult`, `ExecutableComponent`, or
`nros::Component`. Remaining `CallbackId` references are intentional dispatch
internals in `book/src/internals/dispatch-strategy.md` and
`packages/testing/nros-tests/tests/phase216_a_dispatch_strategy.rs`.

### Verification run this session

- `cargo check -p nros-macros` (clean, after the esp32 -> `Esp32C3` fix)
- `bash -n scripts/build/workspace-fixtures-build.sh` (syntax OK)
- `scripts/build/workspace-fixtures-build.sh native rust` (green; confirms the
  new NuttX libc-patch call is a no-op for non-NuttX rows)

### Push handoff

`origin/main` is at `35b28a091` (session-1 commits pushed). One unpushed local
commit remains: `bf308a0b1 fix(workspaces): wire nuttx libc patch + fix esp32
board mapping`, plus this `AGENTS.md` update. If resuming with intent to
publish, `git fetch origin`, rebase if needed, then push.

### Verification already run in session 1 after the final rebase

- `cargo test -p nros --quiet`
- `cargo test --manifest-path packages/cli/nros-cli-core/Cargo.toml --test orchestration_generate --quiet`
- `python3 scripts/build/fixtures-manifest.py validate-workspaces --platform native`
- `python3 scripts/build/fixtures-manifest.py validate-workspaces --platform freertos`
- `python3 scripts/build/fixtures-manifest.py validate-workspaces --platform threadx-linux`
- `scripts/build/workspace-fixtures-build.sh native rust`
- `scripts/build/workspace-fixtures-build.sh native mixed`

Earlier, before rebasing over `50248367a`, these also passed:

- `scripts/build/workspace-fixtures-build.sh native c`
- `scripts/build/workspace-fixtures-build.sh native cpp`
- Rust formatting over dirty Rust files
- `clang-format --dry-run --Werror` over changed C/C++ API and example files
- `python3 -m py_compile scripts/build/fixtures-manifest.py`
- `bash -n scripts/build/workspace-fixtures-build.sh`
- `git diff --check`
- `cargo check --manifest-path packages/cli/cargo-nano-ros/Cargo.toml --quiet`
- `cargo check -p nros-macros --quiet`

## Handover Notes (2026-06-04)

Session ended out-of-tokens mid-stream. Active in-flight work: none recorded.

### Closed this session

* **Phase 221 — Build System + Test Antipattern Audit Findings** — ARCHIVED at `docs/roadmap/archived/phase-221-antipattern-audit-findings.md`. Tracks A–E closed; A.7 remains explicitly deferred to later top-level CMake work outside Phase 221. Closure verification passed with `cargo check -p zpico-sys -p nros-c -p nros-cpp` and `source ./activate.sh && XDG_RUNTIME_DIR=/tmp just build`.
* **Phase 220 — Full-Suite Readiness Sweep** — ARCHIVED at `docs/roadmap/archived/phase-220-full-suite-readiness-sweep.md`. All 10 tracks (A–J) closed. Final commit chain ends at `c05830b2c`. Followups identified for Phase 222 (CLI surface) territory; one for codegen `--target <rmw>` honor.

### CLI install reality post-218

* `~/.cargo/bin/nros` + `~/.nros/bin/nros` are STALE shadows from pre-Phase-218 install paths. Remove if present.
* Canonical install: `git submodule update --init packages/cli/third-party/<one-by-one as needed>` (NOT `--recursive`), then `just setup-cli`, then `source ./activate.sh`. PATH wires `nros`, `play_launch_parser`, `zenohd` from `~/.nros/sdk/*/bin/`.
* `just doctor` now FAILs (not warns) on stale shadows + missing `play_launch_parser` (Phase 220.A + 220.J).

### Agent-dispatch contract (Phase 220.J)

* Every `just <plat>` invocation needs `source ./activate.sh` first. Dispatch templates MUST source it.
* Pre-218 pattern `export PATH="$HOME/.nros/bin:$PATH"` is INSUFFICIENT — misses `play_launch_parser` under `~/.nros/sdk/play_launch_parser/bin/`.
* CLAUDE.md "Practices" section carries the contract.

### Submodule init landmines

Never run `git submodule update --init --recursive` from a worktree. The transitive closure pulls QEMU → OpenSSL → pyca-cryptography (~30 min wasted in 220.G first attempt). Only init what the immediate task needs.
