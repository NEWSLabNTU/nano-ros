# Repository Guidelines

## Project Structure & Module Organization

This is a Rust workspace for a `no_std` ROS 2 client stack with C/C++ integration. Core crates live in `packages/core/`; RMW backends in `packages/zpico/`, `packages/dds/`, and `packages/xrce/`; board/platform support in `packages/platforms/`; and drivers in `packages/drivers/`. Rust integration tests are under `packages/testing/nros-tests/`; shell and smoke fixtures are in `tests/`. Examples are grouped by target in `examples/` (`native`, `qemu-arm-*`, `zephyr`, `stm32f4`). Build orchestration is in `justfile` and `just/*.just`; reference material is in `docs/`.

## Build, Test, and Development Commands

- `just --list`: show public recipes.
- `just setup`: install toolchains and local tools.
- `just build`: build the workspace plus generated bindings and transport artifacts for normal development.
- `just build-examples`: compile the workspace and example matrix.
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
