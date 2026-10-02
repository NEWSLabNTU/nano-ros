---
id: 1614
title: "Gate re-run 2026-10-01, W5: 18 gates still read a population narrower than their rule — scan roots, file kinds and single-file producers"
status: resolved
resolved_in: 2026-10-02
type: tech-debt
area: testing, build
severity: medium
found: 2026-10-01
related: [phase-472, 1614, 1615, 1616, 1617, 1618]
---

## What

The 2026-10-01 re-run of the phase-472 audit
([findings](../../development/audit-findings-2026-10-01-rerun.md)) re-applied each
mutation the 2026-09-28 audit recorded, plus new audits and spot-checks. Every
gate below still exits **0** on the mutation in its row, and each has a positive
CONTROL that exits non-zero: the same defect placed where the gate does read.
So these are measured holes, not readings.

Class: **W5 — scan roots that stop short of the tree**.

| class | gate · facet | mutation | rc | control rc | source |
| --- | --- | --- | ---: | ---: | --- |
| W5 | `check-board-name-reach.py` | `packages/boards/nros-board-nuttx-qemu/nros-board.toml`: names = ["riscv-qemu", "nuttx-riscv", | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-build-tool-verbs-exempt.py` · top-verb | `cmake/NanoRosLink.cmake` += execute_process(COMMAND "${_NROS_CLI}" rerun-verb) | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-build-tool-verbs-exempt.py` · zephyr-ws | `zephyr/cmake/nros_generate_interfaces.cmake` += execute_process(COMMAND "${_NROS_CLI}" ws rerun-sub) | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-c-knob-guard-order.py` | `packages/api/nros-cpp/include/nros/node.hpp`: #if NROS_COMPONENT_MAX_TIMERS < 1 | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-ci-doc-workflow-refs.py` | `docs/development/ci-conventions.md`: See `.github/workflows/rerun-nonexistent.yml` for the pattern. | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-cmake-image-policy.py` | `cmake/platform/nano-ros-nuttx.cmake`: (delete) nros_apply_panic_policy(platform "nros_platform_link_app(${ta; `cmake/board/nano-ros-board-rv-virt-threadx.cmake`: (delete) nros_apply_panic_policy(platform | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-cross-toolchain-provenance.py` · 5th-site | `cmake/board/nano-ros-board-mps2-an385-freertos.cmake` += set(CMAKE_C_COMPILER "/usr/bin/arm-none-eabi-gcc") | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-dist-floors.py` · rust-rustup | `nros-sdk-index.toml`: dist.linux-riscv64 = { url = "https://example.invalid/rustup-init", sh | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-feature-gated-modules.sh` · submodule | `packages/core/nros-node/src/executor/mod.rs` += mod rerun_mod;; new `packages/core/nros-node/src/executor/rerun_mod.rs` | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-generated-leaf-regenerable.sh` · outside-pathspec | new `integrations/rerun_leaf/Cargo.toml` | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-message-crate-identity.py` · workspace-deps | `Cargo.toml`: nros-std-msgs = { path = "packages/interfaces/generated/humble/nros-st | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-no-allow-multiple-def.sh` · zephyr-cmake | `zephyr/cmake/nros_cargo_build.cmake` += target_link_options(rerun PRIVATE -Wl,--allow-multiple-definition) | 0 | 1 | new audit |
| W5 | `check-orphan-generated-stamp.py` · corrosion-cargo | remove `build/corrosion-cargo/threadx-riscv64/56c8585c79ad/rerun_8e5ec/nros-c-generated/nros/nros_config_generated.h` | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-qos-profile-ssot.py` · rule6-outside-traits | `packages/api/nros/src/lib.rs` += pub const QOS_PROFILE_RERUN: QoSProfile = QoSProfile { history: QoSHis | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-retired-cmake-keywords.py` · rust-template | `packages/cli/nros-cli-core/src/builder/cmake_root.rs` += const RERUN_CMAKE: &str = "nano_ros_entry(app HOST native)\n"; | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-set-e-bare-assignment.py` · scripts-bin-cargo | `scripts/bin/cargo` += _rerun() { | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-zenoh-feature-off-compile.py` · zephyr-CMakeLists | `zephyr/CMakeLists.txt` += zephyr_compile_definitions(Z_FEATURE_LINK_UDP_UNICAST=0 Z_FEATURE_LINK | 0 | 1 | new audit |
| W5 | `check-zenoh-platform-macros.py` | `zephyr/cmake/nros_rmw_zenoh.cmake`: zephyr_compile_definitions(ZENOH_ZEPHYR ZENOH_LINUX) | 0 | 1 | recorded 2026-09-28 |
| W5 | `check-zephyr-workspace-resolvers.py` | `zephyr/CMakeLists.txt` += set(_rerun_ws "${CMAKE_SOURCE_DIR}/../nano-ros-workspace") | 0 | 1 | recorded 2026-09-28 |

## Direction

Move each population onto `scripts/lib/file_kinds.py` (or the KIND the rule is about) and add the normal-path reach control the W5 members got. `check-no-allow-multiple-def` must NOT be widened in isolation: it turns red on `main` (issue 1618).

Per CLAUDE.md "Fix the CLASS": move each gate onto the class's shared helper,
add the negative control its row names, and re-run the row's mutation to show
it now fails. Phase-472's acceptance ("no confirmed hole in any class") stays
unmet until this list is empty.

## Resolution (2026-10-02)

Every row's population is now the KIND the rule is about (`scripts/lib/file_kinds.py`)
or a harvested set. All 18 recorded mutations fail (rc 0 → 1), as do all 18
controls. `no-allow-multiple-def` was closed by issue 1618 (#1559).

| gate | population now |
| --- | --- |
| `cmake-image-policy` | `file_kinds cmake`. A Rust-runtime carrier (`nros_declare_rust_runtime_carrier`) is an image too. NuttX's `nros_platform_link_app` is a REQUIRED seam, with its reason (images there bypass `nano_ros_entry`), and a stale row fails. |
| `board-name-reach` | plus every descriptor ALIAS (`names = [...]`), judged by its overlay's reach. A platform selector (`nuttx`, `threadx-riscv64`) is not a machine claim. **3 pre-existing alias violations** (`esp32-qemu`, `esp32c3`, `threadx-qemu-riscv64`) entered the ratchet baseline: they are compatibility ids, so renaming them is phase-437's call. |
| `ci-doc-workflow-refs` | the `.github/workflows/x.yml` spelling, across every live doc (development/reference/design/book). This fixed one dead citation in `versioning.md`. |
| `c-knob-guard-order` | the `c-family` kind (`.hpp` included). A header's OWN later `#define` no longer counts as "defined by a header". |
| `build-tool-verbs-exempt` | `file_kinds cmake` (zephyr included). A GUARDED top-level verb that keeps the workspace check fails unless rowed (`plan`, run in the user's workspace). The top-verb mutation is re-expressed with a guarded verb (`sync`); an unguarded `rerun-verb` is correctly fine (control row passes). |
| `dist-floors` | every index table with a `dist` map. `[rust.rustup]`'s 3 floorless rows became baseline debt (ceiling 0 → 3), so a fourth host fails. |
| `feature-gated-modules` | gated `mod` declarations in every `.rs` of the crate, not just `lib.rs`. 5 more features are now compiled alone, all clean. |
| `cross-toolchain-provenance` | every CMake file that selects a compiler, not only `cmake/toolchain/`. |
| `generated-leaf-regenerable` | every tracked `Cargo.toml`. |
| `message-crate-identity` | plus `[workspace.dependencies]`. |
| `orphan-generated-stamp` | the shared cargo dirs below `build/` and `target/`: a bounded-depth glob of untracked build output (0.8 s). |
| `qos-profile-ssot` rule 6 | every Rust source. An alias of a fenced preset is allowed (`nros` re-exports the rclrs names that way). |
| `set-e-bare-assignment` | plus shebang-shell files (`file_kinds.shebang_shell`: `scripts/bin/cargo`, `.githooks/pre-push`). This caught one new violation in this PR's own edit, which was fixed. |
| `retired-cmake-keywords` | plus the CMake the CLI EMITS (Rust string literals and `.jinja` under `packages/cli`). |
| `zenoh-platform-macros` | plus every CMake producer, attributed to a port by path. |
| `zephyr-workspace-resolvers` | `CMakeLists.txt` is code, not `.txt` prose. |
| `zenoh-feature-off-compile` | every CMake file of the Zephyr module. |

The harness used for these re-runs now sources the worktree's `activate.sh`
before each gate. The calling shell carried another checkout's
`NROS_REPO_DIR`, and `check-feature-gated-modules` honours that variable, so
its control was reading the wrong tree.
