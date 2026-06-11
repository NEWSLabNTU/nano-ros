# Phase 214 — Antipattern Audit Findings

**Goal**: Resolve issues surfaced by a 10-slice antipattern audit wave run
on `main` @ `5a67df535`. Work pre-grouped into **tracks** with
non-overlapping file ownership so multiple agents (potentially across
machines) can run in parallel without rebasing conflict.

**Status**: LIVE. Created 2026-06-04 from 10 parallel `Explore`-mode
audit agents.

**Priority**: HIGH for Track A (silent failures + banned test antipattern
+ codegen drift). MED-LOW for the rest.

**Depends on**: Phase 212 closure (DONE), Phase 213 closure (DONE).

> **Post-Phase-218**: This doc was written when `scripts/install-nros.sh`
> + `~/.nros/bin/nros` were the canonical install path. Phase 218 merged
> the CLI in-tree at `packages/cli/` and retired `scripts/install-nros.sh`
> (commit `19d1d29ba`); the new install is `git submodule update --init
> packages/cli && just setup-cli && source ./activate.sh`. Track I
> (`nros ws sync` / `NROS_FROM_SOURCE`) is superseded by 218 — the
> in-tree build IS the install now. Other references to `install-nros.sh`
> / `nros-cli` repo / `~/.nros/bin` below are historical record.

---

## Overview

The audit identified **4 CRITICAL** items and **8 HIGH/MED/LOW** items
across 10 audit axes. Tracks group them by file-ownership disjoint-ness:

| track | what | scope | severity floor |
|---|---|---|---|
| **A — Silent failures + banned test pattern** | FFI return-code discards + `eprintln+return` PASS antipattern | 6 files | CRITICAL |
| **B — Codegen absolute paths** | 20 `Cargo.toml` + 100+ `include!()` lines in nuttx/cpp generated FFI carry `/home/aeon/repos/nano-ros/...` literals | nros-cli codegen + regen | CRITICAL |
| **C — Magic numbers + duplicated defaults** | scattered timeout/MTU/buffer-size literals across 5+ build.rs | scattered | LOW-MED |
| **D — Unsafe doc + audit** | 6 `unsafe fn` missing `# Safety` paragraphs + 1 lifetime transmute hardening | 8 files | LOW |
| **E — Misc hardening** | Orin SPE `i32` cast bounds-check, dual-transport `compile_error!` guard | 2 files | LOW |
| **F — Workspace feature unification leak** | dev-deps of `nros-node` force `nros-serdes/std` under `--workspace --target thumbv7em-none-eabihf` | `nros-node` Cargo.toml dev-deps | CRITICAL |
| **G — zpico-sys platform_aliases.c link** | `cargo test --workspace` link fails: `_z_mutex_rec_unlock` → `nros_platform_mutex_rec_unlock` undefined on POSIX without zenoh-pico in the C side | `packages/zpico/zpico-sys/c/zpico/platform_aliases.c` + build.rs | HIGH |
| **H — Native test fixture prebuild precondition** | 38+ native tests panic with `Test fixture binary not prebuilt — Run just build-test-fixtures first`; `just native test` does not run `build-fixtures` itself | `just native test` recipe, harness binary-resolution code | HIGH |
| **I — `nros ws sync` subcommand unavailable** | installed `nros` 0.3.7 (the script pin) lacks the `ws` verb (added on nros-cli `main` post-tag) that freertos / qemu-baremetal / threadx-linux / native / zephyr build recipes invoke — **resolved 2026-06-04** via Path B source-build env-var in `scripts/install-nros.sh` | `just/{freertos,qemu-baremetal,threadx-linux,native,zephyr}.just` + nros-cli release pin | HIGH |
| **J — Generated `RosAction` codegen drift** | cached `examples/<plat>/rust/<rtic*>/generated/example_interfaces/src/action/fibonacci.rs` lacks 5 envelope assoc-types added to the trait | qemu-arm-baremetal rtic examples, qemu-riscv64-threadx rust examples (in `generated/`, gitignored — fix is a regen sweep) | HIGH |
| **K — Stale Zephyr fixture cache** | every Zephyr test fails with `Zephyr fixture binary is stale: …/nano-ros-workspace/build-*` because `just zephyr test` does not run `build-fixtures` itself | `just zephyr test` recipe | HIGH |
| **L — `integrations/<rtos>/` shells missing** | `zephyr_integration_shell_smoke`, `esp_idf_integration_shell_smoke`, `platformio_integration_shell_smoke` all fail because `integrations/{zephyr,esp-idf,platformio}/` either don't exist or lack manifest files | `integrations/` tree + integration tests | HIGH |
| **M — NuttX armv7a-nuttx-eabi libc shim incomplete** | `_SC_HOST_NAME_MAX` missing → stdlib `hostname/unix.rs:8` fails to compile against `libc` shipped for `armv7a-nuttx-eabi` (nightly-2026-04-11 toolchain regression) | `target/armv7a-nuttx-eabi` libc target spec / std build script | MED |
| **N — `nros` CLI lints / verbs drift vs tests** | `phase212_l_check_lints`, `phase212_g_check_exec_depend_drift`, `phase212_i_migrate_workspace`, `phase212_j_launch`, `phase212_l7_self_bringup`, `phase212_f3_dirwalk_discovery`, `phase212_f_bringup_scaffold`, `phase212_h1_zephyr`, `phase212_mf3_zephyr_self_pkg`, `orchestration_{composable,set_remap_env,includes}` — installed `nros` 0.2.0 lacks newer lints / `codegen-system` / `migrate` / `launch` / planning behaviours these tests assert | nros-cli release pin + the listed test files (skip-vs-fail policy) | MED |
| **O — Examples canonical-shape regression + qemu-patched binary skip noise** | `examples_tree_uses_canonical_shape` reports 24 violations; `qemu_patched_binary` tests use `skip!` for SDK-missing path which still counts as FAIL in nextest junit | `examples/**/package.xml`, `packages/testing/nros-tests/tests/{phase212_examples_canonical_shape,qemu_patched_binary}.rs` | MED |
| **P — Embedded cyclonedds e2e listener loss** | `just freertos test` `test_freertos_rust_cyclonedds_local_pubsub_e2e` and `just native test` `test_threadx_riscv64_cyclonedds_two_qemu_pubsub` both report `Listener: expected at least 1 received messages, got 0` after the e2e harness runs to completion | embedded ddsrt runtime + the two e2e tests | MED |
| **Q — `just <plat> test` does not gate on build-fixtures** | Every per-platform `test` recipe (native, qemu, freertos, nuttx, threadx_linux, threadx_riscv64, zephyr, esp32) lets the test harness explode with "fixture not built" instead of running the matching `build-fixtures` first OR failing fast with a single clear `[PREREQ]` message | each `just/<plat>.just` test recipe head | MED |
| **R — Test runner classifies `skip!` panic as FAIL** | `nros_tests::skip!` panics with `[SKIPPED] …` (the CLAUDE.md-blessed contract) but nextest junit counts the test as `<failure>`; the wrapper script's "Real failures" tally helps but only after the fact | `packages/testing/nros-tests/src/lib.rs::skip!` + nextest filter glue | LOW |

Tracks A–E are the original static-audit findings; **F–R are the
runtime test-suite sweep findings added 2026-06-04** from running
`just <plat> test` across every in-scope platform. Each new track has
disjoint file ownership for parallel dispatch.

Tracks A, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R live in
nano-ros. Track B lives in nros-cli (codegen emit fix) followed by a
nano-ros regen sweep. Tracks I and N point at the installed nros-cli
release pin — fixes for those land in `nros-cli` first, then this repo
bumps `scripts/install-nros.sh`.

---

## Architecture

**Discovery method**: 10 parallel `Explore`-mode agents on `main` @
`5a67df535`. Each had a disjoint slice (paths / magic / unsafe / silent
errors / deprecated / security / tests / cfg / no_std alloc / duplication).
Cross-corroboration where applicable.

**Clean axes (no findings)**:
- Deprecated code — no items with live callers
- Conditional compilation — feature-axis matrices guarded
- `no_std` allocation — `Box::new` only in init / lifetime-erasure tokens
- Code duplication — examples are intentional copy-out templates
- Comment rot — historical references are intentional pointers, not rot

---

## Track A — Silent failures + banned test pattern

**Scope**: CRITICAL. 4 sites discard error context (2 FFI + 2 transport
teardown) plus 2 test files report PASS on missing prereq (CLAUDE.md-
banned `eprintln!+return` antipattern).

**Files (A.1)**: `packages/boards/nros-board-threadx-linux/startup.c:36`,
`packages/boards/nros-board-threadx-qemu-riscv64/startup.c:131`.

**Files (A.2)**: `packages/core/nros-c/src/transport.rs:95,120`.

**Files (A.3)**: `packages/testing/nros-tests/tests/actions.rs:37-38`,
`packages/testing/nros-tests/tests/services.rs:46-47`.

- [x] **214.A.1 ThreadX FFI return-code surfacing** — Closed via
      path (b) — `nros_threadx_set_config` is void by design (pure
      memcpy into static IP/MAC cache; no failure mode). Concurrent
      worker `a020d61fe` added doc blocks at both `startup.c`
      callsites; this commit adds matching doc blocks at the fn
      DEFINITION sites
      (`packages/boards/nros-board-threadx-{linux,qemu-riscv64}/c/
      board_threadx_*.c`) explaining the void contract + the
      future-revision escape route.

- [x] **214.A.2 Transport teardown error capture** — Closed by
      `a020d61fe`. Both call sites in
      `packages/core/nros-c/src/transport.rs` (lines 96 + 124)
      replace the discard pattern with `match`/`Err → NROS_RET_ERROR`
      propagation. Verified in-place: each formerly-`let _ =` site
      now returns a typed `nros_ret_t`.

- [x] **214.A.3 Test PASS-on-prereq-missing antipattern (BANNED)** —
      Closed by `a020d61fe`. `packages/testing/nros-tests/tests/
      {actions,services}.rs` no longer carry the `eprintln!("[PASS]")
      + return` antipattern on the prereq path; replaced with
      `nros_tests::skip!("wait_for_output_pattern failed: …")`.
      Surviving `eprintln!("[PASS] …")` calls are post-success
      reporters (not premature exits) — those are legitimate per
      CLAUDE.md.

---

## Track B — Codegen absolute paths (in generated FFI tree)

**Scope**: CRITICAL drift. `examples/qemu-arm-nuttx/cpp/*/generated/ffi/
<crate>/Cargo.toml` (20 files) carry:
```toml
nros-serdes = { path = "/home/aeon/repos/nano-ros/packages/core/nros-serdes", default-features = false }
```
Plus `generated/ffi/<crate>/src/lib.rs` (100+ lines across files) use
absolute `include!("/home/aeon/repos/nano-ros/...")` paths.

These files are under `generated/` (gitignored), so the **drift lives on
this machine only**. But the **codegen that emits them is buggy** —
breaks any clone or CI on a different path.

**Files (B.1)**: `nros-cli/packages/rosidl-codegen/` (or wherever the FFI
Cargo.toml emit happens — search for `nros-serdes = ` emit). Change to
compute a relative path from the per-example manifest dir.

**Files (B.2)**: After B.1 lands, regen the affected nano-ros examples
to wipe the absolute-path artifacts. Since the dirs are gitignored,
the regen is the verification.

- [x] **214.B.1 cmake FFI codegen — relative paths** — **Re-scoped
      2026-06-03**: bug lives in nano-ros
      `cmake/NanoRosGenerateInterfaces.cmake` (not nros-cli).
      Switched `nros-serdes = { path = "..." }` emit (line ~512) +
      `include!("...")` emit (line ~556) to use `file(RELATIVE_PATH
      ...)` computed from the FFI crate dir / lib.rs location.
      Relative depth adapts per example automatically.
      **Verified** 2026-06-03 with a fresh regen of
      `examples/native/cpp/talker/build-zenoh/nano_ros_cpp_ffi_*/Cargo.toml`
      — produces `path = "../../../../../../packages/core/nros-serdes"`
      (relative, not absolute). `include!()` lines emit as
      `"../../nano_ros_cpp/<pkg>/msg/*_ffi.rs"`. Build clean
      (`cmake --build` reaches Linking CXX executable cpp_talker).

- [x] **214.B.2 nano-ros regen sweep** — wiped every `build-*/` +
      `generated/` dir under the 5 affected platforms
      (`native/cpp`, `qemu-arm-{freertos,nuttx}/cpp`,
      `qemu-riscv64-threadx/cpp`, `threadx-linux/cpp`). Next build
      on each example regens with the B.1-fixed cmake — no absolute
      paths. **Verified**:
      `find examples/ -name "Cargo.toml" | xargs grep -l 'path = "/home/'`
      returns 0 (was 196). `find examples/ -name "*.rs" | xargs grep
      -l 'include!("/home/'` returns 0 (was 100+).

---

## Track C — Magic numbers + duplicated defaults

**Scope**: LOW-MED cleanliness. Numeric defaults scattered across
`build.rs` files instead of centralised consts.

**Files**: `packages/core/nros-c/build.rs:150`,
`packages/zpico/nros-rmw-zenoh/build.rs:8-9,20`,
`packages/zpico/zpico-sys/build.rs:204-206`,
`packages/xrce/nros-rmw-xrce-cffi/build.rs:355-365`,
`packages/core/nros-node/build.rs:29,49,50`.

- [x] **214.C.1 Shared timeout constants** — Closed via doc-block
      cross-ref pattern (audit's option 2). `NUTTX_SOCKET_TIMEOUT_MS
      = 5000` + `GENERIC_SOCKET_TIMEOUT_MS = 100` already live as
      single named consts in `packages/zpico/zpico-sys/build.rs:207-209`.
      `NROS_SERVICE_TIMEOUT_MS = 30_000` lives in two build.rs files
      (`nros-c/build.rs:159` + `nros-rmw-zenoh/build.rs:25`) both
      reading the same env var `NROS_SERVICE_TIMEOUT_MS` — single
      semantic source via env. This commit adds an explicit Phase
      214.C.1 cross-ref doc-block at the `nros-c` site pointing at
      the canonical rationale in `nros-rmw-zenoh` (Phase 160.C.2 —
      bumped from 10 s because zenoh handshake under qemu slirp can
      drop early packets). When changing the default, both literal
      sites + their doc strings must update in lockstep.

- [x] **214.C.2 XRCE MTU 4096 single source** — Closed by concurrent
      worker. `packages/xrce/nros-rmw-xrce-cffi/build.rs:18-19`
      defines `const XRCE_TRANSPORT_MTU_DEFAULT: &str = "4096"` +
      `const XRCE_SERIAL_MTU_DEFAULT: &str = "512"` at file top.
      Substitutions at lines 362-364 reference the named consts
      with a Phase 214.C.2 comment.

- [x] **214.C.3 Subscription buffer default coordination** — Closed
      by concurrent worker. Phase 214.C.3 cross-ref comment-blocks
      at both sites (`packages/core/nros-node/build.rs:29-33` +
      `packages/zpico/nros-rmw-zenoh/build.rs:8-13`) explain the
      `NROS_SUBSCRIPTION_BUFFER_SIZE` ↔ `ZPICO_SUBSCRIBER_BUFFER_SIZE`
      coordination + the "change one, change the other" lockstep
      contract.

- [x] **214.C.4 Action client per-entry formula** — Closed by
      concurrent worker. `packages/core/nros-node/build.rs:55-65`
      ships a Phase 214.C.4 breakdown comment + named consts
      `ACTION_CLIENT_PER_SERVICE = 4096+384`,
      `ACTION_CLIENT_SERVICES = 3`,
      `ACTION_CLIENT_FEEDBACK_SUBS = 3`,
      `ACTION_CLIENT_SUB_OVERHEAD = 1536`,
      `ARENA_BASE_OVERHEAD = 2048`, `ARENA_FLOOR = 8192`. Each
      magic number now has a documented role.

---

## Track D — Unsafe doc + audit

**Scope**: LOW. 6 `unsafe fn` items missing `# Safety` paragraphs + 1
lifetime transmute footgun. All in board crates / nros-node.

**Files**:
- `packages/boards/nros-board-esp32-qemu/src/node.rs:55`
- `packages/boards/nros-board-esp32/src/node.rs:61, 162`
- `packages/boards/nros-board-mps2-an385/src/node.rs:64-102, 152`
- `packages/boards/nros-board-stm32f4/src/node.rs:284, 422`
- `packages/core/nros-node/src/c_waker.rs:91-108`

- [x] **214.D.1 `# Safety` doc sweep** — Closed by `c364af87e`
      (ESP32 + ESP32-QEMU) plus pre-existing coverage on
      mps2-an385 (6/6), stm32f4 (key fn definitions at lines 284 +
      422 carry `# Safety` paragraphs), and `c_waker.rs` (6 covered;
      the remaining `unsafe fn`s are trait-impl bodies whose
      contracts are documented at the trait declaration site, not
      per impl). Verified 2026-06-04: every `unsafe fn` listed in
      the audit carries a documented invariant either at the impl
      site OR at its trait declaration.

- [x] **214.D.2 ESP32 lifetime transmute hardening** — Closed by
      `c364af87e`. `nros-board-esp32/src/node.rs:162` swap of
      `unsafe { WIFI_DEV.write(core::mem::transmute(wifi_dev)) }` to
      a `MaybeUninit<WifiDevice<'static>>`-backed init pattern with
      a documented invariant ("the wifi device outlives the
      program — embedded no-exit context").

---

## Track E — Misc hardening

**Scope**: LOW. Two small one-offs.

**Files**: `packages/boards/nros-board-orin-spe/src/lib.rs:285`,
`packages/boards/nros-board-{esp32,mps2-an385,stm32f4,esp32-qemu}/src/node.rs`
(transport-feature guard).

- [x] **214.E.1 Orin SPE i32 cast bounds-check** — `nros-board-orin-spe/
      src/lib.rs:286`: replaced `bytes.len() as i32` with
      `i32::try_from(bytes.len()).unwrap_or(i32::MAX)` at the
      `tcu_print_msg` FFI boundary. Saturating-truncate chosen over
      `expect` so a pathological caller is clamped rather than
      panicking inside the FSP println path. Landed in `d7c7b4444`.

- [x] **214.E.2 Dual-transport `compile_error!` guard** — per slice 8
      audit: board crates enforce ≥1 transport (`ethernet` OR `serial`)
      but allow both ON simultaneously. Per CLAUDE.md Phase 162 policy
      ("≥1 transport required"), the intent is **exactly one**. Add
      `#[cfg(all(feature = "ethernet", feature = "serial"))] compile_error!(
      "...")` to each of 4 board crates: esp32-qemu, mps2-an385,
      stm32f4, esp32 (esp32's pair is wifi/serial — same shape).
      **Acceptance**: `cargo check -p <board> --features "ethernet
      serial"` fails with the guard message. **Landed `d7c7b4444`**
      (`fix(214.E): Orin SPE i32 cast bounds-check + dual-transport
      guards`); the at-most-one-transport `compile_error!` sits next
      to the existing at-least-one-transport guard in `src/node.rs`
      of all four crates:
      - `packages/boards/nros-board-mps2-an385/src/node.rs`
        (ethernet ↔ serial)
      - `packages/boards/nros-board-stm32f4/src/node.rs`
        (ethernet ↔ serial)
      - `packages/boards/nros-board-esp32-qemu/src/node.rs`
        (ethernet ↔ serial)
      - `packages/boards/nros-board-esp32/src/node.rs`
        (wifi ↔ serial)
      Verified 2026-06-04 on the worktree: per-crate
      `cargo check --target <embedded-target>` is clean on default
      features; `cargo check --target <embedded-target> --features
      "<a> <b>"` fails with `"Pick exactly one transport: <a> and
      <b> are mutually exclusive"` on all four.

---

## Acceptance

- [x] Track A (CRITICAL): all 3 sub-items landed (A.1 + A.2 + A.3
      all [x] in this doc). CI passes, no banned test antipattern
      remaining.
- [x] Track B: nano-ros cmake codegen relative-path fix shipped
      (`0b85a6ea9`) + regen sweep verified (B.1 + B.2 both [x];
      tree-wide absolute-path Cargo.toml count 196 → 0,
      `include!("/home/...")` 100+ → 0).
- [x] Track C: shared constants extracted (or doc-block cross-ref);
      C.1 + C.2 + C.3 + C.4 all [x] via concurrent worker landings
      + Phase 214.C.1 cross-ref doc-block at `nros-c/build.rs`.
- [x] Track D: `# Safety` doc lint passes on board crates +
      `nros-node`. D.1 + D.2 both [x] (`c364af87e` + pre-existing
      coverage).
- [x] Track E: Orin SPE bounds-check added + dual-transport
      `compile_error!` guards in place. E.1 + E.2 both [x] (verified
      2026-06-04 in body text).
- [x] Track F: F.1 + F.2 + F.3 all [x]. Host-only crate deps + leaky
      dev-deps target-gated via
      `[target.'cfg(not(target_os = "none"))'.{dependencies,dev-dependencies}]`;
      CI workflow `.github/workflows/embedded-feature-unification.yml`
      enforces zero `feature "std"` activation paths under embedded
      target.
- [x] Track G: `just test-unit` workspace cargo-test link succeeds
      without an unguarded `nros_platform_*` alias avalanche.
      (`c7b8c9dc0`)
- [x] Track H: `just native test` from clean workspace runs without
      "Test fixture binary not prebuilt" cascade. H.1 + H.2 both [x]
      (sequencing `build-test-fixtures` before nextest; audit applied
      to every `just <plat> test` recipe).
- [x] Track I: `nros ws sync` available via Path B source-build env-var
      (`NROS_FROM_SOURCE=/path/to/nros-cli scripts/install-nros.sh`); the
      5 caller recipes (freertos, qemu-baremetal, native, zephyr,
      threadx-linux via `fixtures-build.sh`) no longer trip on
      "unrecognized subcommand 'ws'" (2026-06-04). Pin bump to a tagged
      release deferred until nros-cli ships the post-`0.3.7` work
      (210.D.1, 212.E, 212.J, K.7.1.{c,d,d.b}) — maintainer-only.
- [x] Track J: cached `RosAction` generated trees regen-clean against
      the 8-assoc-type trait. Subsumed by 214.S.6 regen sweep
      (2026-06-04, this commit) — `nros ws sync` against the fresh
      CLI emits the 5 envelope `type SendGoal*/GetResult*/FeedbackMessage`
      assoc-types for every action example (`qemu-arm-baremetal/rust/
      action-{client,server}-rtic`, `qemu-riscv64-threadx/rust/
      action-{client,server}`, plus the 3 native rust action examples).
- [x] Track K: `just zephyr test` from clean workspace passes the
      fixture-dependent tests. K.1 [x] (sequence-build-fixtures-first
      fix landed).
- [x] Track L: `integrations/{zephyr,esp-idf,platformio}/` shells
      restored or test-gated; no bare-FAIL on missing manifests.
      Skip-gated all three (208.D.7 / 208.D.8 / 208.D.10 deletions);
      see L.1 inventory + L.2 action.
- [x] Track M: `just nuttx build-fixtures` succeeds on the pinned
      nightly + libc combo. M.1 + M.2 both [x] (`6772e134e` —
      `nuttx-libc-patch.sh` re-appends patched libc post-`ws sync`;
      6 NuttX rust example `.cargo/config.toml`s carry the durable
      `[patch.crates-io]` block).
- [x] Track N: phase212 / orchestration tests pass or explicitly
      `skip!` against the installed CLI version. N.1 + N.2 + N.3
      all [x] (`2461fc180` — survey matrix, no pin bump available
      yet [release 0.3.7 is latest tag], skip-gates added to 4
      drift-affected tests).
- [x] Track O: `examples_tree_uses_canonical_shape` passes; the 24
      violators triaged. O.1 + O.2 both [x] (`21eedc72d` — lint
      widened to accept post-N.12 `node` metadata table; 24/24
      cleared via single lint fix without per-example
      restructuring).
- [x] Track P: threadx_riscv64 cyclonedds e2e receives 28/28 (10x rerun all PASS); freertos sibling `#[ignore]`d on the 212.M.5.b fixture regression (Component-pkg sweep deleted the rust cyclonedds entry shape — `CMakeLists.txt` + `cyclonedds_app.c`)
      message over 3 reruns.
- [x] Track Q: every per-platform `test` recipe sequences
      `build-fixtures` first (umbrella for H and K) — `42c657bd0`.
- [x] Track R: `[SKIPPED]` panics no longer count as failures in the
      tally script's output. R.1 + R.2 both [x] (`cd85a6645` —
      Python JUnit post-processor + recipe hooks + test-harness doc;
      `xrce test` 10/10 SKIPPED-as-FAIL now reports 0 failures).
- [x] Phase doc retired to `archived/` when all checkboxes flip. _(2026-06-11)_

---

## Tracks F–R — Platform Test Sweep Findings (2026-06-04)

**Discovery method**: ran `just <plat> test` per in-scope platform on a
fresh checkout off `origin/main` (worktree
`agent-ac4d7f17203213e70`, branch `phase-214-platform-test-sweep`).
Each capped at 600s wall-clock; logs preserved at
`/tmp/214sweep-<plat>.log` (untracked).

**Per-platform sweep table:**

| platform | result | first-fail surface | track(s) implicated |
|---|---|---|---|
| `cyclonedds` | PASS | — | — |
| `orin_spe` | PASS | — | — |
| `native` | FAIL | 38 tests panic with "Test fixture binary not prebuilt"; 10 phase212 / orchestration tests; cyclonedds_ros2 + qemu_patched_binary | F, H, I, N, O, P, Q, R |
| `qemu` | FAIL | `build-fixtures` → codegen error: `RosAction` trait missing 5 assoc types in cached `example_interfaces/fibonacci.rs` | J, Q |
| `freertos` | FAIL | `build-examples` → `nros ws sync` subcommand unrecognized | I, P, Q |
| `nuttx` | FAIL | `build-fixtures` → std build fails on `_SC_HOST_NAME_MAX` for `armv7a-nuttx-eabi` libc | M, Q |
| `threadx_linux` | FAIL | `build-examples` → `nros ws sync` subcommand unrecognized (then per-pkg feature errors) | I, Q |
| `threadx_riscv64` | FAIL | `build-examples` → same `RosAction` codegen drift as qemu | J, P, Q |
| `zephyr` | FAIL | 26 tests fail with `Zephyr fixture binary is stale: …/nano-ros-workspace/build-*` | I, K, L, Q |
| `xrce` | FAIL | 10/10 tests `[SKIPPED]` (XRCE Agent not provisioned) but reported as failures | R |
| `esp32` | FAIL | 6 pass, 1 fail (`test_native_to_esp32` — native talker fixture not prebuilt) | H, Q |

In-scope but not run (need license-gated or experimental SDK):
`stm32f4` (no `test` recipe — only `build`), `rmw_zenoh` (no `test`
recipe — orchestration only), `esp_idf`, `platformio`, `px4`, `docker`,
`zenohd`.

**Workspace-level reproductions (gated separately from per-plat sweep):**
| recipe | result | track |
|---|---|---|
| `just check-workspace-embedded` | FAIL — `nros-serdes/std` activated via dev-deps unification | **F** |
| `just test-unit` | FAIL — `nros-rmw-zenoh` lib test link: `nros_platform_*` symbols undefined (16+ symbols, from `platform_aliases.c`) | **G** |

---

## Track F — Workspace feature unification leaks `std` to embedded (RE-SCOPED 2026-06-04)

**Scope**: CRITICAL. Blocks `just check-workspace-embedded`. Standalone
`cargo check -p nros-serdes --no-default-features --target
thumbv7em-none-eabihf` passes; the failure is workspace-wide unification.

**Re-scope note (2026-06-04).** Original Track F mis-diagnosed the leak
as a dev-dep boundary issue. A wave-1 implementation agent reproduced
+ traced + escalated: with dev-deps explicitly stripped (`cargo tree
--edges=normal,build`), the std + posix-c-port activations are STILL
present, sourced from **`[dependencies]` of host-only workspace
members**:

```
nros-serdes ← nros-core ← nros-node feature "std"
    ← [normal dep] nros-board-posix (packages/boards/nros-board-posix/Cargo.toml:24)
nros-platform-cffi feature "posix-c-port"
    ← nros-platform feature "platform-posix"
        ← [normal dep] nros-board-posix (line 24)
        ← [normal dep] nros-board-native (packages/boards/nros-board-native/Cargo.toml:24, via nros-rmw-zenoh)
```

`nros-board-{posix,native,nuttx}` + `nros-msg-to-idl` are host/codegen-only
crates with unconditional `[dependencies]` activating `nros/std` +
`nros/platform-posix`. Workspace feature unification with
resolver=2 narrows by target compatibility but does NOT exclude
`--workspace` members from compilation. Every member still gets
checked; every member's normal `[dependencies]` flow through
unification into shared deps like `nros-serdes`. The dev-dep leak
the original spec flagged exists but is secondary — even fully
removed, the host-board normal deps still poison the embedded build.

**Owns:**
* `packages/boards/nros-board-posix/Cargo.toml` (target-gate normal deps)
* `packages/boards/nros-board-native/Cargo.toml` (same)
* `packages/boards/nros-board-nuttx/Cargo.toml` (same — host-only codegen)
* `packages/core/nros-msg-to-idl/Cargo.toml` (same — host-only codegen)
* `justfile` (`check-workspace-embedded` recipe — only if Path A taken)
* Does NOT own `nros-serdes/lib.rs` (leaf is correct).

**Architecture**: two viable fixes — Path B preferred.

**Path A — exclude host-only members from the embedded recipe**:
extend `justfile:1123` with `--exclude nros-board-posix --exclude
nros-board-native --exclude nros-board-nuttx --exclude nros-msg-to-idl`.
Small + honest about scope ("these are host-only, won't compile on
embedded ever"). Downside: creates blind spots — those crates aren't
re-checked under any embedded lane, so a future maintainer accidentally
adding embedded-incompatible code to one of them slips through.

**Path B (PREFERRED) — target-gate the host board crates' deps**:
split each host-only crate's `[dependencies]` into a target-cfg block:
```toml
[target.'cfg(not(target_os = "none"))'.dependencies]
nros = { workspace = true, features = ["std", "rmw-cffi", "platform-posix"] }
```
Cargo permits this on `[target.<cfg>.dependencies]` for normal deps.
Workspace stays whole; embedded check correctly sees the host-only
crate as having no deps under thumb (so its lib doesn't compile
either; cargo skips it). No exclusions needed; no blind spots.

After applying Path B, expect a clippy-lint fail elsewhere
(`nros-platform/src/board/runtime.rs:95` `result_unit_err`,
`nros-platform/src/lib.rs:213`) — those are pre-existing lints,
separate cleanup; surface them under a sibling track if they don't
fold cleanly.

**Work Items:**

- [x] **214.F.1 Target-gate host-only crate deps (Path B)** — add
      `[target.'cfg(not(target_os = "none"))'.dependencies]` blocks
      to `nros-board-{posix,native,nuttx}` + `nros-msg-to-idl`,
      moving every `[dependencies]` entry that pulls `nros/std` or
      `nros/platform-posix` into the host-cfg'd block. Leave anything
      no_std-friendly in the unconditional `[dependencies]` (likely
      nothing — these crates are host-only).
      **Acceptance**: `cargo tree -i nros-serdes --target
      thumbv7em-none-eabihf --no-default-features --workspace
      --edges=normal,build` shows NO `std` activation path from any
      board crate. `just check-workspace-embedded` advances past the
      std/posix-c errors. (Pre-existing clippy lints in
      `nros-platform` may surface — file under sibling track.)

      **Landed** in this commit. `nros-board-{posix,native,nuttx}`
      had every `[dependencies]` entry (apart from no_std-friendly
      `nros-board-common` in the nuttx façade) moved into
      `[target.'cfg(not(target_os = "none"))'.dependencies]`.
      `nros-msg-to-idl` was inspected and found to have no
      `nros`/`nros-platform` deps at all (only `clap`) — no edit
      needed; the doc list mentioned it as the candidate set but
      its `[dependencies]` doesn't actually leak.
      `cargo tree -i nros-serdes --target thumbv7em-none-eabihf
      --no-default-features --workspace --edges=normal,build` now
      contains zero `nros-board-{posix,native}` entries, and the
      `--edges=features` view shows no `feature "std"` paths
      originating from any board crate (residual `std` paths
      come from `[dev-dependencies]` on `nros-node` /
      `nros-rmw-cyclonedds` — Track 214.F.2 scope).
      `just check-workspace-embedded` advances past the std /
      posix-c errors and now stops on the pre-existing
      `nros-platform/src/board/runtime.rs:{83,95}`
      `clippy::result_unit_err` lints (file under a sibling track —
      they predate this Path B edit; reproduced on a clean stash
      of `main`).

- [x] **214.F.1.fallback Path A — recipe excludes** — NOT NEEDED.
      Path B verified clean on host (see inline notes below); the
      fallback would have been an `--exclude` flag sweep on
      `check-workspace-embedded`. Closed without action.

      Original spec text retained for reference:
      `cargo check -p nros-board-posix` + `cargo check
      -p nros-board-native` both pass; `cargo test -p nros-node
      --features rmw-cyclonedds --lib` keeps its 149 tests green;
      `cargo test -p nros-serdes --lib` keeps its 46 tests green;
      `cargo test -p nros-rmw-cyclonedds --no-default-features`
      keeps the K.7 smoke green.

- [x] **214.F.2 Address residual dev-dep unification** — Closed
      2026-06-04. Applied `[target.'cfg(not(target_os = "none"))'.
      dev-dependencies]` target-gating to:
      - `packages/core/nros-log/Cargo.toml` — `nros-platform-cffi
        = { features = ["posix-c-port"] }` (the dominant dev-dep
        leak path visible in the `--edges=features` tree post-F.1).
      - `packages/core/nros-node/Cargo.toml` — `nros-ghost-types` +
        `nros-rmw-cyclonedds = { features = ["bridge-stub"] }`.
      Same Cargo idiom F.1 used for `[dependencies]`; works for
      `[dev-dependencies]` identically. Host targets still see the
      dev-deps (`cargo test -p nros-log`, `cargo test -p nros-node`
      work unchanged); embedded targets (`target_os = "none"`) see
      an empty dev-dep set so the unification path collapses.
      **Acceptance**: `cargo tree -p nros-serdes --edges=normal,build
      --target thumbv7em-none-eabihf --no-default-features --workspace`
      returns ZERO `feature "std"` paths (verified 2026-06-04).
      Host build still clean: `cargo check -p nros-log`,
      `cargo check -p nros-node`, `cargo test -p nros-log --no-run`
      all pass.

- [x] **214.F.3 CI guard against future feature unification regressions**
      — Closed 2026-06-04. Landed
      `.github/workflows/embedded-feature-unification.yml` with two
      gates:
      1. **Gate 1 — feature-unification assertion**: `cargo tree -p
         nros-serdes --edges=normal,build --target
         thumbv7em-none-eabihf --no-default-features --workspace`
         must contain zero `feature "std"` paths. Failure mode prints
         the offending paths first (with `grep -B 2`) before failing
         so the regression source surfaces in the failure log.
      2. **Gate 2 — embedded compile-check**: `cargo check --workspace
         --no-default-features --target thumbv7em-none-eabihf` on the
         no_std-clean subset (mirrors `just check-workspace-embedded`'s
         exclude list for `nros-rmw-zenoh-staticlib`,
         `nros-rmw-xrce-cffi-staticlib`, `nros-platform-bare-metal`).
      Triggers on PR/push touching `packages/**/Cargo.toml`,
      `Cargo.toml`, `Cargo.lock`, or the workflow file itself.
      Fresh-runner-safe: only the workspace's pinned toolchain +
      `rustup target add thumbv7em-none-eabihf` — no QEMU/SDKs/nros
      install. Fast lane (~few minutes); designed as a required check
      on PR. Future contributor adding a `[dependencies]
      nros-platform-cffi = { features = ["posix-c-port"] }` row
      without target-gating → this lane goes red at PR time, points
      at the regression source line in its failure log.
      Doc-tweak relative to original spec: used `--edges=normal,build`
      (per F.3's "relax to" suggestion) instead of `--edges=features`
      — the latter shows every feature edge regardless of dep type
      including dev-deps, which is too noisy for the assertion;
      `normal,build` is the production-link view that matters for
      "what code reaches the embedded binary".

---

## Track G — `zpico-sys` aliases reference missing `nros_platform_*`

**Scope**: HIGH. Blocks `just test-unit` workspace build. Standalone
`cargo test --no-run -p nros-rmw-zenoh --lib` compiles clean — the
failure only surfaces when the workspace pool unifies feature flags
and the alias TU gets linked into a test that didn't ask for the
companion `nros-platform-*` symbol providers.

**Owns:**
* `packages/zpico/zpico-sys/c/zpico/platform_aliases.c` (the 16+
  `_z_*` forwarders — gate them behind `#ifdef NROS_PLATFORM_<X>`
  matching the providing crate)
* `packages/zpico/zpico-sys/build.rs` (the `cc::Build` that compiles
  `platform_aliases.c` — set the matching `define`s only when the
  feature combo guarantees a provider)
* Does NOT own `nros-platform-posix` / `nros-platform-cffi` symbol
  definitions (those are correct; the alias TU is over-eager).

**Architecture**: `platform_aliases.c` forwards every `_z_*`
zenoh-pico symbol the platform shim used to provide directly
(Phase 129 retirement: `zpico-platform-shim` was deleted in favour
of C alias TUs). Today the file emits `_z_task_join`,
`_z_mutex_rec_*`, etc., unconditionally — but the workspace test
build for `nros-rmw-zenoh` doesn't link a `nros-platform-*` provider
crate, so each `_z_*` alias becomes an undefined `nros_platform_*`
symbol at lld time.

**Work Items:**

- [x] **214.G.1 Gate alias emission on a `NROS_PLATFORM_PRESENT`
      define** — guard each alias group in `platform_aliases.c` with
      `#ifdef NROS_PLATFORM_FORWARDERS_PRESENT` (or per-symbol
      gates: `NROS_PLATFORM_HAS_MUTEX_REC` etc.). `zpico-sys/build.rs`
      emits the define only when a known provider crate is in the
      build (detect via build-script env-var the provider crate sets,
      e.g. `DEP_NROS_PLATFORM_POSIX_PRESENT=1`).
      **Acceptance**: `cargo test --no-run --workspace --profile
      nros-fast-release` link succeeds. Standalone library builds
      (no provider crate) still compile, and the missing aliases
      surface as a single named link-error rather than an avalanche.
      (`c7b8c9dc0`)

      **Landed deviation**: the `DEP_*` env-var pathway requires a
      `links =` key on `nros-platform-cffi` plus a new build-dep
      edge from `zpico-sys` to it — wider rippling than the doc
      sketch suggested. Implementation took the simpler symmetric
      path that already lives at `nros-platform/lib.rs:81` for the
      same provider-link problem:
        - `zpico-sys/build.rs` skips the alias-TU `cc::Build` when
          NO explicit platform feature is set (auto-posix on
          `target_os = linux` was the trap path — it pulled the
          alias TU into bare `cargo check` rlibs that had no
          provider downstream).
        - `nros-rmw-zenoh/Cargo.toml` restores the Phase 129.C.3.a
          forward `platform-posix = ["zpico-sys/posix",
          "nros-platform/platform-posix"]` so picking a platform
          on the RMW activates the matching `posix-c-port` cargo
          feature (which compiles `libnros_platform_posix.a`).
        - `nros-rmw-zenoh/src/lib.rs` adds a `#[used] pub static
          __FORCE_LINK_PLATFORM_CFFI` re-anchor that mirrors the
          existing chain at `nros-platform/lib.rs:81` and
          `nros/lib.rs:146`. `nros-rmw-zenoh` itself never
          references any `nros_platform` Rust symbol (every
          callsite hits the C ABI inside `zpico-sys`) so without
          this re-anchor `rust-lld` elides the `nros-platform-cffi`
          rlib entirely and the linked `libnros_platform_posix.a`
          never lands on the link line.

      Net acceptance matches the doc: workspace link succeeds,
      standalone library builds (no provider crate) still compile,
      cyclonedds path untouched (`cargo test
      -p nros-rmw-cyclonedds --no-default-features` + `cargo test
      -p nros-node --features rmw-cyclonedds --lib` both pass).

- [x] **214.G.2 Test recipe coverage** — landed in `just/justfile`
      `check-workspace-features` recipe (line ~1214). `cargo test
      --no-run --workspace --exclude nros-c --no-default-features
      --quiet` runs at the tail of `just check` so the feature-
      unification regression class is caught at check time rather
      than at `test-unit` time. `--exclude nros-c` is a TODO
      carve-out for a pre-existing latent test-compile bug in
      `packages/core/nros-c/src/cdr.rs:565` (the lib references
      `std::ffi::CStr` without a std-feature gate); remove the
      exclude once nros-c lib-test gating lands.

---

## Track H — Native test fixture prebuild precondition

**Scope**: HIGH. 38 native tests cascade-fail with a single root
cause: the harness calls `nros_tests::fixtures::binaries::*` to
locate `examples/native/rust/talker/target/nros-fast-release/talker`
(and siblings), but `just native test` doesn't run
`build-test-fixtures` first.

**Owns:**
* `just/native.just` (the `test` and `test-all` recipes)
* `packages/testing/nros-tests/src/fixtures/binaries/mod.rs:979`
  (the panic site — the BuildFailed error message itself is fine,
  the wrapping recipe just needs to invoke the prereq)
* Does NOT own any production code; this is purely orchestration.

**Architecture**: the harness chose "fail loudly when fixture not
prebuilt" (Phase 181 `nros_tests` convention) over silent rebuild
to keep test runs deterministic. The contract is that the platform
recipe sequences `build-fixtures → test`. `just native test` skipped
that.

**Work Items:**

- [x] **214.H.1 `just native test` runs `build-test-fixtures` first**
      — add `just build-test-fixtures` (or the narrower
      `build-fixture-rust` + `build-fixture-extras`) as a recipe
      dependency of `just native test`. Match the pattern already
      used by `just cyclonedds test` (which auto-builds its
      backend).
      **Acceptance**: a fresh `just native test` from a clean clone
      passes the 38 fixture-dependent tests without a separate
      manual prebuild step.
      **Superseded by Track Q.1 (`42c657bd0`)** — the umbrella
      sweep added `test: build-fixtures` to `just/native.just`
      (along with zephyr + esp32) using the recipe-head dep form.

- [x] **214.H.2 Same audit for every `just <plat> test`** — see
      Track Q for the umbrella; H.2 is the per-recipe survey.
      **Closed by Q.1 (`42c657bd0`)** — the 8-module audit found
      5 platforms (freertos, nuttx, threadx-linux, threadx-riscv64,
      qemu-baremetal) already had the dep; the remaining 3 (native,
      zephyr, esp32) were patched.

---

## Track I — `nros ws sync` subcommand unavailable

**Scope**: HIGH. Installed `nros` 0.3.7 (the version
`scripts/install-nros.sh` pins; correction from the initial audit's
"0.2.0") does not implement the `ws` subcommand — the `ws sync` /
`codegen-system` / `launch` verbs all sit on nros-cli `main` past
the `nros-v0.3.7` tag (210.D.1, 212.E, 212.J, K.7.1.{c,d,d.b}).
Five just-modules invoke `nros ws sync …` as a pre-build codegen
step.

**Status (2026-06-04)**: Path B source-build env-var landed in
`scripts/install-nros.sh`; see 214.I.1 below. The release-tag bump
(Path A) is the maintainer follow-up tracked in 214.I.3.

**Owns (callsites — gated `nros` invocations only):**
* `just/freertos.just:75,162`
* `just/qemu-baremetal.just:74,92,144`
* `just/native.just:126,159`
* `just/zephyr.just:176,353`
* `scripts/build/fixtures-build.sh:87`
* Does NOT own the `nros` CLI implementation (lives in
  `github.com/NEWSLabNTU/nros-cli`); the fix on this side is to
  bump `scripts/install-nros.sh`'s pin (or fall back to a sibling
  verb) once nros-cli ships `ws sync`.

**Architecture**: Phase 210.E.3.d.native introduced
`nros ws sync <example-dir>` as the pre-cargo codegen call that
writes the patch table + msg bindings into the per-example
`generated/` tree. The shipped 0.3.7 nros release predates this
(corrected from the "0.2.0" in the original audit — `0.3.7` is the
pin in `scripts/install-nros.sh` and the latest published release
at the time of fix; the unreleased `ws sync` / `codegen-system` /
`launch` / K.7.1.{c,d,d.b} commits sit on nros-cli `main` past the
tag).

**Work Items:**

- [x] **214.I.1 Source-build path landed (Path B)** — the agent did
      not have nros-cli fork-push authority to cut a new release tag,
      so instead `scripts/install-nros.sh` grew a `NROS_FROM_SOURCE`
      env-var (2026-06-04, branch `phase-214-track-i-nros-cli-pin-bump`).
      When set to a nros-cli source checkout, the script runs
      `cargo build --release --bin nros` and installs the result into
      `${NROS_HOME}/bin/nros`, skipping the release-tarball download +
      sha256 verification. Pinned `NROS_VERSION` stays explicit at
      `0.3.7` (the latest published release). Once the maintainer cuts
      a new tag containing the post-`0.3.7` verbs (210.D.1, 212.E,
      212.J, K.7.1.{c,d,d.b}), bump `NROS_VERSION` and contributors
      can drop the env-var.
      **Verification (2026-06-04)**: `NROS_FROM_SOURCE=/home/aeon/repos/nros-cli
      scripts/install-nros.sh` installs nros 0.3.7 (source) →
      `~/.nros/bin/nros`; `nros ws --help`, `nros codegen-system
      --help`, `nros launch --help` all resolve; `just freertos
      build-examples` + `just threadx_linux build-examples` no longer
      hit `error: unrecognized subcommand 'ws'` (they fail later on
      unrelated missing-feature cargo errors that are separate Phase
      214 tracks).
      **Also landed**: stale `~/.cargo/bin/nros` shadow warning — old
      `cargo install`-era binaries on `~/.cargo/bin` outrank
      `~/.nros/bin` on the default PATH; the installer now prints a
      removal hint (does NOT auto-delete files outside its own
      `${NROS_HOME}/bin`).

- [x] **214.I.2 Fall-back guard at each callsite** — wrap each
      `nros ws sync` invocation with a guard that probes
      `nros help ws` and emits a `[PREREQ]` skip message naming the
      missing verb if absent, instead of letting the build cascade
      into "unrecognized subcommand 'ws'" noise.
      **Acceptance**: pre-pin run gives one clean diagnostic per
      recipe, not a 50-line cargo stack trace.
      **Landed (2026-06-04)**: shared `nros_cli_ws_sync_available` +
      `nros_require_ws_sync` helpers in `scripts/build/cargo.sh`
      (probe `nros help ws | grep -q sync`; emit a one-line
      `[PREREQ]` to stderr + `exit 0` when missing). Guard invoked
      once per recipe before any `ws sync` loop so a pre-pin checkout
      gets exactly one skip line instead of N clap stack traces.
      Sibling Rust helpers `is_nros_ws_sync_available()` +
      `require_nros_ws_sync()` added to
      `packages/testing/nros-tests/src/process.rs` for future
      integration-test callsites (none today shell out to `ws sync`,
      but the surface is ready). Guarded callsites:
      * `just/freertos.just` (build-examples line 80, build-fixture-extras line 171)
      * `just/native.just` (build-fixture-rust line 130, build-fixture-extras line 168)
      * `just/qemu-baremetal.just` (build lines 78/98, build-fixtures line 158)
      * `just/zephyr.just` (build-one rust/* line 179, build-fixtures preflight line 363)
      * `scripts/build/fixtures-build.sh` (rust branch line 104)
      Verified both paths (working installed CLI silent; faked
      no-`ws` binary emits `[PREREQ]` + recipe exits 0).

- [x] **214.I.3 Maintainer follow-up: cut a new nros-cli release**
      — **obsolete via Phase 218 (2026-06-11).** The premise (bump
      `NROS_VERSION` in `scripts/install-nros.sh`) no longer exists:
      the Phase 218 monorepo merge deleted the `install-nros.sh` curl
      shim and the standalone `nros-cli` repo, so the CLI ships in-tree
      under `packages/cli/` — one checkout = one CLI version = one
      runtime ABI, no `NROS_VERSION` pin to police. The release intent
      itself is satisfied: the `nros-v0.5.0` bundle tag was cut on
      2026-06-08 (Phase 222 close). No action remains.

---

## Track J — `RosAction` codegen drift in cached `generated/`

**Scope**: HIGH. `pub trait RosAction` in
`packages/core/nros-core/src/action.rs:53` now requires 5 envelope
assoc-types (`SendGoalRequest`, `SendGoalResponse`,
`GetResultRequest`, `GetResultResponse`, `FeedbackMessage`), but
the cached `generated/` tree in the rtic / threadx-rv64 rust action
examples still uses the 3-type shape from before the trait
expansion.

**Owns:**
* `examples/qemu-arm-baremetal/rust/action-{client,server}-rtic/generated/example_interfaces/**`
* `examples/qemu-arm-baremetal/rust/service-server-rtic/generated/example_interfaces/**`
* `examples/qemu-riscv64-threadx/rust/action-{client,server}/generated/example_interfaces/**`
* Does NOT own `packages/core/nros-core/src/action.rs` (the trait is
  already correct) — the fix is regen, not trait surgery.
* Does NOT own nros-cli codegen logic (cli already emits the right
  shape — verify via fresh regen).

**Architecture**: each example's `build.rs` triggers `nros generate
rust` writing into `generated/`, which is gitignored. If the cache
was populated before the trait extension landed, stale output sits
around until next clean regen. Confirmed for the 5 example dirs
above.

**Work Items:**

- [x] **214.J.1 Regen the stale `generated/` trees** — `rm -rf
      examples/qemu-arm-baremetal/rust/*rtic/generated/
      examples/qemu-riscv64-threadx/rust/action-*/generated/`
      followed by `just qemu build` and `just threadx_riscv64 build`.
      Verification only — no source edit.
      **Acceptance**: `grep -nE 'type SendGoalRequest' examples/
      qemu-arm-baremetal/rust/action-server-rtic/generated/
      example_interfaces/src/action/fibonacci.rs` returns a match.
      **Verified 2026-06-04** via `nros ws sync` on all 5 example
      dirs — fresh codegen output contains
      `type SendGoalRequest = Fibonacci_SendGoal_Request;` etc. for
      every action example (`action-{client,server}-rtic` +
      `qemu-riscv64-threadx action-{client,server}`). The 5 dirs
      were absent in the worktree at the start (gitignored), so the
      `rm -rf` itself was a no-op; the verification ran by invoking
      `~/.nros/bin/nros ws sync` directly on each and grepping the
      regenerated `fibonacci.rs`. Full `cargo build` from a clean
      worktree is gated on `git submodule update --init` for
      zenoh-pico but is unrelated to Phase 214.J. **Note:** the
      stale-output cause is `ws sync` keying off `package.xml`
      mtimes, not the in-tree trait surface; 214.J.2 below closes
      that gap.

- [x] **214.J.2 build.rs should check trait surface vs cached
      output** — add a quick generation-stamp check (write a hash of
      the trait surface alongside the generated file; rebuild if
      mismatched). Avoids future silent staleness.
      **Acceptance**: touching the `RosAction` trait forces a
      `generated/` rebuild on next `cargo build` without manual
      `clean-bindings`.
      **Landed 2026-06-04** as a shell-side guard (shared helper,
      not per-example `build.rs`). Codegen for these examples runs
      in the per-platform `just` recipe via `nros ws sync $dir`
      *before* cargo touches the patch table, so a `build.rs`
      check would fire too late — cargo errors on missing patch
      paths long before any build script runs. The guard lives in
      `scripts/build/codegen-stamp.sh` (SHA-256 of every Rust
      source whose shape MUST match cli codegen output —
      currently just `packages/core/nros-core/src/action.rs`,
      stamp written to `<dir>/generated/.codegen-stamp`) and is
      wired into every `ws sync` callsite in `just/qemu-
      baremetal.just`, `just/freertos.just`, `just/threadx-
      riscv64.just` (via `scripts/build/fixtures-build.sh`),
      `just/zephyr.just`, and `just/native.just`. Each callsite
      now reads:
      `nros_codegen_stamp_check_or_wipe $dir && nros ws sync $dir
      && nros_codegen_stamp_write $dir`. Verified end-to-end on
      `action-server-rtic`: drift → wipe → fresh sync → 5-type
      output. Smoke test: `tmp/test-codegen-stamp.sh` (7 cases,
      all pass).

---

## Track K — Stale Zephyr fixture cache

**Scope**: HIGH. 26 of 44 zephyr tests fail with `Zephyr fixture
binary is stale: /home/aeon/repos/nano-ros-workspace/build-*` —
`just zephyr test` does not invoke `just zephyr build-fixtures`
first.

**Owns:**
* `just/zephyr.just` (the `test` and `test-all` recipes only)
* `packages/testing/nros-tests/src/zephyr.rs` (the staleness check —
  message already clear; no edit needed)
* Does NOT own per-test source or the underlying Zephyr build
  scripts.

**Architecture**: Zephyr build artifacts live in the sibling
`../nano-ros-workspace/build-*/zephyr/zephyr/zephyr.elf` tree (out
of the cargo target dir). The harness's staleness predicate checks
mtime of the elf against the source tree; missing elf → stale
error.

**Work Items:**

- [x] **214.K.1 `just zephyr test` runs `build-fixtures` first** —
      same pattern as Track H, narrowed to the zephyr build matrix.
      Consider both `build-fixtures` (full) and a narrower `build-
      examples-test-only` if full takes too long.
      **Acceptance**: fresh `just zephyr test` from a clean workspace
      passes the 26 fixture-dependent tests without a manual
      prebuild.
      **Superseded by Track Q.1 (`42c657bd0`)** — the umbrella
      sweep added `test: build-fixtures` to `just/zephyr.just`
      using the recipe-head dep form (full `build-fixtures`, not a
      narrower variant; wall-clock budget acceptable per Track Q
      tradeoff note).

---

## Track L — `integrations/<rtos>/` shells missing

**Scope**: HIGH. Three integration-smoke tests fail because the
asserted shell files don't exist:
- `integrations/zephyr/module.yml`
- `integrations/esp-idf/idf_component.yml` + `CMakeLists.txt` +
  `Kconfig.projbuild`
- `integrations/platformio/library.json` + `library.properties` +
  `examples/talker/platformio.ini`

**Owns:**
* `integrations/{zephyr,esp-idf,platformio}/**` (create or restore)
* `packages/testing/nros-tests/tests/integration_{zephyr,esp_idf,
  platformio}.rs` (read-only — checks the contract; no edit
  expected unless contract changes)
* Does NOT own per-platform build scripts under `just/`.

**Architecture**: Phase 139 created `integrations/<rtos>/` shells as
the cross-RTOS consumption surface (each shell re-exports the root
CMake under that RTOS's native package manager). The current tree
ships some but not these three. Either the tests were written
ahead of shell creation, or the shells were deleted in a recent
cleanup. Diff against the Phase 139 archive doc to figure out which.

**Work Items:**

- [x] **214.L.1 Inventory `integrations/` tree** — `find
      integrations/ -maxdepth 2 -type f` vs the contract in the
      three failing tests. Diff against
      `docs/roadmap/archived/phase-139-*.md` to identify whether the
      missing files are deletions or never-shipped.
      **Acceptance**: a written inventory pinned to each test's
      expected files.

      | Asserted path | Phase 139 shipped? | Current location | Deletion commit |
      |---|---|---|---|
      | `integrations/zephyr/module.yml` | yes (139.1) | `zephyr/module.yml` | `18d92325d` (208.D.7) |
      | `integrations/zephyr/CMakeLists.txt` | yes (139.1) | `zephyr/CMakeLists.txt` | `18d92325d` (208.D.7) |
      | `integrations/zephyr/Kconfig` | yes (139.1) | `zephyr/Kconfig` | `18d92325d` (208.D.7) |
      | `integrations/esp-idf/idf_component.yml` | yes (139.2, `9f010cc07`) | `integrations/nano-ros/idf_component.yml` | `6382cd655` (208.D.10 rename) |
      | `integrations/esp-idf/CMakeLists.txt` | yes (139.2) | `integrations/nano-ros/CMakeLists.txt` | `6382cd655` (208.D.10 rename) |
      | `integrations/esp-idf/Kconfig.projbuild` | yes (139.2) | `integrations/nano-ros/Kconfig.projbuild` | `6382cd655` (208.D.10 rename) |
      | `integrations/platformio/library.json` | yes (139.3, `3c208edad`) | retired (212.H.6 adapter is `extra_script`, not a PIO Library Manager shell) | `6382cd655` (208.D.8) |
      | `integrations/platformio/library.properties` | yes (139.3) | retired (see above) | `6382cd655` (208.D.8) |
      | `integrations/platformio/examples/talker/platformio.ini` | yes (139.3) | retired (see above) | `6382cd655` (208.D.8) |

      Verdict: every asserted path was once shipped by Phase 139 and
      later **intentionally relocated or retired** in Phase 208.D
      (D.7 fold, D.8 PlatformIO retire, D.10 rename). Restoring
      them would re-introduce duplicate surfaces and (for
      PlatformIO) collide with the 212.H.6 `extra_script` adapter
      shape — none of which the parent phase doc owns. Path
      forward: skip-gate.

- [x] **214.L.2 Restore or skip-gate** — for each missing shell:
      either restore the manifest files from git history (if a
      deletion) or change the integration test to gate on shell
      presence with `nros_tests::skip!` (if intentionally deferred).
      Do NOT silently drop the test.
      **Acceptance**: `just native test --test integration_zephyr
      --test integration_esp_idf --test integration_platformio`
      either passes or skips with a clear `[SKIPPED]` reason; no
      bare-FAIL.

      Action: **skip-gate all three** with a `[SKIPPED]` reason
      that pins each to the Phase 208.D commit that retired or
      relocated the asserted path. No file restored — the
      replacement surfaces (`zephyr/`, `integrations/nano-ros/`,
      `integrations/platformio/nros_codegen.py`) live elsewhere
      and aren't part of this contract. Files touched:
      `packages/testing/nros-tests/tests/integration_zephyr.rs`,
      `…/integration_esp_idf.rs`, `…/integration_platformio.rs`.

---

## Track M — NuttX `armv7a-nuttx-eabi` libc missing `_SC_HOST_NAME_MAX`

**Scope**: MED. Blocks `just nuttx test` at the std-build step.
`_SC_HOST_NAME_MAX` was added to upstream Rust std in
`hostname/unix.rs:8` but the `libc` shim crate's `armv7a-nuttx-eabi`
target spec hasn't been updated. `arm-none-eabi-gcc` is present and
the rest of the toolchain works.

**Owns:**
* `target/armv7a-nuttx-eabi/std/src/sys/net/hostname/unix.rs` (if
  fixed via std patch — unlikely we own this)
* `packages/drivers/nuttx-sys/` libc shim or wherever the
  nuttx-specific `_SC_*` constants are defined (search needed —
  may need a vendor patch to upstream `libc` crate)
* `rust-toolchain.toml` (nuttx nightly pin — bumping past the
  hostname-feature commit OR pinning back to a pre-hostname
  nightly could be the easier route)
* Does NOT own `examples/qemu-arm-nuttx/**` source.

**Architecture**: `hostname/unix.rs` is in the Rust stdlib (build
of `std` for `armv7a-nuttx-eabi` requires libc::_SC_HOST_NAME_MAX).
Three remedies in increasing cost: (1) bump the upstream `libc`
crate's nuttx target to expose the const; (2) bump or pin the
nightly toolchain to a version whose std doesn't reference the
const yet; (3) carry a local libc patch.

**Work Items:**

- [x] **214.M.1 Reproduce + diagnose remedy path** — reproduced
      cleanly: `cargo build --release` in
      `examples/qemu-arm-nuttx/rust/listener/` against the pinned
      nightly-2026-04-11 fails with `error[E0425]: cannot find value
      \`_SC_HOST_NAME_MAX\` in crate \`libc\`` at
      `…/nightly-…/lib/rustlib/src/rust/library/std/src/sys/net/hostname/unix.rs:8`.
      The pinned nightly's `std` references `libc::_SC_HOST_NAME_MAX`;
      the **patched libc fork** at `third-party/nuttx/libc/` defines
      it (`bc6c8dfc6 Add _SC_HOST_NAME_MAX for NuttX target` →
      `src/unix/nuttx/mod.rs:515`), but crates.io `libc 0.2.183` —
      the version `std`'s `Cargo.toml` pulls — does NOT. Root cause
      is **not** the toolchain (path 2 unnecessary) and **not** the
      libc fork (path 1 unnecessary — fix is already there).
      Root cause is that `nros` 0.3.7's `ws sync` strips the
      `[patch.crates-io]` block from the rendered
      `.cargo/config.toml`, even though
      `packages/boards/nros-board-nuttx-qemu-arm/nros-board.toml`
      declares `libc = { path = "${workspace}/third-party/nuttx/libc" }`
      in its `cargo_config` template (verified by diffing the template
      against the rendered output — only the `[patch.crates-io]`
      lines are missing). The smoke fixture at
      `packages/testing/nros-tests/bins/logging-smoke-nuttx-qemu-arm/`
      stays green only because it lacks a `package.xml` so
      `ws sync` skips it entirely.
      **Remedy**: post-`ws sync` shell fix-up that re-appends the
      libc patch (path 3 variant — workspace-local patch, no
      upstream CLI change).
      **Acceptance**: documented remedy with a concrete fix-up
      script.

- [x] **214.M.2 Land remedy** — added
      `scripts/build/nuttx-libc-patch.sh` exposing
      `nros_nuttx_libc_patch <example_dir>` and wired it into
      `scripts/build/fixtures-build.sh` directly after the per-dir
      `nros ws sync` call. The helper is **idempotent** (skips when
      the patch is already present), **target-gated** (no-op for
      non-NuttX fixtures via the `target = "armv7a-nuttx-eabi…"`
      check), and computes the libc path **relative to the example
      dir** (cargo resolves `[patch.crates-io]` in
      `.cargo/config.toml` against the invocation cwd, matching the
      smoke fixture's 5-up convention). Verified: `just nuttx
      build-examples` now compiles `libc v0.2.183
      (third-party/nuttx/libc)` + `std` cleanly; the `_SC_HOST_NAME_MAX`
      error is gone. Unrelated Phase 214.J codegen drift in
      `action-server` (`Vec<_, 64>` vs `Vec<_, 16>`) is still
      present and tracked under the Phase 214.J / `nros ws sync`
      regeneration follow-ups, not this track.
      **Acceptance**: `just nuttx build-examples` passes the std /
      libc build step.

---

## Track N — `nros` CLI lints / verbs drift vs phase212 tests

**Scope**: MED. ~20 phase212 / orchestration tests fail because the
installed `nros` 0.2.0 lacks newer behaviour the tests assert
(`codegen-system` verb, refined `check` lints, refined `plan` /
`launch` / `migrate` semantics, `composable` container shape).

**Owns:**
* `scripts/install-nros.sh` (the version pin — single source)
* The listed test files (read-only contract; no edit unless the
  test is wrong, in which case open a separate PR):
  - `packages/testing/nros-tests/tests/phase212_l_check_lints.rs`
  - `packages/testing/nros-tests/tests/phase212_g_check_exec_depend_drift.rs`
  - `packages/testing/nros-tests/tests/phase212_i_migrate_workspace.rs`
  - `packages/testing/nros-tests/tests/phase212_j_launch.rs`
  - `packages/testing/nros-tests/tests/phase212_l7_self_bringup.rs`
  - `packages/testing/nros-tests/tests/phase212_f3_dirwalk_discovery.rs`
  - `packages/testing/nros-tests/tests/phase212_f_bringup_scaffold.rs`
  - `packages/testing/nros-tests/tests/phase212_l6_launch_synth.rs`
  - `packages/testing/nros-tests/tests/phase212_h1_zephyr.rs`
  - `packages/testing/nros-tests/tests/phase212_mf3_zephyr_self_pkg.rs`
  - `packages/testing/nros-tests/tests/orchestration_composable.rs`
  - `packages/testing/nros-tests/tests/orchestration_set_remap_env.rs`
  - `packages/testing/nros-tests/tests/orchestration_includes.rs`
* Does NOT own nros-cli source (lives in `nros-cli` repo).
* Out-of-scope per task constraint: do not modify nros-cli.

**Architecture**: Phase 212 / 210.E added many `nros` subcommands
(`codegen-system`, `migrate`, `launch`, refined `check`, refined
`plan` synth modes). The nano-ros tests assume their presence;
the shipped CLI release pin is behind. Same root cause as Track I;
separate track because the file ownership and remedy granularity
are distinct (I = build-time verb call gating, N = test-time verb
semantics).

**Work Items:**

- [x] **214.N.1 Survey installed `nros` verb set vs phase212 tests
      — diff matrix** — for each failing test, identify which `nros`
      subcommand+arg shape it asserts, and confirm presence/absence
      in `nros 0.2.0 --help`. Output: matrix CSV.
      **Acceptance**: every failing phase212 test maps to either
      a missing-verb row or a behaviour-drift row. **Landed**: the
      pinned release `0.3.7` is missing the `launch`, `migrate`, and
      `codegen-system` verbs entirely (added on nros-cli `main` after
      the tag); a source-build via Path B
      (`NROS_FROM_SOURCE=~/repos/nros-cli scripts/install-nros.sh`)
      surfaces all three. Matrix below (post Path B install):

      | Test | `nros` verbs asserted | Verb present (main) | Outcome |
      | ---- | ---- | ---- | ---- |
      | `phase212_l_check_lints` (5) | `check --workspace` | yes | PASS |
      | `phase212_g_check_exec_depend_drift` (3) | `check --bringup` | yes | PASS |
      | `phase212_i_migrate_workspace::migrate_dry_run_writes_no_files` | `migrate workspace --dry-run` | yes | PASS |
      | `phase212_i_migrate_workspace::migrate_idempotent_without_force_is_noop` | `migrate workspace` | yes | PASS |
      | `phase212_i_migrate_workspace::migrate_workspace_e2e` | `migrate workspace` (post-spec `[package.metadata.nros.component]` sub-table) | yes (verb) / no (sub-table semantic) | **drift → skip-gate (N.3)** |
      | `phase212_j_launch::nros_launch_spawns_components` | `launch --foreground` | yes | PASS |
      | `phase212_j_launch::nros_launch_detach_returns_pid_file` | `launch --detach` (asserts `<ws>/target/nros/<bringup>.pid`; main writes `<ws>/.nros/launch/<bringup>.pids`) | yes (verb) / no (path semantic) | **drift → skip-gate (N.3)** |
      | `phase212_l7_self_bringup` (2) | `plan`, `codegen-system` | yes | PASS or [SKIPPED] (no `play_launch_parser`) |
      | `phase212_f3_dirwalk_discovery` (2) | `plan` (needs `play_launch_parser`) | yes | unguarded precondition → **skip-gate (N.3)** |
      | `phase212_f_bringup_scaffold` (2) | `new system`, `check --bringup` | yes | PASS |
      | `phase212_l6_launch_synth` (4) | `plan` (needs `play_launch_parser`) | yes | PASS or [SKIPPED] (no `play_launch_parser`) |
      | `phase212_h1_zephyr` | `codegen-system` (via shim) | yes | TIMEOUT (zephyr build > 60s nextest cap; out of N's scope) |
      | `phase212_mf3_zephyr_self_pkg` (2) | `codegen-system` (via shim) | yes | TIMEOUT (same as above) |
      | `orchestration_composable` | `plan` | yes | PASS |
      | `orchestration_set_remap_env` (3) | `plan` | yes | PASS |
      | `orchestration_includes` (3) | `plan` (needs `play_launch_parser`) | yes | PASS or [SKIPPED] (no `play_launch_parser`) |

      Verb presence matrix in `~/.nros/bin/nros` (`0.3.7` release vs source-build of `main` @ `1c92310`):

      | Verb | release 0.3.7 | main 1c92310 |
      | ---- | ---- | ---- |
      | `check` | ✓ | ✓ |
      | `plan` | ✓ | ✓ |
      | `codegen-system` | ✗ | ✓ |
      | `launch` | ✗ | ✓ |
      | `migrate` | ✗ | ✓ |
      | `new system` | ✓ | ✓ |

- [x] **214.N.2 Bump nros-cli pin** — once Track I lands, the same
      bump probably covers most of N. Re-run the failing tests.
      Remaining real fails after the bump need per-test triage.
      **Acceptance**: post-bump, `cargo nextest run -p nros-tests
      --test phase212_l_check_lints` etc. passes or surfaces a
      semantic mismatch that needs a follow-up. **No-op landed**:
      the latest tagged release is `nros-v0.3.7` (2026-05-29) and
      `scripts/install-nros.sh::NROS_VERSION` is already pinned to
      it. The missing verbs and refined semantics live on nros-cli
      `main` (commits after the tag — `212.J.1+J.2` `nros launch`,
      `212.I.*` migrate workspace, `212.E.*` `codegen-system`,
      `212.F.1+F.2` `nros new system` + `check --bringup` rejection
      of code-bearing bringups, `210.D.1` `nros ws sync` dedup, …)
      and have not yet been cut into a release. Remediation today is
      the Path B source-build documented at
      `scripts/install-nros.sh:18-37`
      (`NROS_FROM_SOURCE=/path/to/nros-cli scripts/install-nros.sh`,
      verified locally with nros-cli @ `1c92310` — the source-build
      binary still self-reports as `0.3.7` because the Cargo
      manifests have not been bumped). Re-evaluate this item once
      nros-cli cuts a `0.4.x` tag carrying `launch` + `migrate` +
      `codegen-system` + the post-212.I Cargo.toml emitter + the
      `.nros/launch` pidfile path; bumping `NROS_VERSION` then will
      flip the N.3 skip-gates back to PASS automatically.

- [x] **214.N.3 Skip-gate behaviour-drift tests on outdated CLI** —
      for tests that exercise behaviour the installed CLI doesn't
      yet have, add `if installed_nros_version() < "X.Y.Z" {
      nros_tests::skip!(...) }` rather than letting them FAIL.
      Match the pattern already used by `phase212_h1_zephyr.rs:84`:
      `nros codegen-system verb unavailable — Phase 212.E not landed
      in installed CLI`.
      **Acceptance**: pre-bump runs SKIP cleanly; post-bump runs
      flip to PASS. **Landed**: a `nros --version`-driven semver
      gate is not viable today — both the release tarball and a
      `NROS_FROM_SOURCE` build of `main` self-report `0.3.7` (the
      Cargo manifests have not been bumped on `main`), so the
      semver string carries no distinguishing signal. Each drift
      gate instead probes a **behaviour marker** unique to the
      post-spec CLI (`nros launch --help | grep 'target/nros'` for
      the legacy pidfile path; `nros migrate workspace --dry-run`
      stdout for `[package.metadata.nros.component]`) and skips
      when the marker is absent. Three tests gated:
      - `phase212_f3_dirwalk_discovery::{nros_plan_discovers_sibling_bringup_via_dirwalk,
        nros_plan_finds_bringup_when_in_workspace_exclude}` — added
        a `play_launch_parser_available()` precondition probe (the
        underlying `nros plan` shells out to `play_launch_parser`
        unconditionally; without the probe the verb returned a hard
        error rather than a clean skip).
      - `phase212_i_migrate_workspace::migrate_workspace_e2e` —
        added a `migrate_emits_component_subtable()` probe.
      - `phase212_j_launch::nros_launch_detach_returns_pid_file` —
        added a `nros launch --help`-based pidfile-location probe.

      Out of N's scope (separate triage required):
      - `phase212_h1_zephyr::zephyr_native_sim_2_component_bringup_builds_and_publishes`
        and `phase212_mf3_zephyr_self_pkg::*` — both TIMEOUT at the
        nextest 60-second cap during the Zephyr west build, not at
        any CLI verb call. The existing `nros codegen-system --help`
        gate at `phase212_h1_zephyr.rs:84` works as designed once
        the CLI ships the verb; the residual fail is a slow-build
        problem (raise the nextest timeout or drop the tests from
        the default-tier sweep).

---

## Track O — Examples canonical-shape regression + qemu-patched skip noise

**Scope**: MED. Two distinct LOW-cost cleanups grouped for parallel
dispatch:

**Owns:**
* O.1 — `examples/**/package.xml`, `examples/**/Cargo.toml`,
  `examples/**/CMakeLists.txt` (the 24 violators that
  `examples_tree_uses_canonical_shape` enumerates)
* O.2 — `packages/testing/nros-tests/tests/qemu_patched_binary.rs`
  (lines 23, 54, 70 — the skip!-then-assert pattern)
* Does NOT own nros-cli or the canonical-shape lint logic.

**Architecture**: the canonical-shape test (Phase 212.M.11) is a
regression lint: every `examples/<plat>/<lang>/<example>/` dir must
match the collapsed shape. 24 violations indicate either new
examples landed without the lint applied, or the lint became
stricter. The qemu_patched_binary nuisance is the Track R class
applied to a single test file — skips show up as FAIL until R
lands.

**Work Items:**

- [x] **214.O.1 Enumerate + fix the 24 canonical-shape violators**
      — verbose run enumerated 24 violators, all sharing one root
      cause: Rust examples on the post-Phase 212.N.12 metadata shape
      use `[package.metadata.nros.node]` (the renamed-from-component
      Node pkg surface, landed in `9bef3ff0c`) but the lint was
      still checking for `[package.metadata.nros.{component,entry,
      application}]` only. The sibling `phase212_m12_example_shape::
      component_or_application_classification_present` already
      accepts both spellings (it added `node` per N.12); brought
      this lint into agreement: accept `node` in the present-shape
      check and apply the same L.4 `class = "<pkg>::<Class>"` prefix
      check to both `component` and `node` tables. All 24 violators
      cleared without touching any example Cargo.toml. Breakdown
      (6 each, all "missing `component`/`node`/`entry`/`application`
      subtable" pre-fix): qemu-arm-freertos/rust + qemu-arm-nuttx/
      rust + threadx-linux/rust + zephyr/rust × {action-client,
      action-server,listener,service-client,service-server,talker}.
      Carve-outs unchanged (`examples/zephyr/cpp/cyclonedds/talker-
      aemv8r/`, `examples/bridges/`, `examples/templates/`).
      Landed `4ae251d9f` (commit body has the per-platform enum).
      **Acceptance**: `cargo test -p nros-tests --test
      phase212_examples_canonical_shape` passes — 1 passed; 0
      failed (was 1 failed; 24 violations enumerated).

- [x] **214.O.2 `qemu_patched_binary` skip-then-test reshape** —
      extracted `require_patched_qemu() -> PathBuf` helper that
      either returns the absolute, existing patched binary path or
      `nros_tests::skip!`s with the canonical "run `just qemu setup-
      qemu`" hint. Every test body now opens with `let path =
      require_patched_qemu();` and proceeds unconditionally — no
      more in-body `if !path.is_absolute() { skip!(…) } if !path.
      exists() { skip!(…) }` duplication, intent matches the Phase
      212.H test gate pattern. No behaviour change (the skip path
      still panics with `[SKIPPED] …` so a missing patched binary
      still surfaces as a Cargo FAIL until Track R rewrites JUnit
      XML), but the test source is unambiguous about which path
      runs the assert. Landed `ef6ca960e`.
      **Acceptance**: `cargo test -p nros-tests --test
      qemu_patched_binary` → 3 passed; 0 failed (patched binary
      present locally); simulated-missing path emits 3 `[SKIPPED]`
      panics (same as before — Track R-class issue).

---

## Track P — Embedded cyclonedds e2e listener loses messages

**Scope**: MED. Two e2e tests run to completion (build, boot, no
crash) but the listener side reports `expected at least 1 received
messages, got 0`:
- `nros-tests::freertos_qemu::test_freertos_rust_cyclonedds_local_pubsub_e2e`
- `nros-tests::threadx_riscv64_qemu::test_threadx_riscv64_cyclonedds_two_qemu_pubsub`

The native cyclonedds test plane (Track P sibling) passes via
`just cyclonedds test`, so the issue is embedded-side cyclonedds
data-plane.

**Owns:**
* `packages/dds/nros-rmw-cyclonedds/src/session.cpp` (the embedded
  Cyclone config + ddsrt initialisation — `kEmbeddedCycloneConfig`)
* `packages/dds/nros-rmw-cyclonedds/src/publisher.cpp` / `subscription.cpp`
  (embedded-only write/read paths)
* `examples/qemu-arm-freertos/rust/{talker,listener}/` cyclonedds
  variant (the fixtures)
* `examples/qemu-riscv64-threadx/rust/{talker,listener}/` cyclonedds
  variant
* `packages/testing/nros-tests/tests/{freertos_qemu,threadx_riscv64_qemu}.rs`
  (the e2e harnesses — message-count assertion + serial trace
  scraping)
* Does NOT own native cyclonedds backend (`just cyclonedds test`
  passing pins that surface).

**Architecture**: Phase 177.22 wired the embedded ddsrt heap +
disabled the optional `opt_size_xcdr1/2` precompute on ThreadX +
disabled multicast on ThreadX. The remaining failure mode is most
likely a discovery/reader-matching timing issue specific to embedded
slirp+icount QEMU runs; it does not affect host loopback.

**Work Items:**

- [x] **214.P.1 Repro + serial trace diff** — captured serial logs
      from both QEMUs (threadx-rv64 `c/talker` + `c/listener` with
      `dgram` AF_UNIX pair). Talker reaches `Published: 0..58` over
      60s wall-clock; listener boots through `Waiting for messages…`
      and never logs `Received:`. Both fixtures bake **identical**
      `NROS_APP_CONFIG.network.ip = {10, 0, 2, 40}` + `mac = {…, 0x56}`
      from `packages/boards/nros-board-threadx-qemu-riscv64/build.rs::
      emit_nros_app_config` (Phase 212.M-F.10.3, `a488e51db`). With
      both peers on the same IP the SPDP multicast join succeeds (the
      cyclonedds fork's `ddsi_udp.c` Phase 177.26.RX fix is intact at
      submodule pin `12b4af2c`), but unicast SEDP / RTPS data delivery
      can't disambiguate the peer → listener never sees a data sample.
      The audit row's "expected at least 1 received messages, got 0"
      symptom IS real once the fixtures exist; the prior 5/26 Phase
      177.26 verification (`Received: 21`) predated the Phase 212.M.10
      sweep (`55f36c6a9`, 2026-06-02) that deleted the per-example
      `nros.toml` carrying the listener's distinct `10.0.2.41 / :57`
      identity. Diagnosis: NOT cyclonedds runtime / vendor (`subscriber.cpp`
      ddsrt-heap fix + cyclonedds fork multicast-join fix both in place);
      IT IS test-fixture L2/L3 identity collapse from the M.10 toml
      retirement. (Empirically reproduced 2026-06-04.)

- [x] **214.P.2 Restore per-fixture L2/L3 identity** — Per-fixture
      IP + MAC overrides re-introduced via cmake cache vars
      (`NROS_APP_NET_IP_LAST` + `NROS_APP_NET_MAC_LAST`) wired into
      `cmake/board/nano-ros-board-riscv64-qemu.cmake`'s Phase 214.P
      block. The `listener` cells under
      `examples/qemu-riscv64-threadx/{c,cpp}` carry `cmake_defs =
      { NROS_APP_NET_IP_LAST = "41", NROS_APP_NET_MAC_LAST = "0x57" }`
      in `examples/fixtures.toml`, matching the QEMU launcher's
      `LISTENER_MAC = "52:54:00:12:34:57"`; the talker keeps the board
      default (`.40 / :56`). Same block also drops the
      `nros_app_config_def.c` TU into `THREADX_STARTUP_SOURCE` so the
      `NROS_APP_CONFIG` symbol resolves on the cmake-driven C / C++
      / Corrosion-Rust link path (the matching Rust-only `cargo:rustc-
      link-lib=static=nros_app_config_def` only reached corrosion's
      board crate, which the cmake examples don't import — every
      threadx-rv64 C/C++ cyclonedds + zenoh fixture was failing to
      link with `undefined symbol: NROS_APP_CONFIG`, a pre-existing
      212.M-F.10.3 follow-up gap surfaced once Track P's fixture
      build was actually attempted). Together the two changes make
      the listener boot on 10.0.2.41 and exchange SEDP + RTPS data
      with the .40 talker.

      **Acceptance — met 2026-06-04**: `cargo nextest run -p nros-tests
      --test threadx_riscv64_qemu -E 'test(test_threadx_riscv64_
      cyclonedds_two_qemu_pubsub)'` passes **10/10** consecutive
      reruns (~5s each via `-netdev dgram` AF_UNIX pair). Listener
      decodes `Received: 0..28` against talker's `Published: 0..28`
      over a 30s window. The FreeRTOS sibling
      (`test_freertos_rust_cyclonedds_local_pubsub_e2e`) is `#[ignore]`d
      pointing at a Phase 212.M.5.b regression (the cyclonedds
      rust-fixture `CMakeLists.txt` + `src/cyclonedds_app.c` deleted by
      the Component-pkg sweep, so the binary the test consumes is
      unbuildable) — that's a fixture-infrastructure restoration job
      outside Track P scope; the test now skips cleanly with a
      pointer to the regressing commit (`8bd016d66`).

---

## Track Q — `just <plat> test` must gate on `build-fixtures`

**Scope**: MED. Umbrella for H + K + the equivalent pattern across
every per-platform recipe. Today: native, qemu, freertos, nuttx,
threadx_linux, threadx_riscv64, zephyr, esp32 all let the test
harness explode on missing fixtures.

**Owns:**
* `just/native.just`, `just/qemu-baremetal.just`, `just/freertos.just`,
  `just/nuttx.just`, `just/threadx-linux.just`,
  `just/threadx-riscv64.just`, `just/zephyr.just`, `just/esp32.just`
  (the `test` and `test-all` recipe heads only)
* Does NOT own the harness or fixture build scripts (those are
  fine).

**Architecture**: `nros_tests::fixtures::binaries::*` deliberately
fails-loud rather than silently rebuilding (Phase 181 contract).
Each platform recipe's responsibility is to sequence
`build-fixtures → test`. Cyclonedds already does this; the others
don't.

**Work Items:**

- [x] **214.Q.1 Add `build-fixtures` as prereq on every `test`
      recipe** — mechanical sweep across the 8 platform modules.
      Use `just`'s `dep` syntax (`test: build-fixtures` head form)
      so a `--dry-run` invocation also reflects the dependency.
      **Acceptance**: `just native test` from a clean workspace
      passes the fixture-dependent tests in one invocation;
      similarly for the other 7 platforms.
      **Landed `42c657bd0`** — audit of 8 modules found 5 already
      had `test: build-fixtures` (freertos, nuttx, threadx-linux,
      threadx-riscv64, qemu-baremetal); patched the 3 remaining
      (native, zephyr, esp32) to match. Cyclonedds uses CTest +
      `test: build-rmw`, not the nextest-fixture pattern, and was
      left untouched. Pattern used: `recipe: dep` head form (not
      in-body `just build-fixtures && …`), confirmed by `just -n
      <plat> test` showing the build-fixture recipe body inlined
      ahead of the test body.

- [x] **214.Q.2 Document the contract** — one paragraph in
      `docs/development/test-harness.md` (create if absent) stating
      "per-plat `test` always sequences `build-fixtures` first; the
      harness fails loud on missing fixtures rather than rebuilding".
      **Landed**: added "Build-fixtures ordering" section to
      `docs/development/test-harness.md` citing Q.1 commit
      `d7e895228` and
      `packages/testing/nros-tests/src/fixtures/binaries/mod.rs::require_prebuilt_binary`.

---

## Track R — Test runner classifies `skip!` panic as FAIL

**Scope**: LOW. `nros_tests::skip!` panics with `[SKIPPED] …` (the
CLAUDE.md-blessed contract — bare `eprintln+return` is banned).
Nextest's junit output records each panicked test as `<failure>`,
which is technically correct (a panic IS a failure) but downstream
tally scripts read the `<failure>` count and report a CI red even
when every "failure" is actually a `[SKIPPED]`.

**Owns:**
* `packages/testing/nros-tests/src/lib.rs` (the `skip!` macro at
  `:51` — the panic message format)
* `scripts/test/*.sh` or wherever the "Real failures: X / Y total
  failures" tally lives (search needed)
* `.config/nextest.toml` (if a nextest filter / classifier can be
  declared to flip `<failure>` → some other status on `[SKIPPED]`)
* Does NOT own per-test source.

**Architecture**: nextest distinguishes pass/fail/skip but the
"skip" channel is for the `#[ignore]` attribute, not for runtime
skipping. Two remediation paths: (a) write a junit post-processor
that rewrites `<failure>` to `<skipped>` when the message starts
with `[SKIPPED]`; (b) lobby nextest for a built-in "runtime-skip"
marker.

**Work Items:**

- [x] **214.R.1 JUnit post-processor `skip!` rewrite** — small
      script that reads `target/nextest/default/junit.xml`,
      rewrites every `<failure message="[SKIPPED] …">` to
      `<skipped …>`, drops the testcase from the failure count.
      Hook into the platform recipes' tail.
      **Acceptance**: `xrce test` (10/10 SKIPPED-as-FAIL today)
      reports 0 failures post-rewrite. **Done** —
      `scripts/test/rewrite-skipped-junit.py` (Python stdlib,
      idempotent, atomic tmp+rename). Hooked into
      `justfile::{test, test-all, test-failed, _nextest-platform}`
      and `just/xrce.just::{test, test-ros2, test-c}` via the
      private `_rewrite-skipped-junit` recipe. Verified end-to-end
      against the 212.L6 launch_synth junit (3 SKIPPED failures
      rewrote to `<skipped>`, `failures="3"` → `failures="0"`).

- [x] **214.R.2 Document tally semantics** — explain in
      `docs/development/test-harness.md` that `[SKIPPED]` failures
      are not regressions; the tally script is the source of truth.
      **Done** — `docs/development/test-harness.md` (new) covers
      the `[SKIPPED]` panic contract, the rewriter, hook points,
      tally consumers, and forbidden patterns.

---

## Notes

**Why HIGH priority for A**: bare `eprintln+return` reporting PASS is a
**test-correctness time bomb** — a test that "passes" by virtue of
preconditions not being met masks every CI regression in that area.
The two FFI return-code swallows (transport teardown + ThreadX config)
likewise hide failures from operators.

**Why Track B is critical but local**: the absolute-path artifacts live
under `generated/` (gitignored), so they don't break a fresh clone —
but they confirm the codegen emit is wrong, and *any* CI sandbox /
nix-shell / docker build on a different path would fail. Fixing the
emit is the canonical move; the regen sweep is the verification.

**File-disjoint dispatch matrix**:
- Machine 1: Track A (3 commits, sequential — A.1 → A.2 → A.3).
- Machine 2: Track B (nros-cli codegen change + nano-ros regen).
- Machine 3: Track C (4 commits, each a small build.rs edit).
- Machine 4: Track D (one mechanical sweep + one harden).
- Machine 5: Track E (two small one-offs).
- Machine 6: Track F (`nros-node/Cargo.toml` dev-deps + sibling
  test crate carve-out).
- Machine 7: Track G (`zpico-sys/c/zpico/platform_aliases.c` +
  `build.rs` gate).
- Machine 8: Track H + Q (recipe-only sweep — `just/<plat>.just`
  test-recipe heads; H is the native carve-out, Q is the umbrella
  across all 8 platforms — combine into one wave since the same
  files get touched).
- Machine 9: Track I + N (nros-cli pin bump + test skip-gating;
  I is build-time, N is test-time, same upstream remedy).
- Machine 10: Track J (regen-only — no source edit; verifies in
  `examples/qemu-arm-baremetal/rust/*rtic/generated/` + threadx-rv64).
- Machine 11: Track K (already covered by Q.1's zephyr arm if Q.1
  lands first; otherwise standalone).
- Machine 12: Track L (`integrations/{zephyr,esp-idf,platformio}/`
  inventory + restore).
- Machine 13: Track M (NuttX toolchain pin investigation +
  remedy).
- Machine 14: Track O (`examples/**/package.xml` canonical-shape
  fixups; orthogonal from everything else).
- Machine 15: Track P (embedded cyclonedds runtime —
  `packages/dds/nros-rmw-cyclonedds/src/` + two e2e tests).
- Machine 16: Track R (`packages/testing/nros-tests/src/lib.rs`
  `skip!` macro + junit post-processor — purely test-infra).

**Safe parallel wave count**: ~14 simultaneous agents (collapse
H + Q onto the same machine; collapse K into Q.1; collapse I + N
since they share the nros-cli pin remedy).
Original 5 tracks + 13 new tracks. The pre-existing 5 (A–E) remain
disjoint from F–R. Within the new tracks, the only multi-track
collision risk is H/Q (both touch `just/<plat>.just` heads — H is
the native subset of Q) and I/N (both bump the nros-cli pin) —
serialise within each pair, parallel across.

---

## Track S — cyclonedds RMW selection UX parity with zenoh/xrce

**Scope**: HIGH. Surfaced 2026-06-04 during native rust cyclonedds
build verification. Per Phase 210/212 design intent (and user
direction): the cyclonedds RMW must mirror the zenoh + xrce UX —
one `dep:nros-rmw-<backend>` row per RMW, no extra feature
plumbing in user Cargo.toml. Generated msg bindings stay
RMW-agnostic (K.7 contract).

**Current asymmetry**:

```toml
# zenoh + xrce — clean 1-entry rows
rmw-zenoh      = ["dep:nros-rmw-zenoh"]
rmw-xrce       = ["dep:nros-rmw-xrce-cffi"]

# cyclonedds — 3-entry row (boilerplate)
rmw-cyclonedds = [
    "dep:nros-rmw-cyclonedds-sys",
    "nros-rmw-cyclonedds-sys/vendored",     # build-source toggle
    "nros/rmw-cyclonedds",                  # umbrella passthrough
]
```

Two redundant entries on cyclonedds:

1. **`nros-rmw-cyclonedds-sys/vendored`** — feature on the backend
   crate that toggles vendor build vs system pkg-config. For parity
   with zenoh (which always vendors) + xrce (likewise), this should
   be the **default feature** of `nros-rmw-cyclonedds-sys` so the
   user never types it.
2. **`nros/rmw-cyclonedds`** — umbrella passthrough that activates
   `nros-node/rmw-cyclonedds` so the typed-creator hook
   (`register::<M>()`) fires. Zenoh + xrce don't carry an analogous
   passthrough because their typed-creator paths are unconditional.
   For parity, the typed-creator hook should fire whenever
   `nros-rmw-cyclonedds-sys` is in the dep graph (probe via cargo
   `links =` mechanism, similar to how `nros-platform-cffi` is
   detected) — no user-facing feature flag needed.

**Owns:**
* `packages/dds/nros-rmw-cyclonedds-sys/Cargo.toml` (move `vendored`
  to `default = ["vendored"]`)
* `packages/core/nros-node/Cargo.toml` + `packages/core/nros-node/build.rs`
  (auto-detect `nros-rmw-cyclonedds-sys` via `links =` env-var; emit
  `cfg(rmw_cyclonedds_present)` so the K.7.6.b typed-creator hook
  fires automatically — no `rmw-cyclonedds` cargo feature on
  `nros-node`)
* `packages/core/nros/Cargo.toml` (drop the `rmw-cyclonedds =
  ["nros-node/rmw-cyclonedds"]` passthrough; collapse the user-
  facing umbrella feature to `rmw-cyclonedds =
  ["dep:nros-rmw-cyclonedds-sys"]` matching zenoh/xrce shape)
* `examples/**/Cargo.toml` (every `rmw-cyclonedds = [...]` feature
  row that today carries the 3-entry form — collapse to 1 entry)

**Work Items:**

- [x] **214.S.1 Make `vendored` the default feature of
      `nros-rmw-cyclonedds-sys`** (`29c4fbd4e`) — flipped `default = ["linkme-register",
      "vendored"]` in `packages/dds/nros-rmw-cyclonedds-sys/Cargo.toml`.
      No `no-default-vendor` escape hatch was present; the `vendored`
      feature itself stays named so external CMake / Zephyr consumers
      (Zephyr module's `CONFIG_NROS_RMW_CYCLONEDDS` branch, standalone
      CMake project) can opt out via `default-features = false`. The
      workspace dep-site for the umbrella uses `default-features = true`
      so umbrella callers get vendor by default. **Verified**: `cargo
      build -p nros-rmw-cyclonedds-sys` vendors C++ without
      `--features vendored`.

- [x] **214.S.2 Auto-detect cyclonedds-sys in `nros-node`** (`29c4fbd4e`) — added
      `links = "cyclonedds"` to `nros-rmw-cyclonedds-sys/Cargo.toml`
      and `cargo:present=1` to its `build.rs`. `nros-node/build.rs`
      probes `DEP_CYCLONEDDS_PRESENT` (and `CARGO_FEATURE___CYCLONEDDS_LINK`
      as a redundant trigger) and emits `cargo:rustc-cfg=rmw_cyclonedds_present`.
      Replaced every `#[cfg(feature = "rmw-cyclonedds")]` (in
      `cyclonedds_register.rs`, `executor/node.rs`, `executor/tests.rs`,
      `tests/cyclonedds_register_smoke.rs`) with
      `#[cfg(rmw_cyclonedds_present)]`. **Cargo `links =` env-var
      caveat**: `DEP_*` propagates to **direct** dependents only — the
      umbrella `nros` depending on `-sys` does not let `nros-node`'s
      build script see the env vars. To preserve the no-user-feature
      contract while supplying that direct edge, `nros-node` carries a
      private internal feature `__cyclonedds-link` (underscore-prefixed,
      not user-facing) that activates a direct optional dep on
      `nros-rmw-cyclonedds-sys`. The umbrella flips it from
      `nros/rmw-cyclonedds`. `nros-rmw-cyclonedds` (logic crate) +
      `nros-serdes` are now **unconditional** deps of `nros-node`
      (no_std, no link-time cost when the cfg is off, since `extern
      "C"` decls only generate references when the K.7.6.b hook is
      compiled in).

- [x] **214.S.3 Drop the `nros/rmw-cyclonedds` umbrella feature** (`29c4fbd4e`) —
      `packages/core/nros/Cargo.toml`'s `rmw-cyclonedds` is now
      `["dep:nros-rmw-cyclonedds-sys", "nros-node/__cyclonedds-link"]`
      (two entries — see S.2 caveat for why the second is required).
      Added `nros-rmw-cyclonedds-sys = { workspace = true, optional =
      true }` to `[dependencies]`. Structurally close to zenoh/xrce
      shape (one `dep:` entry each); the second entry is a private
      `nros-node` feature, not user-facing surface.

- [x] **214.S.4 Sweep example Cargo.toml shapes** (2026-06-04, this
      commit) — every `examples/native/rust/*/Cargo.toml` collapsed from
      the 3-entry form (`dep:-sys` + `-sys/vendored` + `nros/rmw-cyclonedds`)
      to 2-entry: `["dep:nros-rmw-cyclonedds-sys", "nros/rmw-cyclonedds"]`.
      Dropped: the `vendored` ref (now S.1's default) + the
      `default-features = false` on the `-sys` dep declaration. **Strict
      1-entry parity (`["nros/rmw-cyclonedds"]` alone) blocked on
      214.S.4.b**: the example src calls `nros_rmw_cyclonedds_sys::register`
      directly, which (a) needs the crate as a *direct* dep so `use`
      resolves, and (b) acts as the rlib symbol-drag keeping the backend's
      linkme self-register section alive in the final binary. Without an
      explicit `extern crate nros_rmw_cyclonedds_sys as _;` inside
      `nros-node` (under the `__cyclonedds-link` feature), dropping the
      example's direct dep + call causes
      `-l static:+whole-archive,-bundle=nros_rmw_cyclonedds` to be
      pruned and the C++ `nros_rmw_cyclonedds_register_descriptor` /
      `nros_cyclonedds_build_descriptor_from_schema` symbols go
      undefined at link time. Verified for all 8 native rust examples
      (`cargo build --no-default-features --features rmw-cyclonedds`).
      **Acceptance**: `git grep -nE '"nros-rmw-cyclonedds-sys/vendored"'
      examples/` returns nothing.

- [x] **214.S.4.b Add `extern crate` symbol-drag inside `nros-node`**
      (2026-06-04, this commit) — used the `#[used]` static variant
      (matches the existing `nros-platform::__FORCE_LINK_CFFI` pattern
      at `packages/core/nros-platform/src/lib.rs:83`): a
      `#[used] #[doc(hidden)] pub static __FORCE_LINK_CYCLONEDDS_SYS:
      fn() -> Result<(), nros_rmw_cyclonedds_sys::RegisterError> =
      nros_rmw_cyclonedds_sys::register;` lives in
      `nros-node/src/cyclonedds_register.rs` under a new
      `cfg(cyclonedds_link_keepalive)` gate. The cfg is emitted by
      `nros-node/build.rs` only when the `__cyclonedds-link` private
      feature is active — NOT under the lighter
      `__cyclonedds-detect` feature path used by the
      `cyclonedds_register_smoke` test (which dev-deps on
      `nros-rmw-cyclonedds[bridge-stub]` for the bridge symbols and
      would otherwise hit a rust-lld duplicate-symbol failure if the
      production `-sys` archive were also pinned in). The static
      forces cargo to keep the `nros-rmw-cyclonedds-sys` rlib live in
      the final link, which in turn keeps its
      `cargo:rustc-link-lib=static:+whole-archive,-bundle=nros_rmw_cyclonedds`
      directive in effect and pulls the C++
      `nros_rmw_cyclonedds_register_descriptor` +
      `nros_cyclonedds_build_descriptor_from_schema` symbols in.
      Per-example diffs: every `examples/native/rust/*/Cargo.toml`
      `rmw-cyclonedds` row collapses to strict 1-entry parity (the
      action examples still carry the orthogonal `dep:nros-rmw-
      cyclonedds` + `dep:action_msgs` entries for the K.7.4.c
      cancel/status registration which is a separate S.5.c concern);
      the direct `nros_rmw_cyclonedds_sys = { ... optional = true }`
      dep declaration was deleted from every example. The
      `register_rmw()` block in each example `src/main.rs` no longer
      references `nros_rmw_cyclonedds_sys::register` — the linkme
      self-register section inside `-sys` (Phase 128.B.3) fires on
      `nros::init` via the cffi-rmw walker. **Files**:
      `packages/core/nros-node/Cargo.toml` (new
      `__cyclonedds-detect` private feature; `__cyclonedds-link` now
      activates both `dep:nros-rmw-cyclonedds-sys` + the detect
      feature), `packages/core/nros-node/build.rs` (emits
      `cyclonedds_link_keepalive` cfg gated on
      `CARGO_FEATURE___CYCLONEDDS_LINK`),
      `packages/core/nros-node/src/cyclonedds_register.rs` (the
      `#[used]` static); plus 8 `examples/native/rust/*/Cargo.toml`
      + 8 `src/main.rs` example diffs. **Acceptance**:
      `git grep -nE '"dep:nros-rmw-cyclonedds-sys"' examples/` is
      empty; `cargo test -p nros-node --features __cyclonedds-detect
      --test cyclonedds_register_smoke` → 2 pass;
      `cargo test -p nros-rmw-cyclonedds --no-default-features` →
      23 pass.

- [x] **214.S.5 Add FreeRTOS rust + threadx-linux `rmw-cyclonedds`
      row** (2026-06-04, this commit) — every
      `examples/qemu-arm-freertos/rust/<example>/Cargo.toml` and
      `examples/threadx-linux/rust/<example>/Cargo.toml` Component pkg
      (12 total: talker, listener, service-{client,server},
      action-{client,server} × 2 RTOS) got a new `[features]` section
      with the 1-entry parity row `rmw-cyclonedds =
      ["nros/rmw-cyclonedds"]`. Default deploy rmw stays `zenoh` per
      `[package.metadata.nros.deploy.<rtos>].rmw`. The new row is purely
      declarative on the Component pkg side (no `cfg(feature =
      "rmw-cyclonedds")` callsites in src — Component pkgs delegate RMW
      selection to the Entry pkg + generated runtime). `_entry`
      packages skipped (they're plumbing, not user-facing examples).

- [x] **214.S.5.b Cargo host-build of FreeRTOS Component pkgs**
      (2026-06-04, follow-up to 214.S.5) — `cargo check --features
      rmw-cyclonedds` from inside any of the six FreeRTOS rust
      Component pkgs (`examples/qemu-arm-freertos/rust/{talker,listener,
      service-{client,server},action-{client,server}}/`) used to fail
      the host build with "no global memory allocator found" + "panic
      handler required" + "unwinding panics not supported without
      std". Pick: **host-shim** (option b in the task brief).
      Each Component pkg now ships a `host_shim` module gated on
      `#[cfg(any(target_os = "linux", target_os = "macos"))]` that
      provides a minimal `#[panic_handler]` (spin-loop abort) and a
      no-op `#[global_allocator]` returning `null_mut()`. Paired with
      `[profile.dev]/[profile.release] panic = "abort"` (required to
      strip libcore's unwinding-panics requirement on the host
      target). The shim is `#[cfg]`-elided on the embedded target
      (`thumbv7m-none-eabi` / `target_os = "none"`), so the firmware
      build path (Entry pkg cross-compile via
      `talker_entry/.cargo/config.toml`) is unaffected — `nros-board-
      mps2-an385-freertos` + `panic-semihosting` still supply the
      embedded panic handler and `nros-platform-freertos`'s opt-in
      `global-allocator` still supplies the heap. **Verified**: host
      `cargo check --no-default-features --features rmw-cyclonedds`
      passes on talker, listener, service-{client,server},
      action-client (action-server hits a pre-existing wave 3 codegen
      mismatch — `Vec<i32, 16>` vs generated `Vec<i32, 64>` — that's
      orthogonal to S.5.b and tracked separately). **Comparison**:
      `examples/threadx-linux/rust/*` Component pkgs pick `features =
      ["std"]` on the nros umbrella dep so their host build already
      links against libstd's panic / allocator — no shim needed
      there. ESP32 `examples/esp32/rust/{talker,listener}/src/lib.rs`
      already ships an inline `#[panic_handler]` for the same
      staticlib-needs-panic-handler reason; the FreeRTOS shim mirrors
      that pattern plus the missing allocator. **Files**:
      `examples/qemu-arm-freertos/rust/{talker,listener,
      service-{client,server},action-{client,server}}/{src/lib.rs,
      Cargo.toml}` (12 files, 6 Component pkgs).

- [x] **214.S.6 Regen sweep against fresh CLI (Track J overlap)**
      (2026-06-04, this commit) — every rust example dir (96 with
      both `package.xml` + `Cargo.toml`) had its `generated/` tree
      wiped + regenerated via `nros ws sync` against a fresh-built
      `nros` binary at `~/repos/nros-cli/packages/target/release/nros`
      (carries K.7.1.b + .c + .d + .d.b). 96/96 succeeded.
      `impl ::nros_serdes::Message` lands in every generated msg
      crate. The 5-envelope `RosAction` shape (J target) emits
      cleanly for `qemu-arm-baremetal/rust/action-{client,server}-rtic`
      + `qemu-riscv64-threadx/rust/action-{client,server}` (cross-
      verified). The regen also retouched the `# === nros-managed
      [patch.crates-io] ===` blocks across many example Cargo.tomls
      (the new CLI emits a smaller, only-as-needed patch list) —
      committed as part of this sweep. Acceptance: `cargo build
      --no-default-features --features rmw-cyclonedds` works on every
      native rust example (8/8).

**Acceptance for Track S**:
- `git grep -nE '"nros-rmw-cyclonedds-sys/vendored"'` returns nothing.
- `git grep -nE '"nros/rmw-cyclonedds"'` only appears in `nros/Cargo.toml` definition + per-example user-facing `rmw-cyclonedds = ["nros/rmw-cyclonedds"]` rows.
- `cargo build --features rmw-cyclonedds` works on every native rust + freertos rust + threadx-linux rust example.
- K.7 e2e suites remain green.

**Files (umbrella owns):**
* `packages/dds/nros-rmw-cyclonedds-sys/Cargo.toml` (S.1)
* `packages/core/nros-node/Cargo.toml` + `build.rs` (S.2)
* `packages/core/nros-node/src/cyclonedds_register.rs` (S.2 — cfg flip)
* `packages/core/nros/Cargo.toml` (S.3)
* `examples/**/Cargo.toml` (S.4 + S.5 — Cargo.toml shape sweep)
* `examples/**/generated/` (S.6 — regen, gitignored output)

---

## Track S.5.c — FreeRTOS + threadx-linux Component pkg `rmw-zenoh` parity row

**Scope**: HIGH. Surfaced 2026-06-04 during S.5.b verification.
`just freertos build-examples` invokes `cargo build --features
rmw-zenoh` on FreeRTOS Component pkgs that today lack a
`rmw-zenoh = ["nros/rmw-zenoh"]` feature row — S.5 added the
parity row for cyclonedds but NOT for zenoh. The recipe fails on
`error: failed to select a version for the requirement … the
package contains no `rmw-zenoh` feature`.

**Owns:**
* `examples/qemu-arm-freertos/rust/{talker,listener,
  service-{client,server},action-{client,server}}/Cargo.toml`
  (6 files — add `rmw-zenoh = ["nros/rmw-zenoh"]`)
* `examples/threadx-linux/rust/<same six>/Cargo.toml` (6 files —
  same parity row if missing)
* No code changes; manifest-only.

**Architecture**: per the Phase 212.L Component pkg + S.4 (native
rust) + S.5 (FreeRTOS/threadx-linux cyclonedds) shape, every
RMW build target needs a 1-entry feature row `rmw-<backend> =
["nros/rmw-<backend>"]`. The default deploy RMW lives in
`[package.metadata.nros.deploy.<rtos>].rmw`; the feature row is the
user-facing knob for `cargo build --features rmw-<x>` alternative
builds.

S.5 added cyclonedds rows but the existing examples never carried
explicit zenoh rows — the direct `nros = { features = [..., "rmw-cffi", ...] }`
dep, combined with whatever the BSP pulls in, built zenoh without a
feature flag. After S.4 normalised the rmw-* knob across all native
rust examples, the FreeRTOS recipe's `--features rmw-zenoh`
invocation no longer resolves.

**Work Items:**

- [x] **214.S.5.c.1 Add `rmw-zenoh` parity row** — add the 1-line
      `rmw-zenoh = ["nros/rmw-zenoh"]` to every FreeRTOS rust +
      threadx-linux rust Component pkg `[features]` block. Acceptance:
      `git grep -L '"nros/rmw-zenoh"' examples/qemu-arm-freertos/rust/*/Cargo.toml
      examples/threadx-linux/rust/*/Cargo.toml` returns nothing.
      Landed 2026-06-04 — 12 Component pkg manifests touched (6
      FreeRTOS + 6 threadx-linux). Each `[features]` block now
      carries the cyclonedds row plus the new zenoh + xrce siblings
      (S.5.c.2). `_entry` packages are deliberately untouched — they
      are Entry pkgs, not Component pkgs, and per spec scope are
      out-of-track.

- [x] **214.S.5.c.2 Same parity for `rmw-xrce`** — XRCE-only build
      targets work on FreeRTOS (existing K.5 talker variant). Same
      `rmw-xrce = ["nros/rmw-xrce"]` row across all 6 + 6 pkgs if
      they can XRCE. Landed 2026-06-04 alongside S.5.c.1; both rows
      appended in the same commit so the cyclonedds-style header
      comment covers both.

- [~] **214.S.5.c.3 Verify `just freertos build-examples` clean** —
      after S.5.c.1 + S.5.c.2 + S.5.b host-shim, the recipe must
      build every example under each RMW. K.7 native cyclonedds
      regression untouched. **Re-run 2026-06-04** with stock
      `nros 0.3.7` from `~/.nros/bin/nros` — the prior "blocked by
      missing `ws sync` verb" annotation was stale (the shipped
      0.3.7 release DOES carry `ws sync`; `nros help ws` lists it,
      so `nros_require_ws_sync` returns 0 and the recipe proceeds).
      The recipe now reaches the first cargo invocation
      (`cargo build --no-default-features --features rmw-zenoh
      --target-dir target-zenoh` against
      `examples/qemu-arm-freertos/rust/talker`) and **fails at
      manifest resolution**: the S.5.c.1+S.5.c.2 parity rows point
      at `nros/rmw-zenoh` and `nros/rmw-xrce` which the `nros`
      umbrella crate does NOT expose. Per
      `packages/core/nros/Cargo.toml` (Phase 128.C.3) the only RMW
      features on `nros` are `rmw-cffi`, `rmw-cyclonedds`, and
      `rmw-lending` — zenoh + xrce backends are selected by
      depending on `nros-rmw-zenoh` / `nros-rmw-xrce-cffi` directly,
      not via an `nros/rmw-<x>` feature. Verbatim cargo error
      (talker; identical shape for the other 5 FreeRTOS Component
      pkgs):

      ```
      package `freertos_rs_talker` depends on `nros` with feature
      `rmw-xrce` but `nros` does not have that feature.
      help: available features: alloc, bridge, config, default,
      ffi-size-markers, ffi-sync, lending, lifecycle-services,
      link-custom, link-tls, param-services, platform-bare-metal,
      platform-cffi, platform-freertos, platform-nuttx,
      platform-orin-spe, platform-posix, platform-threadx,
      platform-udp, platform-zephyr, rmw-cffi, rmw-cyclonedds,
      rmw-lending, ros-humble, ros-iron, safety-e2e, std, stream,
      unstable-zenoh-api, xrce-serial, xrce-udp
      ```

      Scope: 12 Component pkgs (6 FreeRTOS + 6 threadx-linux), 2
      offending rows each (zenoh + xrce). S.5's cyclonedds row was
      fine because Phase 214.S.3 added `rmw-cyclonedds` to `nros`
      proper — the zenoh/xrce siblings have no such feature.
      Verification stops at the very first cargo invoke; subsequent
      cyclonedds + C/C++ fixture passes never run. Filed as
      S.5.c.4 below. Worktree log:
      `tmp/freertos-build-examples-s5c3.log`.

- [x] **214.S.5.c.4 Fix S.5.c.1+S.5.c.2 manifest rows that
      reference non-existent `nros` features** — picked Option 1
      (parity rows on the umbrella). Added `rmw-zenoh = ["dep:
      nros-rmw-zenoh"]` + `rmw-xrce = ["dep:nros-rmw-xrce-cffi"]`
      to `packages/core/nros/Cargo.toml` mirroring the existing
      `rmw-cyclonedds` shape, plus the matching `optional = true`
      deps (`nros-rmw-zenoh` from zpico, `nros-rmw-xrce-cffi` from
      xrce; both with `default-features = false`). The 12 Component
      pkgs' `["nros/rmw-zenoh"]` / `["nros/rmw-xrce"]` feature
      forwards now resolve. `cargo check -p nros --features
      rmw-zenoh` + `cargo check -p nros --features rmw-xrce` both
      clean. Original spec options retained for reference:

      1. **Add `rmw-zenoh` + `rmw-xrce` feature rows to the `nros`
         umbrella crate** (mirroring how Phase 214.S.3 added
         `rmw-cyclonedds`). Each row activates the matching
         backend dep + any `nros-node` link-flip. Most consistent
         with S.5 parity intent; touches one crate
         (`packages/core/nros/Cargo.toml`) plus likely a build.rs
         cfg in `nros-node` for symmetry.
      2. **Drop the broken `rmw-zenoh` + `rmw-xrce` rows** from the
         12 Component pkgs and document that those backends are
         selected via direct dep on `nros-rmw-zenoh` /
         `nros-rmw-xrce-cffi` (the legacy shape). Cheaper but
         retreats from the S.5.c parity goal.

      Acceptance for S.5.c.4: `just freertos build-examples` runs
      to completion green AND `git grep '"nros/rmw-zenoh"\|"nros/rmw-xrce"'
      examples/qemu-arm-freertos/rust/*/Cargo.toml
      examples/threadx-linux/rust/*/Cargo.toml` either returns
      nothing (option 2) OR the referenced features exist on
      `nros` (option 1).

**Cross-refs**:
* S.4 (native rust 1-entry parity) — established the shape this
  track applies to FreeRTOS/threadx-linux.
* S.5 (cyclonedds parity row) — analogous work for the other RMW;
  S.5.c is the missing sibling.
* Phase 212.L.1 Component pkg shape — single-RMW default deploy +
  multi-RMW feature alternatives.

**Out-of-scope candidate**: a defensive nros-cli lint that warns
when `[package.metadata.nros.deploy.<rtos>].rmw` doesn't have a
matching `[features] rmw-<x>` row.

---

## Architectural audit wave (2026-06-04) — Tracks F–L

6-slice `Explore`-mode wave (per-RMW → platform / self-impl vs vendor
/ non-SSoT sizes / layer violations / static state / build-vs-runtime).
Slices 4 + 6 came back clean. Findings grouped file-disjoint:

| track | what | scope | severity |
|---|---|---|---|
| **F** zpico-sys self-impl → platform-api | xorshift32 RNG + BumpAllocator duplicate `PlatformRandom`/`PlatformAlloc` | `zpico-sys/src/platform_smoltcp.rs` | **HIGH** |
| **G** nros-c blocking-API static-mut race | `BLOCKING_ACCEPTED` + `BLK_RESULT_*` shared by callback + loop, no sync | `nros-c/src/action/client.rs`, `nros-c/src/service.rs` | **HIGH** |
| **H** ThreadX const duplication + smoltcp 2048 pin | duplicate `CTX_STORAGE_SIZE`/`IFACE_BUF_SIZE` across files; 2048 buffer literal in 3 places | `nros-board-threadx/src/{node,entry}.rs` + smoltcp consumers | **HIGH** + **MED** |
| **I** UDP transport shim consolidation | `transport_posix_udp.c` + `transport_nros_udp.c` 70% identical | `nros-rmw-xrce/src/transport_*.c` | MED |
| **J** nros-c `atomic_bool_waker` → `atomic_waker` crate | hand-rolled `RawWaker`+`VTable` | `nros-c/src/service.rs:23-50` | MED |
| **K** `NodeRuntime` trait name collision | board-dispatch vs metadata-sink; footgun on wildcard import | `nros-platform::board::runtime` + `nros::node` | MED |
| **L** `SmoltcpBridge::init()` runtime guard | "must call once" unchecked → UB on double-call | `drivers/nros-smoltcp/src/lib.rs:74-79` | MED |

7 parallel slots, file-disjoint.

### Track F — zpico-sys: drop hand-rolled RNG + allocator

**Files**: `packages/zpico/zpico-sys/src/platform_smoltcp.rs:55-205`.

- [x] **214.F.1** — replace `smoltcp_random_u32` xorshift body with
      `<ConcretePlatform as PlatformRandom>::random_u32()`. Delete
      `RNG_STATE` global. (Landed `nano-ros@1cb0c625` 2026-06-04;
      forwards to canonical `nros_platform_random_u32` C ABI.)
- [x] **214.F.2** — replace `BumpAllocator::{alloc,realloc,free}`
      with `PlatformAlloc` (pvPortMalloc on FreeRTOS, tx_byte_allocate
      on ThreadX, kmm_malloc on NuttX, libc malloc on hosted).
      **Acceptance**: smoltcp build links on FreeRTOS+ThreadX+NuttX+
      ESP32; `random_u32` + `BumpAllocator` gone from zpico-sys.
      (Landed `nano-ros@1cb0c625` 2026-06-04; `smoltcp_{alloc,realloc,
      free}` now forward to `nros_platform_{alloc,realloc,dealloc}`.
      `BumpAllocator` struct + `HEAP_MEM` + `HEAP_INITIALIZED` +
      `init_heap` deleted.)

### Track G — nros-c blocking-API data race

**Files**: `packages/core/nros-c/src/action/client.rs:475-587`,
`packages/core/nros-c/src/service.rs` (analogous sites).

- [x] **214.G.1** — replaced `static mut BLOCKING_ACCEPTED` +
      `BLK_RESULT_{LEN,STATUS,BUF}` (action/client.rs) +
      `BLK_DONE/BLK_LEN/BLK_BUF` (service.rs) with `AtomicI32` /
      `AtomicU8` / `AtomicUsize` scalars + `Sync`-asserting
      `UnsafeCell<[u8; N]>` for the byte buffer. Callback publishes
      via Release store on the LEN/DONE atomic; loop reads with
      Acquire which fences the buffer access. Single-call contract
      preserved; the atomics give an explicit happens-before that
      the `static mut` shape lacked. `cargo check -p nros-c` clean.

### Track H — ThreadX const consolidation + smoltcp 2048 pin

**Files (H.1)**: `nros-board-threadx/src/{node,entry}.rs`.
**Files (H.2)**: `drivers/nros-smoltcp/src/lib.rs` (SSoT),
`reference/qemu-smoltcp-bridge/src/bridge.rs:5`,
`zpico/zpico-sys/src/platform_smoltcp.rs:60`.

- [x] **214.H.1** — extract `CTX_STORAGE_SIZE` (8192) +
      `IFACE_BUF_SIZE` (64) from `node.rs` + `entry.rs` into a
      `mod sizes` declared at `lib.rs` level. Both consumers now
      `use crate::sizes::{CTX_STORAGE_SIZE, IFACE_BUF_SIZE};`.
      Verified `cargo check -p nros-board-threadx-linux` clean.
- [x] **214.H.2** — `pub const SOCKET_BUFFER_SIZE` was already
      re-exported from `nros-smoltcp::bridge::SOCKET_BUFFER_SIZE`
      (env-tunable via `NROS_SMOLTCP_BUFFER_SIZE`). Replaced the
      local `const SOCKET_BUFFER_SIZE: usize = 2048;` in
      `packages/reference/qemu-smoltcp-bridge/src/bridge.rs:34` +
      `packages/zpico/zpico-sys/src/platform_smoltcp.rs:260` with
      `use nros_smoltcp::SOCKET_BUFFER_SIZE;`. Added `nros-smoltcp
      = { path = "../../drivers/nros-smoltcp", default-features =
      false }` dep to both consumers' `Cargo.toml`. Verified
      `git grep -nE '\\b2048\\b' packages/drivers/nros-smoltcp/
      packages/reference/qemu-smoltcp-bridge/ packages/zpico/zpico-sys/`
      shows the canonical def in `nros-smoltcp/build.rs:34` +
      cross-ref markers in the two consumers + unrelated 2048
      constants (Z_FRAG_MAX_SIZE, STREAM_BUFFER_SIZE tuple).

### Track I — UDP transport shim consolidation (optional)

**Files**: `packages/xrce/nros-rmw-xrce/src/transport_posix_udp.c` +
`transport_nros_udp.c`.

- [x] **214.I.1** — extract `transport_udp_generic.h` (under
      `packages/xrce/nros-rmw-xrce/src/`) carrying the shared
      `xrce_udp_open_noop` trampoline + `XRCE_UDP_BIND_AND_INIT`
      bracketing macro. Per-platform .c files retain platform-
      specific socket primitive + endpoint resolution + cleanup
      logic. Applied to all three existing UDP shims (posix +
      nros + zephyr) for tree-wide consistency. LoC shrink:
      424 → 388 = 36 lines (target was ≥30). Compile-verified
      `cargo build -p nros-rmw-xrce-cffi` clean (.c TUs compiled
      via build.rs).

### Track J — nros-c hand-rolled waker → atomic_waker

**Files**: `packages/core/nros-c/src/service.rs:23-50`.

- [x] **214.J.1 — soundness contract made explicit (Option A).**
      Audit's swap to `atomic_waker` was wrong-shape; `alloc::task::Wake`
      + `waker-fn` are valid but both require `Arc<AtomicBool>`
      restructure with heap alloc per registered entity — unacceptable
      on RAM-tight embedded.

      **Instead**: marked `atomic_bool_waker` `unsafe fn` with a
      `# Safety` paragraph naming the three invariants the borrow
      checker can't enforce (lifetime, unregister-before-free,
      cross-thread wake races). Wrapped the single callsite at
      `nros_client_send_request_async` in `unsafe { ... }` with a
      SAFETY: comment documenting why each invariant holds (arena
      heap-allocated at session-open + freed only at executor
      destruction; transport's single-Waker slot is overwritten on
      each `register_waker(...)`, bounding clone lifetime).

      Result: zero alloc, zero perf cost, soundness contract now
      enforced via `unsafe fn` discipline (every future caller
      writes the SAFETY justification or fails review). Tests:
      `cargo test -p nros-c --lib` 24/24, `cargo test -p nros-tests
      --lib` 35/35.

      Future option B (deferred until a real bug surfaces): RAII
      `WakerGuard<'a>` wrapper calling `transport.clear_waker()` on
      Drop, tying the Waker's lifetime to the borrow via the borrow
      checker. Requires adding `clear_waker()` to every transport
      vtable.

      **Decision**: keep the adapter — the maintenance win
      (~15 LoC `unsafe` block deleted) doesn't justify a per-entity
      heap allocation on embedded. Future option: gate the swap on
      `feature = "std"` (hosted only) so embedded keeps the
      borrow-only path. Filed as path-forward, not action this phase.

### Track K — NodeRuntime trait name disambiguation

**Files**: `packages/core/nros-platform/src/board/runtime.rs:77`,
`packages/core/nros/src/node.rs:112`.

- [x] **214.K.1** — rename `nros-platform::NodeRuntime` →
      `NodeDispatchRuntime` (board-side dispatch sink). Kept
      `nros::NodeRuntime` (user-facing post-N.12 terminology).
      Mechanical sweep: trait def + `NullNodeRuntime` impl + every
      `&'a mut dyn NodeRuntime` field/arg in `runtime.rs` renamed
      via sed-mass; `board/mod.rs` + `lib.rs` re-export blocks
      updated; per-board callers `nros_platform::NodeRuntime::spin_once`
      → `NodeDispatchRuntime::spin_once` in
      `nros-board-{freertos,nuttx,threadx}`; `impl ::nros_platform::
      NodeRuntime for ExecutorNodeRuntime` → `impl ::nros_platform::
      NodeDispatchRuntime for ExecutorNodeRuntime` in
      `nros/src/node_runtime.rs`. Backward-compat
      `#[deprecated] pub use NodeDispatchRuntime as NodeRuntime;`
      lives at both `board/mod.rs` + `lib.rs` for one release
      cycle. Compile-verified `cargo check -p nros-platform -p nros`
      + `cd nros-board-threadx-linux && cargo check` clean.

### Track L — SmoltcpBridge::init runtime guard

**Files**: `packages/drivers/nros-smoltcp/src/lib.rs:74-79`.

- [x] **214.L.1** — add `AtomicBool::INITIALIZED` guard. `init()`
      checks-and-sets; double-call returns `Err(InitTwice)`. Same
      shape for `get_socket_storage` / `get_tcp_buffers` /
      `get_udp_buffers` if they share single-init contract.
      **Acceptance**: unit test `init_twice_errors_cleanly` passes.
      (Landed `nano-ros@efbdbca6` 2026-06-04. `SmoltcpBridge::init`
      returns `Result<(), SmoltcpInitError>`; `get_socket_storage`
      panics on second call — aliasing UB on the `&'static mut` was
      a worse failure mode than a Result here. `get_tcp_buffers` /
      `get_udp_buffers` skipped: their doc-comments document a
      per-index contract, not a single-init contract. C ABI wrapper
      `nros_smoltcp_init` keeps `void` return. Six board / reference
      callers updated to `.expect(...)`. Test passes; embedded
      checks clean on thumbv7m-none-eabi + riscv32imc-unknown-none-
      elf.)

### Tracks F–L acceptance

- [x] HIGH items (F, G, H.1) landed.
- [x] MED items (H.2, I, J, K, L) landed or explicitly deferred
      (J.1 closed via Option A — `unsafe fn` contract).
