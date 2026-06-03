# Phase 213 — Post-Phase-212 Known Issues

**Goal**: Resolve audit-surfaced issues that remain after Phase 212 (+ 210)
closure. Work items are pre-grouped into **tracks** with non-overlapping
file ownership so multiple agents (potentially across machines) can run
in parallel without conflict.

**Status**: LIVE. Created 2026-06-03 from a 3-agent audit wave
(`nros setup` provisioning gaps, `examples/` dir structure, build.rs +
CMake hardcoded paths / stale references).

**Priority**: Medium. Issues degrade fresh-clone setup UX + leave
deprecated cmake fn names without an alias (silent breakage). None
block the v0.4 cut directly — but each is small + green-baseline-
preserving.

**Depends on**: Phase 212 closure (DONE), Phase 210 closure (DONE
modulo 210.E.3 parent flip — pure docs).

---

## Overview

The audit identified 8 actionable issues (1 critical, 2 high, 3 medium,
2 low). Tracks group them by file-ownership disjoint-ness so agents
can be dispatched independently:

| track | what | scope | severity floor |
|---|---|---|---|
| **A — Provisioning gaps** | nros-sdk-index.toml + just recipes | 4 files | CRIT |
| **B — CMake fn rename sweep** | `nano_ros_component_register` + `nano_ros_application` aliases / callsites | 50+ files | HIGH |
| **C — Entry pkg N.9 migration** | 18 Entry pkgs from `build.rs + include!()` → `nros::main!()` | 18 dirs | MED |
| **D — Fixture cleanup** | 6-level walk-up in PX4 fixture | 1 file | LOW |

Track A is solo / inline (small enough; one machine fixes all 4).
Tracks B + C are independent mechanical sweeps — different
machines can grab them. Track D is solo / inline.

---

## Architecture

**Discovery method**: Three parallel `Explore`-mode agents ran on
`main` @ `d468a0a3e` (nano-ros) / `c482804` (nros-cli). Each had a
disjoint slice (setup / examples / build.rs+CMake). Findings
cross-corroborated where applicable — e.g. tracks A's
`qemu-arm-baremetal` gap was confirmed both by the setup audit
(index missing) and by the examples-structure audit (board crate
exists but no provisioner entry).

**Non-findings** (audit ✅): no `nros.toml` files, no
`[package.metadata.nros.component]` keys, no `nros::component!()`
invocations, no `__nros_component_*` extern symbols, no
`freertos-qemu-mps2-an385-bsp` references, no `find_package(NanoRos)`,
no `system.toml` outside bringup pkgs, all Rust examples standalone
w/ empty `[workspace]`, coverage matrix matches filesystem, all
`*_DIR` env vars resolve through `sdk-env.just`, all submodule pins
match `.gitmodules`. Phase 212 cleanup landed cleanly; tracks below
are the residual edge cases.

---

## Track A — Provisioning gaps

**Scope**: `nros-sdk-index.toml` + `just/*.just` setup recipes. Single
agent / solo inline; small.

**Files**: `nros-sdk-index.toml`, `just/stm32f4.just`,
`just/qemu-baremetal.just` (or new `just/mps2-an385.just`), maybe
`just/esp32.just` (doc comment).

- [x] **213.A.1** — Audit was wrong; `[board.qemu-arm-baremetal]`
      entry already exists in `nros-sdk-index.toml:406-409` with
      `packages = ["arm-none-eabi-gcc", "qemu"]`. The board IS
      provisionable via `nros setup qemu-arm-baremetal` and via
      `just qemu setup`. Closed 2026-06-03 audit verification.

- [x] **213.A.2** — Add `setup` target to `just/stm32f4.just`.
      Currently the recipe carries `build-fixtures` only; users typing
      `just stm32f4 setup` get "recipe not found". Mirror the shape
      every other platform recipe uses (delegates to
      `nros setup stm32f4`).
      **Acceptance**: `just stm32f4 setup` runs to completion against
      the `[board.stm32f4]` index entry.

- [x] **213.A.3** — Added doc note to `just/qemu-baremetal.just`
      header explaining it covers BOTH `qemu-arm-baremetal` AND
      `mps2-an385` (same SDK packages 1:1 except `openocd` for
      real-hw flashing, which is an out-of-tree concern). Folded
      under the existing recipe rather than splitting — the recipe
      already runs `nros setup qemu-arm-baremetal`; users targeting
      mps2-an385 hw run that + `nros setup mps2-an385 --tool openocd`
      separately.

- [x] **213.A.4** — Document in `just/esp32.just` (header comment)
      that `just esp32 setup` covers both `esp32` (real hardware) AND
      `qemu-esp32-baremetal` boards — OR split into two recipes. Pick
      the option that matches the existing `[board.qemu-esp32-baremetal]
      packages = []` declaration (it's empty → fold under esp32).
      **Acceptance**: a user landing on the `qemu-esp32-baremetal`
      example tree finds a one-line pointer to `just esp32 setup` in
      the platform setup table (or the recipe sibling exists).

---

## Track B — CMake fn rename sweep

**Scope**: `nano_ros_component_register` is called by 32 examples + 2
tests but **defined nowhere** (CRITICAL bug surfaced by audit; affected
cmake configure-time failure on every embedded C/C++ example). The N.12
rename retired `Component` → `Node` family but missed the cmake fn name.
Plus `nano_ros_application` (renamed to `nano_ros_entry` per N.6) is
still called in 18 native examples — the N.6 deprecation shim works,
but should sweep.

Two sub-tracks; do **B.1 FIRST** (alias adds a definition; sweep follows).

**Files (B.1)**: `cmake/NanoRosNodeRegister.cmake`.

**Files (B.2)**: `examples/{qemu-arm-freertos,qemu-arm-nuttx,
threadx-linux}/{c,cpp}/*/CMakeLists.txt` (32 files) +
`examples/native/{c,cpp}/*/CMakeLists.txt` (18 files) +
`packages/testing/nros-tests/tests/phase212_l9_cmake_fns.rs` (2 test
bodies) +
`packages/testing/nros-tests/tests/phase212_h7_px4.rs` (1 comment) +
`packages/testing/nros-tests/tests/phase212_d_workspace_metadata.rs`
(1 comment).

- [x] **213.B.1** — **HIGH PRIORITY (configure-time bug).** Add a
      `nano_ros_component_register(...)` deprecation shim in
      `cmake/NanoRosNodeRegister.cmake` mirroring the existing
      `nano_ros_application` shim at line 138. Body:
      ```cmake
      function(nano_ros_component_register)
          message(DEPRECATION
              "nano_ros_component_register is renamed to "
              "nano_ros_node_register — use nano_ros_node_register(...) "
              "instead. The shim will be retired in a future release.")
          nano_ros_node_register(${ARGV})
      endfunction()
      ```
      **Acceptance**: a fresh `cmake -B build` on
      `examples/qemu-arm-freertos/c/talker/` configures clean
      (currently fails on the undefined fn).
      **Landed `228ce61d8`** — shim added adjacent to the
      `nano_ros_application` shim with matching header-comment entry;
      `cargo test -p nros-tests --test phase212_l9_cmake_fns` 4/4
      pass (the test suite exercises `nano_ros_component_register`
      end-to-end through the cmake module), `phase212_d_workspace_
      metadata` 3/3 pass, `phase212_m12_example_shape` +
      `phase212_pre_212_files_forbidden` 9/9 pass. 213.B.2 caller
      sweep retires every callsite next; the shim stays as a one-
      release safety net.

- [x] **213.B.2** — Mechanical sweep: rename callsites
      `nano_ros_component_register(...)` → `nano_ros_node_register(...)`
      across 32 example `CMakeLists.txt` + 2 test bodies. After this
      lands, the B.1 deprecation shim is unused but stays as a one-
      release safety net (drop in a future phase). Same sweep for
      `nano_ros_application(...)` → `nano_ros_entry(...)` in 18 native
      C/C++ examples. **Done 2026-06-03**:
      - **B.2.a** `nano_ros_component_register` → `nano_ros_node_register`
        landed in `16573b430` (33 callsites: 30 example CMakeLists +
        3 test bodies). Note: phase doc said "32 + 2" but actual was
        30 + 3 — `examples/threadx-linux/c/*` has zero callsites
        (different binding pattern), and the test mention count was
        across 3 files (`phase212_l9_cmake_fns.rs` +
        `phase212_d_workspace_metadata.rs` + `phase212_h7_px4.rs`),
        not 2.
      - **B.2.b** `nano_ros_application` → `nano_ros_entry` landed in
        `b4ac2c7d2` (18 native CMakeLists: 10 native/c + 8 native/cpp).
      - **B.2.c** Talker doc-comment cleanup landed in this commit —
        two leftover narrative refs to `nano_ros_component_register`
        in `examples/native/{c,cpp}/talker/CMakeLists.txt` comment
        blocks renamed to the new fn name (callsites already used
        `nano_ros_entry`).
      **Acceptance verified**: `git grep nano_ros_component_register`
      now returns only the deprecation shim in cmake + roadmap doc
      refs (zero example/test callsites). `git grep
      nano_ros_application examples/native/` returns zero callsites.
      Lint quartet (L.9 + D + M.12 + pre-212-forbidden) stays green.

---

## Track C — Entry pkg N.9 migration

**Scope**: 18 wave-4 Entry pkgs ship the legacy `build.rs +
include!(env!("OUT_DIR")/run_plan.rs)` shape (intentional pre-N.9
stub). N.9 `nros::main!()` proc-macro landed at `fde60cbf6`. The
Entry pkgs should migrate to the one-line shape — drops
`nros-build` build-dep, drops `build.rs`, collapses `main.rs`.

**Files**: `examples/{qemu-arm-freertos,qemu-arm-nuttx,threadx-linux}/
rust/<example>_entry/` (18 dirs × 3 files each = ~54 files).

Per-Entry-pkg edits:
1. `Cargo.toml`: drop `[build-dependencies] nros-build = { ... }`. Add
   `nros = { path = "..." }` to `[dependencies]` if not already there.
2. Delete `build.rs`.
3. `src/main.rs`: replace 10-line `include!` + `<Board as
   BoardEntry>::run` body with one line: `nros::main!();`. The macro
   reads `[package.metadata.nros.entry] deploy = "<board>"` from the
   pkg's own Cargo.toml.
4. Optional sibling `src/lib.rs` (Form 1 pattern) if the Entry pkg
   self-bringups a single Node — declare an `ExecutableNode` impl +
   `nros::node!(...)` per the entry-poc reference shape.

Each Entry pkg dir is **independent** — different machines can grab
different subsets. Suggested partition (3 machines, 6 pkgs each):

- Slot A: `examples/qemu-arm-freertos/rust/*_entry/` (6 pkgs)
- Slot B: `examples/qemu-arm-nuttx/rust/*_entry/` (6 pkgs)
- Slot C: `examples/threadx-linux/rust/*_entry/` (6 pkgs)

- [x] **213.C.1** — Migrated FreeRTOS Entry pkgs (commit `cf2585793`
      + follow-up adds sibling `lib.rs` re-export).

- [x] **213.C.2** — Migrated NuttX Entry pkgs (commit `70098e716`
      + drive-by macro fix `QemuArmNuttx` → `QemuArmVirt` + follow-up
      adds sibling `lib.rs` re-export).

- [x] **213.C.3** — Migrated threadx-linux Entry pkgs (commit
      `5d7725bdb` — already shipped sibling `lib.rs`).

- [x] **213.C.4** — Book chapter `book/src/user-guide/
      component-and-entry-pkg.md` already documents the N.9 macro
      shape (landed with Phase 212.N.9); confirmed verbatim at the
      213.C close. `build.rs + include!()` is documented as escape
      hatch only.

- [x] **213.C follow-up: macro no_std-safe emit** — `nros::main!()`
      emit pre-fix referenced `std::eprintln!` + `std::process::exit`
      which broke `#![no_std]` Entry pkgs (surfaced by C.1 audit).
      Fix: emit splits into two cfg-gated entry shapes —
      `#[cfg(not(target_os = "none"))] fn main()` (hosted) +
      `#[cfg(target_os = "none")] #[unsafe(no_mangle)] pub extern "C"
      fn main() -> i32` (embedded). Shared body factored into
      `__nros_entry_run() -> Result<...>`.

- [x] **213.C follow-up: sibling lib.rs + dep on all 12 wave-4
      Entry pkgs** — `nros::main!()` Form-1 emits
      `::<this_crate>::register(runtime)?`; without a lib target
      exposing `register`, every Entry pkg fails to compile with
      `E0433`. Added `src/lib.rs` re-exporting the sibling Node pkg's
      `register` + the sibling Node pkg as a `[dependencies]` entry
      across the 12 FreeRTOS + NuttX Entry pkgs (C.3's threadx-linux
      slot already shipped this).

---

## Track D — Fixture walk-up cleanup

**Scope**: `packages/testing/nros-tests/fixtures/multi_pkg_workspace_px4/
talker_pkg/CMakeLists.txt:24` includes
`${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../cmake/NanoRosNodeRegister.cmake`
(6-level walk-up). Violates CLAUDE.md "no walk-up paths" policy.

Solo / inline; trivial.

**Files**: `packages/testing/nros-tests/fixtures/multi_pkg_workspace_px4/
talker_pkg/CMakeLists.txt`.

- [x] **213.D.1** — Replace the 6-level walk-up include with an env-
      var-driven path: either pass `-DNANO_ROS_CMAKE_DIR=...` from the
      test driver into the cmake configure, or restructure the fixture
      to use `find_package`-style lookup. **Done 2026-06-03** —
      both `talker_pkg/CMakeLists.txt` and
      `brake_arbiter_pkg/CMakeLists.txt` swapped the 6-level walk-up
      for an `NANO_ROS_CMAKE_DIR` env-var / `-D` shape with a clear
      fatal error if neither is set. Test drivers + manual users
      invoke cmake with `-DNANO_ROS_CMAKE_DIR=<repo>/cmake` or
      `export NANO_ROS_CMAKE_DIR=<repo>/cmake`. Note: H.7 test is
      `#[ignore]`'d (uses `nros codegen-system` rather than direct
      cmake configure), so the test driver itself didn't need an
      update; the fix is forward-compatible for future direct cmake
      invocations.
      **Acceptance verified**: `git grep '\.\./.\.\./.\.\./.\.\./.\.\./\.\.'`
      under `packages/testing/nros-tests/fixtures/` returns ZERO
      results. 4-level + 5-level walk-up patterns also clear.

---

## Acceptance

- [ ] All 4 Track A items landed. Fresh `git clone && just <plat> setup`
      works for every documented board (no "board not found in index"
      error).
- [ ] Track B.1 landed before B.2 (alias provides a definition; sweep
      removes need for it). Embedded C/C++ examples configure clean.
- [ ] All 3 Track C slots merged independently. Wave-4 Entry pkgs
      drop `build.rs`; `main.rs` is one line; `cargo build` clean.
- [ ] Track D fixture include is path-policy compliant.
- [ ] Phase doc retired to `archived/` once all checkboxes flip.

---

## Notes

**Why a separate phase doc?** Audit findings are post-closure leftovers
from 212 + 210. Folding them back into the 212 doc would re-open a
closed phase. Separate doc keeps the closure history clean while still
tracking the work.

**Cross-machine dispatch**: each track is file-disjoint. Multiple
agents on different machines can grab a track without rebasing
conflicts. Internal sub-tracks (e.g. C.1 / C.2 / C.3) are also
disjoint dirs.

**Trade-offs in B.1 vs B.2 ordering**: B.1 alone fixes the configure-
time bug but leaves the deprecation noise. B.2 alone leaves the broken
configure period until both land. Recommendation: **B.1 first** as a
single-commit hotfix; B.2 follows as a mechanical sweep wave (or even
two waves — `nano_ros_component_register` first since it's the broken
one, `nano_ros_application` second since it's purely a doc-cleanup).

**Why not collapse C into one wave?** Cross-compile target diversity
(`thumbv7m-none-eabi` for FreeRTOS, `armv7a-nuttx-eabihf` for NuttX,
host for threadx-linux). One agent per target gives clean verification
runs.

**N.6 + N.12 ledger**: the deprecation shims for
`nano_ros_application` (N.6) and `nano_ros_component_register` (N.12,
once B.1 lands) stay for one release cycle. Future phase retires both
once all callsites + downstream users migrate.

---

## Track E — Hardcoded board config externalisation

**Surfaced 2026-06-03 by a follow-on audit (`Explore`-mode agent on
`bba61e09c`)**. 55 findings across 5 platform/lang slots: per-example
Rust + C source embeds network config literals (MAC, IP, gateway,
zenoh locator, domain_id) that should come from config (nros.toml /
`Cargo.toml [package.metadata.nros.deploy.<target>]` / launch-XML
overlay / env-var). Working escape patterns already exist on two
platforms — they just need to be applied uniformly:

- **Rust embedded** — board crate exposes `Config::from_metadata()`
  reading `[package.metadata.nros.deploy.<target>]` at build-time
  (same pattern N.9 macro uses for `deploy = "<board>"`). Already in
  `esp32/rust/` Cargo.toml metadata.
- **C embedded** — `getenv("NROS_LOCATOR") ?: literal` pattern.
  Already in `native/c/` source verbatim.

**Axis 1** (platform leaks) and **Axis 3** (debug prints) came back
✅ clean — no work items needed.

5 sub-tracks; **file-disjoint by platform/lang**; dispatch in
parallel:

- [ ] **213.E.1 qemu-arm-baremetal/rust** — externalise MAC, IP,
      gateway, locator, domain_id from 10 example source files. Move
      to `[package.metadata.nros.deploy.<target>]` in each example's
      `Cargo.toml`, OR to board-crate-side defaults the example reads
      via `Config::from_metadata()`. Pick whichever matches the
      existing N.9 deploy-key pattern.
      **Files**: `examples/qemu-arm-baremetal/rust/*/src/main.rs`
      (10 files: talker, listener, service-{client,server},
      action-{client,server} + RTIC variants).
      **Acceptance**: `git grep -nE '"tcp/10\.0\.2\.2:|\[10, ?0, ?2,
      ?(2|10)\]|\\b[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:[0-9A-Fa-f]{2}:'
      examples/qemu-arm-baremetal/rust/*/src/` returns no matches.

- [ ] **213.E.2 qemu-esp32-baremetal/rust** — externalise the
      hardcoded `mac_addr` / `ip` / `gateway` / `locator` /
      `domain_id` in 2 example sources. The peer `esp32/rust/`
      already uses metadata for this; copy that pattern.
      **Files**: `examples/qemu-esp32-baremetal/rust/{talker,
      listener}/src/main.rs`.
      **Acceptance**: 2 files have no literal locator / IP / MAC; the
      values come from per-example `Cargo.toml` metadata.

- [ ] **213.E.3 qemu-riscv64-threadx/rust** — externalise locator +
      domain_id from 6 Rust examples (the `lib.rs` / `main.rs` split
      that the threadx-linux/rust path already abstracted via
      `ExecutorConfig::from_env_or(default)`).
      **Files**: `examples/qemu-riscv64-threadx/rust/{talker,
      listener,service-*,action-*}/src/{lib,main}.rs` (6 examples).
      **Acceptance**: `git grep -nE '"tcp/10\.0\.2\.2:75' examples/
      qemu-riscv64-threadx/rust/` returns no matches.

- [ ] **213.E.4 qemu-riscv64-threadx/c** — replace hardcoded
      `nros_support_init("tcp/10.0.2.2:75XX", 0)` with the env-or-
      literal pattern that `native/c/` already uses verbatim:
      ```c
      const char *loc = getenv("NROS_LOCATOR");
      if (!loc) loc = "tcp/10.0.2.2:7553"; /* fixture default */
      int domain = 0;
      if (const char *d = getenv("ROS_DOMAIN_ID")) domain = atoi(d);
      nros_support_init(loc, domain);
      ```
      **Files**: `examples/qemu-riscv64-threadx/c/*/src/main.c`
      (6 files).
      **Acceptance**: same shape as `examples/native/c/talker/src/
      main.c`'s env-fallback block.

- [ ] **213.E.5 threadx-linux/c** — same as 213.E.4 but for
      `tcp/127.0.0.1:75XX` host-loopback defaults.
      **Files**: `examples/threadx-linux/c/*/src/main.c` (6 files).
      **Acceptance**: matches the native/c env-fallback shape.

### Track E acceptance

- [ ] All 5 sub-tracks landed; `git grep -nE '"tcp/(127\.0\.0\.1|10\.
      0\.2\.2):(74|75)[0-9][0-9]"' examples/ | grep -v
      'tests\|fixtures\|generated'` returns ≤ 2 matches (the
      `native/{c,cpp}/` literal fallbacks per the documented escape
      pattern).
- [ ] No regression on existing tests; CI `just test-all` skips on
      unprovisioned SDKs as before.

### Notes

**Why per-platform sub-tracks vs one big sweep**: the Rust path needs
`Config::from_metadata()` plumbing in the board crates that don't have
it yet (qemu-arm-baremetal + qemu-esp32-baremetal + qemu-riscv64-
threadx). The C path is a mechanical `getenv() ?: literal` rewrite.
Splitting by platform/lang gives each agent a coherent scope.

**Out of scope** — Axis 1 (platform leaks) and Axis 3 (debug prints)
came back clean. The 2 esp32 `esp_println!("[poll] ...")` lines are
Phase 127.A expected diagnostics, not noise.
