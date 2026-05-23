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

## Coding Style & Naming Conventions

Rust uses edition 2024 and `rustfmt.toml` with nightly-only formatting options. Use `cargo +nightly fmt` or `rustup run nightly cargo fmt`; stable rustfmt produces different output. C and C++ follow `.clang-format` based on LLVM, 4-space indentation, and a 100-column limit. Keep crate names and package paths in the existing `nros-*`, `zpico-*`, backend-specific, and platform-specific patterns.

Project naming:

- `nano-ros`: prose and docs.
- `nros`: crates, Rust/C identifiers, and `CONFIG_NROS_*`.
- `nano_ros`: C header dir, CMake targets, and CMake helpers such as `nros_generate_interfaces()`.

## Testing Guidelines

Prefer the narrowest tier that covers the change. Reusable Rust integration tests belong in `packages/testing/nros-tests/tests/`; shell tests belong in `tests/`; temporary tests can start as Bash and should be promoted when reused. Tests must fail on unmet preconditions with `assert!`, `bail!`, or the project skip helper; do not silently `eprintln!` and return from a test.

For platform failures, rerun the closest platform recipe first, for example `just zephyr build-all`, `just freertos build-fixtures`, or `just qemu build`, before spending time on root `just build-all`.

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
