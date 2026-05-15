# Repository Guidelines

## Project Structure & Module Organization

This is a Rust workspace for a `no_std` ROS 2 client stack with C/C++ integration. Core crates live in `packages/core/`; RMW backends in `packages/zpico/`, `packages/dds/`, and `packages/xrce/`; board/platform support in `packages/platforms/`; and drivers in `packages/drivers/`. Rust integration tests are under `packages/testing/nros-tests/`; shell and smoke fixtures are in `tests/`. Examples are grouped by target in `examples/` (`native`, `qemu-arm-*`, `zephyr`, `stm32f4`). Build orchestration is in `justfile` and `just/*.just`; reference material is in `docs/`.

## Build, Test, and Development Commands

- `just --list`: show public recipes.
- `just setup`: install toolchains and local tools.
- `just build`: build the workspace plus generated bindings and transport artifacts for normal development.
- `just build-examples`: compile the workspace and example matrix.
- `just <platform> build`: build platform-scoped core artifacts when that platform has them.
- `just <platform> build-examples`: compile runnable examples for one platform.
- `just <platform> build-fixtures`: prebuild test fixtures for one platform.
- `just <platform> build-all`: run the platform-scoped full tier (`build`, `build-examples`, and `build-fixtures`) before using root `just build-all` for broad matrix coverage.
- `just build-test-fixtures`: prebuild binaries required by the full test matrix.
- `just test-unit`: run fast workspace unit tests with no external services.
- `just test`: run the standard dev tier; skips heavy platform/ROS 2 groups.
- `just test-all`: run the full matrix, doctests, Miri, and C codegen tests. Run `just build-test-fixtures` first.
- `just check`: run formatting and clippy checks across Rust, C, C++, and Python surfaces.

## Coding Style & Naming Conventions

Rust uses edition 2024 and `rustfmt.toml` with crate-level import grouping and formatted doc-comment examples. Run `just format` before broad changes. C and C++ follow `.clang-format` based on LLVM, 4-space indentation, and 100-column limits. Keep crate names and package paths in the existing `nros-*`, `zpico-*`, and backend-specific patterns.

## Testing Guidelines

Prefer the narrowest tier that covers your change. Put Rust integration tests in `packages/testing/nros-tests/tests/` and use clear feature or backend names such as `services.rs`, `rmw_interop.rs`, or `custom_transport.rs`. Assign heavy platform tests to the right nextest group in `.config/nextest.toml` so `just test` remains fast.

## Commit & Pull Request Guidelines

Recent commits use short, imperative subjects with optional scopes, for example `ci: fix Deploy Book workflow` or `phase-124.F: session-level connectivity probe across the full stack`. Keep PRs focused, list tested commands, link issues or roadmap phases, and include logs or screenshots only when they clarify platform, ROS 2, or generated-artifact behavior.

When integrating remote changes, prefer a linear history: use `git pull --rebase` or `git fetch` followed by `git rebase`, not merge commits. Only create a merge commit when the user explicitly asks for one.

## Agent-Specific Instructions

Do not modify vendored or generated content under `third-party/`, `packages/interfaces/*/generated/`, or build output directories unless the task explicitly requires regeneration. Preserve existing user changes in the worktree.

For platform-specific build failures, rerun the narrow platform recipe first, for example `just <platform> build-examples`, `just <platform> build-fixtures`, or `just <platform> build-all`, before spending time on root `just build-all`.

For Zephyr XRCE C++ service/action work, the C++ CFFI backend link/init issue was fixed in `ffdde60f`; do not assume POSIX-style Rust constructors run on Zephyr/native_sim, and prefer explicit backend registration. Force rebuild the XRCE C++ service/action fixtures or verify stale-fixture detection before focused E2E reruns. The runtime spin/cv-wait hang that starved reliable XRCE retransmission (service) and blocked `send_goal` (action) is patched: `Executor::spin_once` now skips the std `wake_cv.wait_timeout_while` on Zephyr+std and routes the full timeout into `drive_io`, so `nros_cpp_spin_once` calls `executor.spin_once` directly without a bypass. Do not reintroduce a `drive_io(0) + msleep` shortcut in `nros_cpp_spin_once` — that path starves reliable XRCE streams and skips arena dispatch.

When pulling or rebasing the superproject, always inspect submodule changes. If a pull changes a submodule pointer and we also have local work in that submodule, enter the submodule, fetch its remote, rebase the local work onto the updated upstream commit, and then check out the rebased/up-to-date commit expected by the superproject. Record the resulting submodule commit in the parent commit. Do not leave a submodule at an older local commit after observing that the remote pointer advanced.
