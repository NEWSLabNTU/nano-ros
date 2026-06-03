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

- [ ] **214.A.1 ThreadX FFI return-code surfacing** — `nros_threadx_set_config()`
      is called from both ThreadX board startup.c files with no return
      capture. The C declaration is currently `void`. Two fixes:
      (a) change return type to `int` + propagate errors;
      (b) document the void contract + add an explicit
      `NROS_THREADX_CONFIG_FALLBACK` log if the fn fails internally.
      Pick (a) if the impl can fail meaningfully; (b) if not.
      **Acceptance**: startup.c calls capture the result OR the void
      contract is documented at the fn definition site.

- [ ] **214.A.2 Transport teardown error capture** — `nros-c/src/transport.rs:95,120`:
      ```rust
      let _ = unsafe { nros_rmw::set_custom_transport(None) };
      ```
      Replace both call sites with explicit handling: log on `Err`, OR
      propagate via the function's return code. If best-effort teardown
      is genuinely intended, document with a `// rationale: teardown is
      best-effort; later ops will surface a clean error` comment.
      **Acceptance**: each `let _ =` is replaced by `match`/`if let
      Err` with at least a log emit, OR documented inline.

- [ ] **214.A.3 Test PASS-on-prereq-missing antipattern (BANNED)** —
      `packages/testing/nros-tests/tests/actions.rs:37-38` +
      `services.rs:46-47`:
      ```rust
      eprintln!("[PASS] native-rs-{service|action}-server started successfully");
      return;
      ```
      Per CLAUDE.md "Tests must fail on unmet preconditions… bare
      `eprintln!`+`return` reports PASS — never. `nros_tests::skip!`
      panics with `[SKIPPED]` (OK)." Replace `return;` with
      `nros_tests::skip!("wait_for_output_pattern failed: <reason>")`.
      **Acceptance**: `git grep -nE 'eprintln!\(\"\[PASS\]' packages/testing/`
      returns no matches.

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

- [ ] **214.C.1 Shared timeout constants** — extract `NUTTX_SOCKET_TIMEOUT_MS
      = 5000`, `GENERIC_SOCKET_TIMEOUT_MS = 100`, `NROS_SERVICE_TIMEOUT_MS
      = 30_000` into a shared module (e.g. a `nros-defaults` crate or a
      doc-block at the canonical use site referenced from the others).
      **Acceptance**: `git grep -nE '\b(5000|30_?000)\b' packages/*/build.rs`
      shows ≤1 definition of each.

- [ ] **214.C.2 XRCE MTU 4096 single source** — `packages/xrce/nros-rmw-
      xrce-cffi/build.rs:355-365` repeats `4096` 3 times for UDP/TCP/
      serial. Extract to `const XRCE_TRANSPORT_MTU_DEFAULT: usize =
      4096;` at file top.

- [ ] **214.C.3 Subscription buffer default coordination** — `nros-c/
      build.rs:29` + `nros-rmw-zenoh/build.rs:8-9` both default to
      `1024` for subscription rx buf. Already env-coordinated via
      `NROS_SUBSCRIPTION_BUFFER_SIZE` — add a comment-block at both
      sites pointing at the canonical default doc, OR factor into a
      shared constant.

- [ ] **214.C.4 Action client per-entry formula** — `packages/core/nros-
      node/build.rs:49` has `4480` as the per-`ActionClient` entry
      buffer size. Add an inline comment explaining the breakdown
      (3 × service buf + 3 × rx buf + overhead) so future tweaks know
      what each term represents.

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

- [ ] **214.D.1 `# Safety` doc sweep** — add a `/// # Safety` paragraph
      to each `unsafe fn` site listed above. Each paragraph names the
      invariant the caller must uphold (e.g. "must be called only once
      at startup", "ptr must outlive the returned slice").
      **Acceptance**: `cargo clippy --workspace -- -W clippy::missing_safety_doc`
      reports no warnings on the 6 files above.

- [ ] **214.D.2 ESP32 lifetime transmute hardening** — `nros-board-esp32/
      src/node.rs:162`:
      ```rust
      unsafe { WIFI_DEV.write(core::mem::transmute(wifi_dev)) }
      ```
      transmutes `WifiDevice<'d>` → `WifiDevice<'static>` for static
      storage. Acceptable in bare-metal no-exit context but a footgun.
      Replace with `MaybeUninit<WifiDevice<'static>>` + an
      initialization pattern, OR document the invariant ("the wifi
      device outlives the program — embedded no-exit context") in
      a `// SAFETY:` comment.

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
