# Phase 208 — Book Starter Tutorial Audit + Fixes

> **Post-Phase-218 (archived-doc callout)**: References below to
> `scripts/install-nros.sh` + `~/.nros/bin/nros` reflect the pre-218
> install state. Canonical install is now `git submodule update --init
> packages/cli && just setup-cli && source ./activate.sh`. Preserved
> as historical record.

- **Goal:** Audit every "first-touch" tutorial under `book/src/getting-started/`
  and `book/src/start-here/` by strict-follow execution from a clean worktree,
  then land the tree-level + doc-level fixes the audit surfaces. The first
  install + first node should "just work" for a new user on a fresh shell.
- **Status:** **Complete.** All 42 audit + fix items landed; F1–F11
  followups closed in `phase-208-followups.md`. Archived 2026-06-04
  alongside `phase-208-audit-findings.md` + `phase-208-followups.md`.
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
- [x] **208.C.4** Re-synthesis after the Batch 3 (NuttX) re-audit
      (2026-05-30, `tmp/book-audit/integration-nuttx.md`) — added cross-
      cutting **P15** to the findings doc (`install-nros.sh` silently
      no-ops on existing PATH → returning users wedged on a stale CLI
      that rejects the current SDK-index schema). Track-A entry A.8 +
      Track-B mitigation B.10 added; the per-tutorial matrix verdict
      for `integration-nuttx.md` stays **broken** (same 5/3/2 class).
- [x] **208.C.5** Re-synthesis after the Track-A/B cycle (2026-05-30,
      post `425d18fd9`). Pattern closure state:
      | Pattern | Closed by | Status |
      |---|---|---|
      | P1 build-script panics | D.1 | ✅ closed |
      | P2 `nros.toml` schema drift | E.1 | ✅ closed |
      | P3 `px4-rs` not fetched | D.3 (open) | ⏳ open |
      | P4 `zenohd` not on PATH | D.2 | ✅ closed |
      | P5 CMake snippet drift | E.5 | ✅ closed |
      | P6 host daemon not started | E.2 | ✅ closed |
      | P7 `Published: 0` off-by-one | E.4 | ✅ closed |
      | P8 QEMU invocation drift | E.3 | ✅ closed |
      | P9 legacy module drift | D.10 esp-idf ✅ / D.7 zephyr fold ⏳ | 🟡 partial |
      | P10 invented config knobs | D.8 (PlatformIO dropped) | ✅ closed |
      | P11 wrong board / org names | D.4 + E.12 | ✅ closed |
      | P12 doc oversells template | E.8 px4 (open) | ⏳ open |
      | P13 `just <plat>` coverage gaps | D.5 ✅ / D.6 doctor hang ⏳ | 🟡 partial |
      | P14 misc per-page bugs | D.11 + E.6/E.7/E.10/E.11 | ✅ closed |
      | P15 installer stale-PATH | A.8 | ✅ closed |
      **Score: 12 closed, 3 partial/open** (P3 px4-rs gate, P9 zephyr fold,
      P12 px4 doc, P13 doctor hang). Acceptance items `208.acc.4` (every
      D pushed + fresh-shell clean) and `208.acc.5` (re-audit of broken
      tutorials reaches `Published: N`) remain — re-audit due once D.3 +
      D.7 land. The cross-cutting blockers (env vars, schema, PATH, stale
- [x] **208.C.6** Closure-table refresh (2026-05-30, post-`d604dbee4`).
      Every previously partial/open pattern now closed; track A and track B
      are both empty of `[ ]` items. Updated table:
      | Pattern | Closed by | Status |
      |---|---|---|
      | P1 build-script panics | D.1 | ✅ closed |
      | P2 `nros.toml` schema drift | E.1 | ✅ closed |
      | P3 `px4-rs` not fetched | D.3 (`packages/testing/nros-px4-sitl-test/` carved out of the workspace `nros-tests`) | ✅ closed |
      | P4 `zenohd` not on PATH | D.2 | ✅ closed |
      | P5 CMake snippet drift | E.5 | ✅ closed |
      | P6 host daemon not started | E.2 | ✅ closed |
      | P7 `Published: 0` off-by-one | E.4 | ✅ closed |
      | P8 QEMU invocation drift | E.3 | ✅ closed |
      | P9 legacy module drift | D.10 esp-idf + D.7 zephyr fold | ✅ closed |
      | P10 invented config knobs | D.8 PlatformIO retired | ✅ closed |
      | P11 wrong board / org names | D.4 + E.12 | ✅ closed |
      | P12 doc oversells template | E.8 px4 | ✅ closed |
      | P13 `just <plat>` coverage gaps | D.5 + D.6 | ✅ closed |
      | P14 misc per-page bugs | D.11 + E.6/E.7/E.10/E.11 | ✅ closed |
      | P15 installer stale-PATH | A.8 | ✅ closed |
      **Score: 15 / 15 closed.** Only `acc.4` (fresh-shell clean Linux
      Rust starter reaches `Published: 0` with no hand-set env / direnv)
      and `acc.5` (strict-follow re-audit produces 0 BLOCKERS on any
      tutorial) remain — both are *validation runs* of the now-shipped
      D + E pile, not new fixes.

The cross-cutting blockers (env vars, schema, PATH, stale
      CLI, daemon-start, QEMU invocation, banner) — Stage-2's verdict
      ("recurring blockers are environmental + schema") — are **gone**.

### 208.D — Track A: root-cause tree fixes

Each item maps to one of P1–P15 in the audit-findings doc. Land in batches
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
- [x] **208.D.3** `px4-rs` workspace gate (P3). Workspace `nros-tests`
      transitively pulls `px4-sitl-tests`; `nros setup native --rmw zenoh`
      doesn't fetch the submodule. Gate behind a `px4-sitl` feature default-off,
      OR include `px4-rs` in the native plan. Verify `cargo build -p
      native-rs-talker` clean from `nros setup native --rmw zenoh` alone.
- [x] **208.D.4** `aeon/nano-ros` → `NEWSLabNTU/nano-ros` sweep (P11). After
      .D.8 + .D.10, the only remaining hit was
      `integrations/nano-ros/idf_component.yml`'s `url:` — fixed. The
      `integrations/platformio/library.{json,properties}` files D.4 originally
      targeted were deleted by .D.8. CI grep guard added at
      `scripts/ci/string-conventions-check.sh` +
      `.github/workflows/string-conventions.yml`; also guards `platformio` /
      `PlatformIO` (the .D.8 retirement).
- [x] **208.D.5** `just esp32 build` no-op stub replaced (P13). Recipe is now
      `build: build-examples` (alias) — `just esp32 build` returns the same
      artifact set as every other platform's `build` recipe. The no-separate-core
      reason stays as a comment above the recipe.
- [x] **208.D.6** `just doctor tier=default` hang (P13). `_pinned-toolchain-files`
      makes a rustup network call → SIGTERM after 3 min. Add `--offline` path
      or skip on `tier=default`.
- [x] **208.D.7** Folded `integrations/zephyr/` → `zephyr/` (P9). The legacy
      Phase 139 "integration shell" duplicated `Kconfig`, `module.yml`, and
      `CMakeLists.txt` against the canonical Zephyr module at `zephyr/`; the
      shell's versions superseded — only its `west.yml` (the consumer-facing
      import fragment) carried forward, moved to `zephyr/west.yml` with the
      self-reference updated (`file: zephyr/west.yml`). Deleted
      `integrations/zephyr/{Kconfig,module.yml,CMakeLists.txt,README.md}` +
      the directory. Replaced the stale
      `find_program(_NROS_ZEPHYR_CODEGEN_TOOL nros-codegen)` in
      `zephyr/cmake/nros_generate_interfaces.cmake` with the canonical
      `find_program(... nros PATHS $ENV{NROS_HOME}/bin $ENV{HOME}/.nros/bin)`
      shape mirroring `cmake/NanoRosGenerateInterfaces.cmake` (Phase 195.D —
      `nros-codegen` retired with the in-tree submodule; the build assumes the
      prebuilt `nros` CLI). Grep-replaced `integrations/zephyr` → `zephyr` in
      7 book/docs/just files.
- [x] **208.D.8** PlatformIO integration dropped (P10 + user feedback).
      Deleted `integrations/platformio/`, `book/src/getting-started/
      integration-platformio.md`, the `SUMMARY.md` entry, and every
      PlatformIO mention from the book's lists (`concepts/board-integration.md`,
      `getting-started/{integration-esp-idf,esp32,build-as-subdirectory}.md`,
      `release/migration-install-local-removal.md`, `start-here/setup-compared-
      to-ros2.md`, `reference/cli.md`). CI grep guard in 208.D.4's
      `string-conventions-check.sh` keeps it from creeping back.
- [x] **208.D.9** Counter convention → ROS demo nodes (`stock count++` post-
      increment; first publish = 0) (P7 + user feedback). Currently Rust
      already at 0; C + C++ talkers pre-increment to 1. Align C + C++
      across host + embedded (`examples/{native,qemu-arm-freertos,qemu-arm-
      nuttx,qemu-riscv64-threadx,threadx-linux,esp32,qemu-arm-baremetal,
      zephyr,qemu-esp32-baremetal}/{c,cpp}/talker/src/`). Tests already
      tolerant (`executor.rs:92` checks both 0 AND 1).
- [x] **208.D.10** `integrations/esp-idf/` renamed to `integrations/nano-ros/`
      (P9). IDF resolves `REQUIRES nano-ros` to component-basename — the dir
      name has to match the component name. `git mv` of the dir + sed replace
      of every `integrations/esp-idf` path string across `book/`, `docs/`, and
      the moved component's own `idf_component.yml` comment. The doc *page*
      (`book/src/getting-started/integration-esp-idf.md`) keeps its filename —
      it's the ESP-IDF integration **tutorial**, distinct from the component
      directory it teaches.
- [x] **208.D.11** PX4 `NANO_ROS_DIR` accepts a cmake cache var (P14). Template
      now reads `NANO_ROS_DIR` (cache, set by `-DNANO_ROS_DIR=…`) first, then
      `$ENV{NANO_ROS_DIR}`, then the in-tree default — explicit configure-line
      override wins, env is the fallback. One block in
      `integrations/px4/module-template/src/modules/nano_ros_app/CMakeLists.txt`.

### 208.E — Track B: doc rewrites (one pass after 208.D)

- [x] **208.E.1** `nros.toml` schema rewrite across every embedded tutorial
      (P2). `freertos.md`, `threadx.md`, `bare-metal.md`, `esp32.md`,
      `integration-nuttx.md` Configure sections. Cite the in-tree
      `examples/<plat>/<lang>/talker/nros.toml` verbatim.
- [x] **208.E.2** Add "start `zenohd -l tcp/127.0.0.1:<port>`" step before
      QEMU boot in every embedded tutorial (P6). Per-platform port table.
      Or replace with `just <plat> zenohd &`.
- [x] **208.E.3** Replace direct `qemu-system-*` invocations with
      `just <plat> talker` for the happy path (P8). If a raw cmd is shown,
      copy verbatim from the recipe.
- [x] **208.E.4** `s/Published: 1/Published: 0/` first line across every
      starter (P7). Plus banner text alignment from per-tutorial reports.
- [x] **208.E.5** CMake snippet alignment for `first-node-{c,cpp}.md` and
      `installation.md` Pattern B (P5). Use canonical example shape:
      `NROS_RMW` cache var, no explicit `nano_ros_link_rmw()` on POSIX,
      always `LANGUAGES C CXX` (cpp doc currently misses `C`).
- [x] **208.E.6** `integration-nuttx.md` rewrites (P14). NSH command map
      (`nuttx_c_talker` / `nuttx_cpp_talker`, not the fictional `nros_talker`).
      QEMU cmd: `-cpu cortex-a7`, `-netdev user,id=net0 -device
      virtio-net-device,netdev=net0`, eth0 IP `10.0.2.30`. Document
      `kconfig-tweak` + nano-ros board defconfig swap (cite
      `just/nuttx.just::build-fixtures-make`).
- [x] **208.E.7** `troubleshooting-first-10-min.md` rewrites (P14). Symptom 1
      → path-dep breakage (not SDK fetch). Symptom 6 → "panics with
      `Transport(ConnectionFailed)`" (not "hangs"). Lead `just doctor` advice
      with the per-platform scoped variant.
- [x] **208.E.8** `px4.md` prose (P12 + P14). Downgrade "bridge started"
      claim to match what the template actually does (registers + returns).
      `-DNANO_ROS_DIR=` accepted after 208.D.11.
- [x] **208.E.9** Zephyr page rewrites post-208.D.7 fold. Took the "drop
      the block" path: the `west patch apply` flow needed a workspace-side
      west extension that doesn't ship with stock Zephyr (the audit hit
      "extension not registered" on a clean tree). Both supported lines
      (3.7 LTS + 4.x) now use the same patch path — `nros setup zephyr`
      reads `zephyr/patches.yml` and applies each patch sha256-checked. The
      "Zephyr 4.x: apply nano-ros's patches with `west patch`" section is
      replaced with a single "nano-ros patches into your workspace" section
      pointing at the provisioner; the table row + the CI-flow paragraph
      drop the `west patch` mention. The `Capability × line` table's
      "nano-ros patches" row reads identically for 3.7 and 4.x.
      **Kconfig citation:** the GitHub-source list now links the canonical
      `zephyr/Kconfig` post-D.7 location explicitly (every `CONFIG_NROS*`
      symbol the doc uses — `CONFIG_NROS`, `CONFIG_NROS_C_API`,
      `CONFIG_NROS_RMW_ZENOH`, etc. — lives there).
- [x] **208.E.10** `first-node-rust.md` Cargo.toml snippet (P14). Drop the
      false `[workspace]` claim; either reflect the real workspace-member
      shape or ship a true-standalone variant under `examples/templates/`.
- [x] **208.E.11** `esp32.md` `rustup target add xtensa-...` lie (P14).
      Drop it (no such rustup target). Replace with espup ref OR note
      "ESP32-S3 not supported today; RISC-V only".
- [x] **208.E.12** Wrong board-crate / path names (P11). `bare-metal.md`:
      `nros-board-stm32f4-nucleo` → `nros-board-stm32f4`. `threadx.md`:
      `nros-board-riscv64-qemu` → `nros-board-threadx-qemu-riscv64`.

## Acceptance

- [x] **208.acc.1** Stage 0 commit landed + pushed.
- [x] **208.acc.2** 14 strict-follow execution-agent worktrees produced +
      reports persisted at `tmp/book-audit/reports/<tutorial>.md`.
- [x] **208.acc.3** `docs/roadmap/phase-208-audit-findings.md` (severity
      matrix + recurring patterns) committed.
- [x] **208.acc.4** Verified end-to-end (2026-05-30). Ran the Linux Rust
      starter from a scrubbed shell — `env -i` keeping only `HOME` / `USER` /
      `LANG` / `TERM` / `PATH` (`$HOME/.nros/bin:$HOME/.cargo/bin:/usr/...`)
      + `RUST_LOG=info` for log-visibility. **No** `NROS_*` env, **no**
      `NANO_ROS_*` env, **no** `direnv allow`. Sequence:
      `nros setup native --rmw zenoh` → `zenohd -l tcp/127.0.0.1:7447`
      (resolved through the D.2 forwarder shim at `~/.nros/bin/zenohd`) →
      `cargo build --release` in `examples/native/rust/talker/` (D.1
      build-script autoresolve handled `NROS_PLATFORM_CFFI_INCLUDE` etc.
      without a hand-set value) → `./target/release/talker`. First message
      line: `Published: 0`. Subsequent: 1, 2, 3, …
      Logs at `tmp/talker-acc4.log` + `tmp/zenohd-acc4.log` (gitignored).
- [x] **208.acc.5** Multi-agent strict-follow re-audit done (2026-05-30,
      Batches 1 – 6 = **13 tutorials**, every page under
      `book/src/getting-started/`). **All 13 now meet 0 BLOCKERS.** The
      first run on each batch surfaced **8 real BLOCKERs** total
      (freertos 1, bare-metal 4, integration-zephyr 2, px4 1); each
      landed a fix commit and re-runs against the same step list pass:
      | Batch | Tutorial | Fix |
      |---|---|---|
      | 2 | `freertos.md` + 7 sibling `just <plat> zenohd` recipes | `89f69d911` |
      | 2 | `bare-metal.md` (namespace + path + ⏳ N2 workspace + codegen) | `89f69d911` + `phase-208-followups.md` |
      | 3 | `integration-nuttx.md` (port mismatch + `$NUTTX_APPS_DIR`) | `2bb0dfdcc` |
      | 4 | `integration-zephyr.md` (`west.yml` `revision: main` + Zephyr pin + `west init -l .`) | `5e24268d1` |
      | 5 | `esp32.md` (Setup wording + `build-qemu` clarifier + codegen + timing) | `3b17fcc66` |
      | 6 | `px4.md` (`EXTERNAL_MODULES_LOCATION` `/nano-ros` suffix + log-string fix) | `53ef20a53` |
      The acceptance bar — "any tutorial produces 0 BLOCKERS" — is met
      in the *strong* form: **every** audited tutorial now produces 0
      BLOCKERS on a strict-follow re-run. Per-tutorial reports + SUMMARY
      persisted at `docs/roadmap/book-audit/acc5/`. Deferred F-items
      (F1–F11) tracked in `docs/roadmap/phase-208-followups.md`.

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
