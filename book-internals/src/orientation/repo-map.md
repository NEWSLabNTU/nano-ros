# Repo Map

What each top-level directory holds. The `packages/` breakdown is kept in one
place, the "Directory map" in
[RFC-0001](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0001-architecture-overview.md),
and `check-package-directories` checks it against the tree, so it is not
repeated here.

| Directory | Holds |
| --- | --- |
| `packages/` | Every crate and C port: language APIs, core, RMW backends, platforms, boards, drivers, CLI, runners, tooling, tests, verification. See RFC-0001's directory map. |
| `examples/` | Standalone copy-out projects, shaped `examples/<platform>/<language>/<example>/`. The RMW is chosen at build time. |
| `book/` | The nano-ros Book (users and porters), plus `page-map.toml`, which records which book and part every page belongs to. |
| `book-internals/` | This book. |
| `docs/` | The records: `design/` (RFCs), `roadmap/` (phases), `issues/`, plus `development/`, `reference/` and `research/`. |
| `just/`, `justfile` | Build orchestration. `just/check/*.just` holds the gates, and the fast lane is derived from those recipes. |
| `scripts/` | Generators (`gen-*`), gates (`check-*`, `scripts/check/`), probes (`scripts/probe/`) and shared helpers (`scripts/lib/`). |
| `cmake/` | The CMake road: `find_package(NanoRos)` support, board CMake and ament-surface shims. |
| `zephyr/` | The Zephyr module (west road), with its Kconfig generated from the knob registry. |
| `integrations/` | Glue for host tools that are not nano-ros's own build: FreeRTOS, NuttX, PlatformIO, PX4, S32DS. |
| `config/` | Toolchain and target lists, and per-family defaults. |
| `templates/` | Scaffolds for out-of-tree boards and cargo-zephyr projects. |
| `tests/` | Shell and CMake fixture tests. The Rust integration tests live in `packages/testing/nros-tests`. |
| `docker/`, `ci/` | Container images for development and CI. |
| `locks/` | Registry lockfile pins. |
| `third-party/` | Provisioned SDKs and RTOS sources (gitignored; `nros setup` fills it). |
| `tools/` | The host environment and the pinned Rust toolchain for tools. |
| `changelog.d/` | towncrier fragments, one per change. |
