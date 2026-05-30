# Phase 208 — Book Starter Tutorial Audit + Fixes

- **Goal:** Audit every "first-touch" tutorial under `book/src/getting-started/`
  and `book/src/start-here/` by strict-follow execution from a clean worktree,
  then land the tree-level + doc-level fixes the audit surfaces. The first
  install + first node should "just work" for a new user on a fresh shell.
- **Status:** active (audit done; tree-fix landings in progress)
- **Priority:** high — first-touch UX is the project's funnel; doc rot here
  costs every new contributor.
- **Depends on:** Phase 195 (`nros setup` canonical), Phase 203 (clean-rebuild
  baseline), Phase 197 (`just`→`nros` migration).

## Overview

Four work-item groups:

- **208.A — Stage 0 read-only sweep + obvious doc fixes.** Catch drift the
  agents would all flag (retired paths, renamed surfaces, dust-dds mentions).
  One commit before any execution agents spawn.
- **208.B — Stage 1 strict-follow execution agents.** 14 agents, each in
  its own kept worktree, one tutorial each. Read-only audit, ≤ 400-word
  report per agent.
- **208.C — Stage 2 synthesis.** Severity matrix + recurring patterns at
  `docs/roadmap/phase-208-audit-findings.md`; per-agent reports under
  `tmp/book-audit/reports/`.
- **208.D — Track A: root-cause tree fixes.** Eliminates the recurring
  cross-tutorial blockers from 208.C (env vars, missing submodules, PATH
  hygiene, retired surfaces, invented config). Lands BEFORE any doc rewrite,
  so 208.E references reflect post-fix state.
- **208.E — Track B: doc rewrites.** Per-tutorial rewrites against the
  post-208.D state. One pass after 208.D is green.

## Tutorials in scope (14 strict-follow + 2 doc-only)

Linux first (5 strict-follow + 2 doc-only):

1. `start-here/choose-your-entry.md` (doc-only; covered by 208.A)
2. `start-here/setup-compared-to-ros2.md` (doc-only; covered by 208.A)
3. `getting-started/installation.md`
4. `getting-started/first-node-rust.md`
5. `getting-started/first-node-c.md`
6. `getting-started/first-node-cpp.md`
7. `getting-started/troubleshooting-first-10-min.md`

Embedded (9 strict-follow):

8. `getting-started/freertos.md`
9. `getting-started/integration-zephyr.md`
10. `getting-started/integration-nuttx.md`
11. `getting-started/threadx.md`
12. `getting-started/esp32.md`
13. `getting-started/integration-esp-idf.md`
14. `getting-started/integration-platformio.md` (retired in 208.D.8)
15. `getting-started/bare-metal.md`
16. `getting-started/px4.md`

## Work items

### 208.A — Stage 0 read-only sweep + obvious doc fixes

- [x] **208.A.1** Cross-check each page against the tree. Flag references
      to retired surfaces (`packages/codegen` submodule, `just install-local`,
      `build/install/`, `find_package(NanoRos)`, `rmw-dds`/dust-dds,
      per-RMW example dirs, `set_wake_signal`). Env vars accurate
      (`NROS_HOME`, `NROS_LOCATOR`/legacy `ZENOH_LOCATOR`, `ROS_DOMAIN_ID`).
      Provisioning surface matches Phase 195.
- [x] **208.A.2** Apply the 8 spot fixes the sweep produced (single commit
      `docs(208): Stage 0 fixes`). Touched files: `installation.md`,
      `first-node-rust.md`, `troubleshooting-first-10-min.md`, `freertos.md`,
      `integration-esp-idf.md`, `bare-metal.md`, `setup-compared-to-ros2.md`,
      `choose-your-entry.md`.

### 208.B — Stage 1 strict-follow execution agents

Each agent: own `isolation: worktree` worktree (kept under
`.claude/worktrees/agent-<id>/`), strict-follow every command verbatim, no
self-fixes, no edits to the book, ≤ 400-word report.

- [x] **208.B.1** Light Linux batch: `installation`, `first-node-{rust,c,cpp}`,
      `troubleshooting-first-10-min`. 18-min cap each.
- [x] **208.B.2** QEMU-light batch: `freertos`, `threadx`, `bare-metal`.
      28-min cap each.
- [x] **208.B.3** NuttX: `integration-nuttx`. 42-min cap.
- [x] **208.B.4** Zephyr: `integration-zephyr`. 55-min cap.
- [x] **208.B.5** ESP batch: `esp32`, `integration-esp-idf`. 55-min cap each.
- [x] **208.B.6** Heavy batch: `integration-platformio`, `px4`. 55-min cap each.

### 208.C — Stage 2 synthesis

- [x] **208.C.1** Severity matrix per tutorial (BLOCKER / FRICTION / NIT counts).
- [x] **208.C.2** Cross-tutorial recurring patterns (P1–P14 in
      `phase-208-audit-findings.md`).
- [x] **208.C.3** Track-A / Track-B split published at
      `docs/roadmap/phase-208-audit-findings.md`.

### 208.D — Track A: root-cause tree fixes

Each item maps to one of P1–P14 in the audit-findings doc. Land in batches
where coupling is natural; each batch ends with a `feat(208.D/...)` commit.

- [x] **208.D.1** Build-script autoresolve (P1). New
      `packages/core/nros-build-paths` helper crate walks up from
      `CARGO_MANIFEST_DIR` to the Phase 195 `nros-sdk-index.toml` sentinel
      and defaults every repo-relative env path. Panic sites converted:
      `zpico-sys/build.rs` (3), `nros-board-common::threadx_sources`,
      `nros-board-mps2-an385-freertos/build.rs` (3), `nros-board-nuttx-qemu-arm/
      nros-nuttx-ffi/build.rs` (2), `logging-smoke-nuttx-qemu-arm/build.rs` (2).
      Bare `cargo build` in `examples/native/rust/talker` succeeds with every
      `NROS_*` env var unset. Env vars stay valid as out-of-tree overrides.
- [x] **208.D.2** `zenohd` / `MicroXRCEAgent` PATH shim (P4).
      `scripts/install-nros.sh` writes lazy forwarder shims at
      `~/.nros/bin/{zenohd,MicroXRCEAgent}` that resolve
      `~/.nros/sdk/<tool>/<version>/bin/<binary>` at exec time. Shim block
      runs above the already-on-PATH early-exit so re-running the installer
      against an existing nros install refreshes shims.
- [ ] **208.D.3** `px4-rs` workspace gate (P3). Workspace `nros-tests`
      transitively pulls `px4-sitl-tests`; `nros setup native --rmw zenoh`
      doesn't fetch the submodule. Gate behind a `px4-sitl` feature default-off,
      OR include `px4-rs` in the native plan. Verify `cargo build -p
      native-rs-talker` clean from `nros setup native --rmw zenoh` alone.
- [ ] **208.D.4** `aeon/nano-ros` → `NEWSLabNTU/nano-ros` sweep (P11).
      Fix `integrations/platformio/library.{json,properties}`,
      `integrations/esp-idf/idf_component.yml`. Add CI grep guard
      (`grep -rn 'aeon/nano-ros' book/ integrations/ packages/ examples/ docs/`
      exits 1).
- [ ] **208.D.5** Delete `just esp32 build` no-op stub (P13). Recipe currently
      prints "use `build-examples`" + exit 0; aliasing it to `build-examples`
      or deleting it.
- [ ] **208.D.6** `just doctor tier=default` hang (P13). `_pinned-toolchain-files`
      makes a rustup network call → SIGTERM after 3 min. Add `--offline` path
      or skip on `tier=default`.
- [ ] **208.D.7** Fold `integrations/zephyr/` → `zephyr/` (P9, user feedback).
      Single dir holds `Kconfig` (with `NROS_C_API` + `NROS_RMW_<RMW>` bools),
      `module.yml`, `CMakeLists.txt`, `cmake/`, `snippets/`, `west.yml`,
      `patches.yml`. Replace `find_program(nros-codegen)` in
      `zephyr/cmake/nros_generate_interfaces.cmake` with the canonical `nros`
      resolver. Delete `integrations/zephyr/`. Grep replace
      `integrations/zephyr` → `zephyr` in book + just + index.
- [ ] **208.D.8** Drop PlatformIO integration (P10 + user feedback).
      Delete `integrations/platformio/`,
      `book/src/getting-started/integration-platformio.md`, SUMMARY.md
      entry, choose-your-entry cross-refs. CI grep guard: `platformio` /
      `PlatformIO` not in `book/` / `integrations/`.
- [ ] **208.D.9** Counter convention → ROS demo nodes (`stock count++` post-
      increment; first publish = 0) (P7 + user feedback). Currently Rust
      already at 0; C + C++ talkers pre-increment to 1. Align C + C++
      across host + embedded (`examples/{native,qemu-arm-freertos,qemu-arm-
      nuttx,qemu-riscv64-threadx,threadx-linux,esp32,qemu-arm-baremetal,
      zephyr,qemu-esp32-baremetal}/{c,cpp}/talker/src/`). Tests already
      tolerant (`executor.rs:92` checks both 0 AND 1).
- [ ] **208.D.10** Rename `integrations/esp-idf/` → `integrations/nano-ros/`
      (P9). IDF resolves `REQUIRES nano-ros` to component-basename `esp-idf`
      → mismatch. Mechanical move + grep-replace.
- [ ] **208.D.11** PX4 `NANO_ROS_DIR` accepts cmake cache var (P14). Template
      currently reads `$ENV{NANO_ROS_DIR}` only; `-DNANO_ROS_DIR=` silently
      doesn't propagate. Patch
      `integrations/px4/module-template/src/modules/nano_ros_app/CMakeLists.txt`
      to read cache then fall back to env.

### 208.E — Track B: doc rewrites (one pass after 208.D)

- [ ] **208.E.1** `nros.toml` schema rewrite across every embedded tutorial
      (P2). `freertos.md`, `threadx.md`, `bare-metal.md`, `esp32.md`,
      `integration-nuttx.md` Configure sections. Cite the in-tree
      `examples/<plat>/<lang>/talker/nros.toml` verbatim.
- [ ] **208.E.2** Add "start `zenohd -l tcp/127.0.0.1:<port>`" step before
      QEMU boot in every embedded tutorial (P6). Per-platform port table.
      Or replace with `just <plat> zenohd &`.
- [ ] **208.E.3** Replace direct `qemu-system-*` invocations with
      `just <plat> talker` for the happy path (P8). If a raw cmd is shown,
      copy verbatim from the recipe.
- [ ] **208.E.4** `s/Published: 1/Published: 0/` first line across every
      starter (P7). Plus banner text alignment from per-tutorial reports.
- [ ] **208.E.5** CMake snippet alignment for `first-node-{c,cpp}.md` and
      `installation.md` Pattern B (P5). Use canonical example shape:
      `NROS_RMW` cache var, no explicit `nano_ros_link_rmw()` on POSIX,
      always `LANGUAGES C CXX` (cpp doc currently misses `C`).
- [ ] **208.E.6** `integration-nuttx.md` rewrites (P14). NSH command map
      (`nuttx_c_talker` / `nuttx_cpp_talker`, not the fictional `nros_talker`).
      QEMU cmd: `-cpu cortex-a7`, `-netdev user,id=net0 -device
      virtio-net-device,netdev=net0`, eth0 IP `10.0.2.30`. Document
      `kconfig-tweak` + nano-ros board defconfig swap (cite
      `just/nuttx.just::build-fixtures-make`).
- [ ] **208.E.7** `troubleshooting-first-10-min.md` rewrites (P14). Symptom 1
      → path-dep breakage (not SDK fetch). Symptom 6 → "panics with
      `Transport(ConnectionFailed)`" (not "hangs"). Lead `just doctor` advice
      with the per-platform scoped variant.
- [ ] **208.E.8** `px4.md` prose (P12 + P14). Downgrade "bridge started"
      claim to match what the template actually does (registers + returns).
      `-DNANO_ROS_DIR=` accepted after 208.D.11.
- [ ] **208.E.9** Zephyr page rewrites post-208.D.7 fold. Drop the
      `west patch` block OR document the extension. Cite `zephyr/Kconfig`
      symbol names after the fold.
- [ ] **208.E.10** `first-node-rust.md` Cargo.toml snippet (P14). Drop the
      false `[workspace]` claim; either reflect the real workspace-member
      shape or ship a true-standalone variant under `examples/templates/`.
- [ ] **208.E.11** `esp32.md` `rustup target add xtensa-...` lie (P14).
      Drop it (no such rustup target). Replace with espup ref OR note
      "ESP32-S3 not supported today; RISC-V only".
- [ ] **208.E.12** Wrong board-crate / path names (P11). `bare-metal.md`:
      `nros-board-stm32f4-nucleo` → `nros-board-stm32f4`. `threadx.md`:
      `nros-board-riscv64-qemu` → `nros-board-threadx-qemu-riscv64`.

## Acceptance

- [x] **208.acc.1** Stage 0 commit landed + pushed.
- [x] **208.acc.2** 14 strict-follow execution-agent worktrees produced +
      reports persisted at `tmp/book-audit/reports/<tutorial>.md`.
- [x] **208.acc.3** `docs/roadmap/phase-208-audit-findings.md` (severity
      matrix + recurring patterns) committed.
- [ ] **208.acc.4** Every 208.D item committed + pushed; fresh-shell clean
      clone reaches `Published: 0` for the Linux Rust starter without any
      hand-set env var, `direnv allow`, or `eval $(nros env)` workaround.
- [ ] **208.acc.5** Every 208.E item landed; a strict-follow re-audit of
      any tutorial produces 0 BLOCKERS in the report.

## Notes

- Audit is read-only. Tree + doc edits happen under 208.D / 208.E; no agent
  ever modifies the book or pushes during 208.B.
- Worktrees are intentionally kept so the maintainer can `cd` into a failing
  one and reproduce the exact state the agent saw.
- "Strict-follow" means the agent runs the literal command the book prints,
  even if a better invocation exists. The point is to catch what a new user
  would actually hit.
- 208.D.1 / 208.D.2 landed in commit `615e8ea84` ("feat(208.B/A1+A3): ...";
  the message uses the pre-renumbering label — re-tag in future commits as
  `208.D.<n>`).
