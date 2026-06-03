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

Tracks A, C, D, E live in nano-ros. Track B lives in nros-cli (codegen
emit fix) followed by a nano-ros regen sweep.

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
- [ ] Phase doc retired to `archived/` when all checkboxes flip.

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

5 tracks, fully parallel.
