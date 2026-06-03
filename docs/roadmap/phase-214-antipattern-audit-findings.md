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

- [ ] **214.E.1 Orin SPE i32 cast bounds-check** — `nros-board-orin-spe/
      src/lib.rs:285`: `bytes.len() as i32` cast without overflow
      check. Strings are ≤256 bytes in practice so safe today; add
      a `debug_assert!(bytes.len() <= i32::MAX as usize)` or replace
      with `i32::try_from(bytes.len()).expect("string fits i32")`.

- [ ] **214.E.2 Dual-transport `compile_error!` guard** — per slice 8
      audit: board crates enforce ≥1 transport (`ethernet` OR `serial`)
      but allow both ON simultaneously. Per CLAUDE.md Phase 162 policy
      ("≥1 transport required"), the intent is **exactly one**. Add
      `#[cfg(all(feature = "ethernet", feature = "serial"))] compile_error!(
      "...")` to each of 4 board crates: esp32-qemu, mps2-an385,
      stm32f4, esp32 (esp32's pair is wifi/serial — same shape).
      **Acceptance**: `cargo check -p <board> --features "ethernet
      serial"` fails with the guard message.

---

## Acceptance

- [ ] Track A (CRITICAL): all 3 sub-items landed. CI passes, no
      banned test antipattern remaining.
- [ ] Track B: nros-cli codegen relative-path fix shipped + nano-ros
      regen sweep verified.
- [ ] Track C: shared constants extracted; per-file `build.rs` reads
      from the central source.
- [ ] Track D: `# Safety` doc lint passes on board crates +
      `nros-node`.
- [ ] Track E: Orin SPE bounds-check added + dual-transport
      `compile_error!` guards in place.
- [ ] Track F: `just check-workspace-embedded` clean; no dev-dep
      forces `nros-serdes/std` on thumb targets.
- [x] Track G: `just test-unit` workspace cargo-test link succeeds
      without an unguarded `nros_platform_*` alias avalanche.
      (`c7b8c9dc0`)
- [ ] Track H: `just native test` from clean workspace runs without
      "Test fixture binary not prebuilt" cascade.
- [x] Track I: `nros ws sync` available via Path B source-build env-var
      (`NROS_FROM_SOURCE=/path/to/nros-cli scripts/install-nros.sh`); the
      5 caller recipes (freertos, qemu-baremetal, native, zephyr,
      threadx-linux via `fixtures-build.sh`) no longer trip on
      "unrecognized subcommand 'ws'" (2026-06-04). Pin bump to a tagged
      release deferred until nros-cli ships the post-`0.3.7` work
      (210.D.1, 212.E, 212.J, K.7.1.{c,d,d.b}) — maintainer-only.
- [ ] Track J: cached `RosAction` generated trees regen-clean against
      the 8-assoc-type trait.
- [ ] Track K: `just zephyr test` from clean workspace passes the
      26 fixture-dependent tests.
- [ ] Track L: `integrations/{zephyr,esp-idf,platformio}/` shells
      restored or test-gated; no bare-FAIL on missing manifests.
- [ ] Track M: `just nuttx build-fixtures` succeeds on the pinned
      nightly + libc combo.
- [ ] Track N: phase212 / orchestration tests pass or
      explicitly `skip!` against the installed CLI version.
- [ ] Track O: `examples_tree_uses_canonical_shape` passes; the
      24 violators triaged.
- [ ] Track P: both embedded cyclonedds e2e tests receive ≥1
      message over 3 reruns.
- [x] Track Q: every per-platform `test` recipe sequences
      `build-fixtures` first (umbrella for H and K) — `42c657bd0`.
- [ ] Track R: `[SKIPPED]` panics no longer count as
      failures in the tally script's output.
- [ ] Phase doc retired to `archived/` when all checkboxes flip.

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

- [ ] **214.F.1.fallback Path A — recipe excludes** — if Path B
      breaks `just check-workspace` (host build) by making host-only
      crates uncompilable under host targets too (shouldn't happen,
      but verify), revert to adding `--exclude` flags in
      `check-workspace-embedded`. Smaller change; less rigorous.

      **Not needed.** Path B verified clean on host:
      `cargo check -p nros-board-posix` + `cargo check
      -p nros-board-native` both pass; `cargo test -p nros-node
      --features rmw-cyclonedds --lib` keeps its 149 tests green;
      `cargo test -p nros-serdes --lib` keeps its 46 tests green;
      `cargo test -p nros-rmw-cyclonedds --no-default-features`
      keeps the K.7 smoke green.

- [ ] **214.F.2 Address residual dev-dep unification** — once Path B
      removes the dominant leak source, the residual dev-dep leak
      (the doc's original framing) may still trip under different
      feature combos. Re-run the cargo-tree audit AFTER F.1 lands;
      if any std path still shows up, apply the sibling-test-crate
      carve-out the original spec described.
      **Acceptance**: `cargo tree -i nros-serdes …` is std-free.

      **Status after F.1.** Residual `feature "std"` activations
      remain in the `--edges=features` view, all sourced from
      `[dev-dependencies]` on `nros-node` (pulling
      `nros-platform-cffi` with `posix-c-port`) and on
      `nros-rmw-cyclonedds` (the `bridge-stub` feature pulls
      `nros-node`). These are the dev-dep leaks the original
      Track F framing described; they survive Path B because Path B
      only target-gates `[dependencies]`, not `[dev-dependencies]`.
      The sibling-test-crate carve-out the original spec described
      still applies — F.2 remains open.

- [ ] **214.F.3 CI guard against future feature unification regressions**
      — add a smoke test that runs `cargo tree -i nros-serdes
      --target thumbv7em-none-eabihf --no-default-features
      --workspace` and asserts the output is missing the substring
      `feature "std"`. Wire into `just check-workspace-embedded`.

      **Status after F.1.** Still open. The assertion can't go
      green until F.2 closes the dev-dep half of the leak — the
      `cargo tree --edges=features` output above still contains
      `feature "std"` strings sourced from dev-deps. Sequence:
      land F.2 → land F.3 (the guard) → relax to
      `--edges=normal,build` if dev-dep filtering needs a
      different threshold.

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

- [ ] **214.G.2 Test recipe coverage** — add a workspace-level
      `cargo test --no-run --workspace --no-default-features` smoke
      to `just check` so this regression class is caught at check time
      rather than at `test-unit` time.
      **Acceptance**: `just check` rejects an unguarded alias
      reintroduction.

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

- [ ] **214.I.2 Fall-back guard at each callsite** — wrap each
      `nros ws sync` invocation with a guard that probes
      `nros help ws` and emits a `[PREREQ]` skip message naming the
      missing verb if absent, instead of letting the build cascade
      into "unrecognized subcommand 'ws'" noise.
      **Acceptance**: pre-pin run gives one clean diagnostic per
      recipe, not a 50-line cargo stack trace.

- [ ] **214.I.3 Maintainer follow-up: cut a new nros-cli release**
      — once 210.D.1, 212.E, 212.J, K.7.1.{c,d,d.b}, and the post-
      `0.3.7` commits land in a tagged release (likely `0.4.0` given
      the verb-surface growth), bump `NROS_VERSION` in
      `scripts/install-nros.sh` and update the Path B doc note. The
      env-var path stays supported for development iterations.

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

- [ ] **214.J.1 Regen the stale `generated/` trees** — `rm -rf
      examples/qemu-arm-baremetal/rust/*rtic/generated/
      examples/qemu-riscv64-threadx/rust/action-*/generated/`
      followed by `just qemu build` and `just threadx_riscv64 build`.
      Verification only — no source edit.
      **Acceptance**: `grep -nE 'type SendGoalRequest' examples/
      qemu-arm-baremetal/rust/action-server-rtic/generated/
      example_interfaces/src/action/fibonacci.rs` returns a match.

- [ ] **214.J.2 build.rs should check trait surface vs cached
      output** — add a quick generation-stamp check (write a hash of
      the trait surface alongside the generated file; rebuild if
      mismatched). Avoids future silent staleness.
      **Acceptance**: touching the `RosAction` trait forces a
      `generated/` rebuild on next `cargo build` without manual
      `clean-bindings`.

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

- [ ] **214.L.1 Inventory `integrations/` tree** — `find
      integrations/ -maxdepth 2 -type f` vs the contract in the
      three failing tests. Diff against
      `docs/roadmap/archived/phase-139-*.md` to identify whether the
      missing files are deletions or never-shipped.
      **Acceptance**: a written inventory pinned to each test's
      expected files.

- [ ] **214.L.2 Restore or skip-gate** — for each missing shell:
      either restore the manifest files from git history (if a
      deletion) or change the integration test to gate on shell
      presence with `nros_tests::skip!` (if intentionally deferred).
      Do NOT silently drop the test.
      **Acceptance**: `just native test --test integration_zephyr
      --test integration_esp_idf --test integration_platformio`
      either passes or skips with a clear `[SKIPPED]` reason; no
      bare-FAIL.

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

- [ ] **214.M.1 Reproduce + diagnose remedy path** — try
      `rustup install nightly-2026-03-15` (or any commit before the
      hostname commit) and `RUSTC_BOOTSTRAP=1 cargo build`. If that
      passes, the toolchain pin is the lever.
      **Acceptance**: documented remedy with a concrete bump.

- [ ] **214.M.2 Land remedy** — either bump
      `rust-toolchain.toml` to the working nightly OR submit a
      libc PR + carry it via `[patch.crates-io]` until merged.
      **Acceptance**: `just nuttx build-fixtures` succeeds.

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

- [ ] **214.N.2 Bump nros-cli pin** — once Track I lands, the same
      bump probably covers most of N. Re-run the failing tests.
      Remaining real fails after the bump need per-test triage.
      **Acceptance**: post-bump, `cargo nextest run -p nros-tests
      --test phase212_l_check_lints` etc. passes or surfaces a
      semantic mismatch that needs a follow-up.

- [ ] **214.N.3 Skip-gate behaviour-drift tests on outdated CLI** —
      for tests that exercise behaviour the installed CLI doesn't
      yet have, add `if installed_nros_version() < "X.Y.Z" {
      nros_tests::skip!(...) }` rather than letting them FAIL.
      Match the pattern already used by `phase212_h1_zephyr.rs:84`:
      `nros codegen-system verb unavailable — Phase 212.E not landed
      in installed CLI`.
      **Acceptance**: pre-bump runs SKIP cleanly; post-bump runs
      flip to PASS.

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

- [ ] **214.O.1 Enumerate + fix the 24 canonical-shape violators**
      — run the test in verbose mode to dump the violator list, then
      either restructure or document the carve-out (the test allows
      a small list of legitimate exceptions, e.g. `examples/zephyr/
      cpp/cyclonedds/talker-aemv8r/`).
      **Acceptance**: `just native test --test
      phase212_examples_canonical_shape` passes.

- [ ] **214.O.2 `qemu_patched_binary` skip-then-test reshape** —
      restructure the three test bodies so the SDK-missing skip
      happens before any assertion. (They already call
      `nros_tests::skip!` early; the FAIL is because skip!
      itself panics — same R-class issue. May be no-op once R
      lands.)

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

- [ ] **214.P.1 Repro + serial trace diff** — capture serial logs
      from both QEMUs, identify whether the listener cyclone reader
      ever discovers the talker writer (matched-publication
      callback). If discovery is silent, the issue is SPDP/SEDP
      timing.
      **Acceptance**: a captured trace + a one-paragraph diagnosis
      pinned to a sub-system (discovery vs reliability vs
      serialization).

- [ ] **214.P.2 Apply discovery-pause workaround OR fix the
      underlying issue** — depends on P.1 outcome.
      **Acceptance**: both tests publish→receive at least one
      message over 3 reruns.

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

- [ ] **214.Q.2 Document the contract** — one paragraph in
      `docs/development/test-harness.md` (create if absent) stating
      "per-plat `test` always sequences `build-fixtures` first; the
      harness fails loud on missing fixtures rather than rebuilding".

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

- [ ] **214.R.1 JUnit post-processor `skip!` rewrite** — small
      script that reads `target/nextest/default/junit.xml`,
      rewrites every `<failure message="[SKIPPED] …">` to
      `<skipped …>`, drops the testcase from the failure count.
      Hook into the platform recipes' tail.
      **Acceptance**: `xrce test` (10/10 SKIPPED-as-FAIL today)
      reports 0 failures post-rewrite.

- [ ] **214.R.2 Document tally semantics** — explain in
      `docs/development/test-harness.md` that `[SKIPPED]` failures
      are not regressions; the tally script is the source of truth.

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
