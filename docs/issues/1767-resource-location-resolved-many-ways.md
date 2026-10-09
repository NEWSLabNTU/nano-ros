---
id: 1767
title: "A resource's location is decided by 13 mechanisms, and 13 resources are
  resolved two or more different ways"
status: open
type: tech-debt
area: build
severity: medium
found: 2026-10-09
related: [0196, 0460, 0491, 0500, 0616, 1025, 1280, 1527, 1558, 1560, RFC-0014, RFC-0049, RFC-0072, RFC-0094, RFC-0101]
---

## What this is

A census on 2026-10-09 asked one question across every road (cargo build
scripts, cmake, west/Zephyr, `just`, `scripts/`): **how does a build find a
resource it did not produce?** Resources are vendored/SDK source trees,
toolchains and tools, and generated artifacts one step hands to the next.

RFC-0101 answered this for the cargo road's source trees (one owner per graph,
`links` hand-off, `nros_build_paths` resolver) and left the cmake and west
roads out of scope by name (§6: "a tree resolved on two roads is resolved twice
by construction"). That exclusion is where most of the divergence below lives.

## The 13 mechanisms

| # | mechanism | shared helper |
| --- | --- | --- |
| 1 | env-first + in-repo default + re-root (cargo) | `nros_build_paths::{env_or_repo_path,env_path}` |
| 2 | env-first + default + re-root (shell) | `just/sdk-env.just` + `scripts/lib/checkout-paths.sh` |
| 3 | cargo `links` / `DEP_*` | cargo itself (3 live keys) |
| 4 | marker walk-up to the checkout root | `repo_root()`, `MONOREPO_MARKER`, cmake `_nros_find_root` |
| 5 | counted / relative literal | none (`CMAKE_CURRENT_LIST_DIR/../..`: 68 hits in 22 files) |
| 6 | descriptor token interpolation | three interpolators: `nros-platform-config` `interpolate`, `site_config.rs`, `cargo_config.rs` `${workspace}` |
| 7 | cargo config `[env]` / `include` | `nros sync` writes `nros-patch.toml` |
| 8 | cmake cache var → `ENV{}` → default | none per SDK; no re-root |
| 9 | `find_program` / `find_package` / `command -v` | `nros_resolve_cli`, `ProvideCycloneDDS.cmake`, `NanoRosCorrosion.cmake` |
| 10 | SDK store `<store>/<tool>/<pin>` | pin parsed 4 ways (Rust index, Rust mini-parser, awk, cmake) |
| 11 | ament prefix search | `nros_zenohd_bin` + Rust twin — the one CLEAN mechanism |
| 12 | cargo JSON `build-script-executed` → `OUT_DIR` | 3 parsers |
| 13 | fixed path-rule function for a generated artifact | `model_location`, `descriptor_path`, `nros_fixture_row_artifact_dir`, … |

## Resources resolved more than one way

Each line is one resource; every arm was read at the cited site.

1. **FreeRTOS kernel / lwIP — seven arms.** `freertos_dir()` (env, re-rooted,
   submodule default); `nros-board-freertos/build.rs:67` (raw, skips when
   unset); `nros-platform.toml` `{envpath:}` (fails when unset); 81
   `system.toml` `sdk = {env:…}` rows across FreeRTOS/NuttX/ThreadX, resolved
   through a raw `std::env::var` closure (`cmd/board_facts.rs:486`, no
   re-root); cmake board modules (cache → ENV → counted walk, no re-root);
   `sdk-env.just` (re-rooted); `just/freertos.just:446` (literal path).
2. **ThreadX config / includes.** `sdk-env.just:113` defaults
   `THREADX_CONFIG_DIR` to the linux board's config; the RISC-V board's
   `nros-board.toml` FORCE-sets another (force beats env, the reverse of
   RFC-0049's ladder); `THREADX_EXTRA_INCLUDES` hard-codes
   `third-party/threadx/kernel` independent of `THREADX_DIR`; cmake
   `rv-virt-threadx.cmake:105` is a third resolution.
3. **Cyclone DDS — four arms.** cargo `env_or_repo_path("CYCLONEDDS_SOURCE_DIR")`;
   cmake `ProvideCycloneDDS.cmake`; Zephyr `nros_rmw_cyclonedds.cmake:18`
   hard-codes `${NROS_REPO_DIR}/third-party/dds/cyclonedds` under a different
   name (`CYCLONEDDS_DIR`), ignoring the override; the index carries both
   `[tool.cyclonedds]` and `[source.cyclonedds-src]`.
4. **`nros-c` headers — three arms.** `nros_c_include()` honours
   `NROS_C_INCLUDE`; `freertos_build.rs`, `nros-board-threadx-linux/build.rs:53`
   and `threadx_qemu_riscv64_build.rs` join `repo_root()` and ignore it;
   `DEP_NROS_C_INCLUDE` via `links`.
5. **The nano-ros root — five ladders, three env names, two markers.** CLI
   (`--nano-ros-path` → `NROS_REPO_DIR` → walk → install prefix); cmake
   `_nros_resolve_root` (`NANO_ROS_ROOT`, no install rung); Zephyr
   `zephyr/CMakeLists.txt` (`CMAKE_CURRENT_LIST_DIR/..`); `repo_root()` (a
   different marker file than `CHECKOUT_MARKER`); `cmake-incremental.sh`
   (`${NROS_REPO_DIR:-${NANO_ROS_ROOT}}`).
6. **The SDK store root — four orders.** CLI `NROS_STORE` → `NROS_HOME` →
   `~/.nros`; `activate.sh` and `NanoRosCrossToolchain.cmake` `NROS_SDK_STORE`
   → `NROS_HOME/sdk`; `NanoRosCorrosion.cmake` `NROS_HOME/sdk` only;
   `nros_build_paths::riscv64::sdk_store` and `riscv64-toolchain.sh`
   `NROS_SDK_STORE` → `$HOME/.nros/sdk`, ignoring `NROS_HOME`. Only the CLI
   honours `NROS_STORE`.
7. **The Zephyr workspace.** `scripts/lib/zephyr-workspace.sh:102` has a store
   rung; the CLI's `zephyr_base` (`cmd/build.rs`) has none, while its doc
   comment says it copies the shell ladder verbatim.
8. **`nros-launch-resolve`.** Rust `model_location.rs` (env → re-rooted
   `NROS_REPO_DIR` → crate target → store); shell
   `launch-resolver-identity.sh` (no store, no re-root); two literal paths.
9. **The `nros` CLI.** cmake (PATH, then store); `just` literals
   (`packages/cli/target/release/nros` in `check/codegen.just`,
   `check/cmake.just`) or `${NROS_CLI:-literal}`; `activate.sh` PATH.
10. **QEMU.** a project-local `build/qemu` prefix in three `.just` files, a
    `QEMU_SYSTEM_RISCV32` env in Rust, `command -v` in a third.
11. **The SystemModel.** `model_location`, a cmake mirror of its ladder
    (`NanoRosEntry.cmake:599`), a literal in `compile-check-fixtures.sh:402`.
12. **`nros_config_generated.h` — four arms.** `$CARGO_TARGET_DIR/nros-c-generated`,
    `DEP_NROS_C_CONFIG_INCLUDE`, three `OUT_DIR` JSON parsers,
    `${NANO_ROS_ROOT}/target/...` (PX4).
13. **A fixture's artifact dir.** shell `nros_fixture_row_artifact_dir_by_id`
    vs Rust `groups.rs` `workspace_artifact_dir` (issue 1025's class).

## Why it matters

Every arm above computes a correct answer under the configuration its author
tested, so every gate is green. They diverge exactly where a user or a second
checkout differs from that configuration — an override set (`CYCLONEDDS_SOURCE_DIR`
reaches cargo and not Zephyr), a non-default store (`NROS_STORE`), a linked
worktree (the cmake rows have no re-root, issue 1280's shape). That is the class
issues 0500, 0616, 1025, 1280 and 1527 each fixed one instance of.

RFC-0101 §5's conformance table is also partly stale: the ThreadX RISC-V
`env_path_or`, `ZENOH_PICO_DIR` and `NV_SPE_FSP_DIR` now route through
`env_path`.

## Direction

The design is RFC-0103 ([one owner per fact](../design/0103-one-owner-per-fact.md)):
one resolver (`nros_build_paths` + `nros locate`), store first, and every road
reading the resolved answer rather than re-deriving it — the RFC-0094 rule
("one place decides, every other place reads") extended from knobs to
locations and selections.
