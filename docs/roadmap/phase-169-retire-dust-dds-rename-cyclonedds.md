# Phase 169 — Retire dust-dds; rename `dds` RMW → `cyclonedds`; complete example matrix

**Goal.** Collapse nano-ros's DDS story onto a single backend —
Cyclone DDS — by retiring the `nros-rmw-dds` (dust-DDS) Rust crate,
renaming the RMW backend identifier `dds` → `cyclonedds` everywhere
it surfaces (features, cmake vars, Kconfig, example tree, docs), and
filling the per-platform × per-language example matrix for the
remaining `cyclonedds` cell. The `nros-rmw-cyclonedds` package stays
C++ (Cyclone DDS's native language; matches the RMW backend
host-language policy frozen 2026-05-07). New examples target
`no_std + no-alloc` where the platform allows it.

**Status.** Not Started.

**Priority.** P1 — closes Phase 117 follow-up + dramatically
shrinks the DDS surface area to one well-supported backend.

**Depends on.**
- Phase 117 Cyclone DDS RMW bring-up (POSIX + Zephyr/cpp landed;
  stock-RMW interop slices 117.X.1–117.X.5 still open — those land
  on top of this rename, not blocked by it).
- Phase 131 examples-tree shape (canonical
  `examples/<plat>/<lang>/<rmw>/<example>/` layout).

---

## Overview

Today the workspace ships two DDS backends:

- **`nros-rmw-dds`** — Rust crate wrapping a vendored `dust-dds`
  submodule (`third-party/dust-dds/`). `no_std + alloc`, embedded-
  friendly on paper, but Phase 117.2h surfaced a hard
  `Actor<DcpsStatusCondition>::poll` deadlock on Xtensa LX7 (tracked
  as Phase 166.F) that blocks ESP32-S3 close-out. Phase 71's
  `DdsRuntime` abstraction was supposed to make dust-dds platform-
  portable; in practice the actor mailbox shape clashes with
  non-reentrant `critical-section` impls and the maintenance cost
  has dominated every recent embedded port.

- **`nros-rmw-cyclonedds`** — C++ wrapper around Eclipse Cyclone
  DDS (`third-party/dds/cyclonedds/` pinned at tag `0.10.5` to
  match `ros-humble-cyclonedds`). Lands the canonical RTPS wire
  format used by the wider ROS 2 ecosystem; full wire-compat with
  stock `rmw_cyclonedds_cpp` is the explicit Phase 117 goal.
  Currently surfaces only in `examples/zephyr/cpp/cyclonedds/`.

Naming gap: every other surface (cargo features, cmake cache vars,
Kconfig values, example-tree directories, book docs) uses bare
`dds` to mean "dust-DDS". Once dust-DDS is gone, `dds` is a stale
identifier — `cyclonedds` is what the backend actually is.

This phase does three things in order:

1. **Rename** `dds` → `cyclonedds` everywhere it surfaces in code,
   build glue, example dirs, and docs. This is mostly mechanical
   but touches enough surfaces that doing it as one atomic phase
   avoids half-renamed states.
2. **Retire** dust-dds: delete the `nros-rmw-dds` +
   `nros-rmw-dds-staticlib` crates, the `third-party/dust-dds/`
   submodule, every feature flag / Cargo dep / test reference.
3. **Complete the matrix**: fill every `<plat>/<lang>/cyclonedds/`
   cell that Cyclone DDS can actually build on, with `no_std +
   no-alloc` examples where the platform / language allow.

The retire step has to come AFTER the rename — running them in
parallel risks leaving the workspace in a state where `dds`
historically meant dust-DDS but cyclonedds doesn't yet answer to
`dds`, which would make every grep / replace pass ambiguous.

---

## Architecture

### Naming after this phase

| Concept             | Before               | After                |
|---------------------|----------------------|----------------------|
| Cargo feature       | `rmw-dds`            | `rmw-cyclonedds`     |
| Cargo crate         | `nros-rmw-dds`       | (deleted)            |
| Cargo crate         | `nros-rmw-dds-staticlib` | (deleted)        |
| Cargo crate         | `nros-rmw-cyclonedds-staticlib` (new) | `nros-rmw-cyclonedds-staticlib` |
| CMake cache var     | `-DNANO_ROS_RMW=dds` | `-DNANO_ROS_RMW=cyclonedds` |
| CMake macro         | `NROS_RMW_DDS=1`     | `NROS_RMW_CYCLONEDDS=1` |
| Kconfig value       | `CONFIG_NROS_RMW="dds"` | `CONFIG_NROS_RMW="cyclonedds"` |
| Example dir         | `examples/<plat>/<lang>/dds/` | `examples/<plat>/<lang>/cyclonedds/` |
| Example matrix col  | `dds`                | `cyclonedds`         |
| Backend host lang   | (dust-DDS = Rust)    | Cyclone DDS = C++ (frozen) |
| RMW enum variant    | `Rmw::Dds`           | `Rmw::CycloneDds`    |
| Submodule           | `third-party/dust-dds/` | (deleted)         |
| Submodule           | `third-party/dds/cyclonedds/` (kept) | `third-party/dds/cyclonedds/` |

### `no_std + no-alloc` policy for new examples

The remaining `cyclonedds` backend is C++ on a C++ DDS stack —
Cyclone DDS itself uses dynamic allocation internally and there's
no path to make THAT alloc-free. The policy applies to the
**example code and the nano-ros wrapper layer**, not to the C++ DDS
core:

- **Rust examples**: declare `#![no_std]`, no `extern crate alloc`,
  use `heapless::{Vec, String}` for any collections, static buffers
  for sample storage. The example app itself never touches `alloc`.
- **C examples**: stack-allocated message structs + fixed-size
  scratch buffers; no `malloc` in the app code (Cyclone DDS may
  allocate internally — that's transparent to the app).
- **C++ examples**: `nros-cpp` is freestanding C++14 with optional
  `std`; new cyclonedds examples target the freestanding mode
  (`NROS_CPP_STD=OFF`), use `nros::Vec`-style fixed-capacity
  containers, no `std::vector` / `std::string` in app code.
- **Wrapper code in `nros-rmw-cyclonedds`** (the package itself,
  not its tests): stays C++14 freestanding-compatible.
  `nros::Result` instead of `std::expected`, fixed-capacity
  containers, no `std::shared_ptr` / `std::unique_ptr` (use
  raw pointers + RAII guards from `nros-cpp`).

Platforms that don't yet support the chosen no-alloc shape (e.g. a
platform whose Cyclone DDS port still pulls in libc heap
unavoidably) document the constraint per-cell in
`examples/README.md` "Intentionally empty cells" — same shape as
Phase 118 / 131 used.

### Backend host-language policy update

`book/src/internals/rmw-backends.md` (RMW backend host-language
policy, frozen 2026-05-07) currently records:

> dust-dds=Rust, cyclonedds=C++, XRCE=Rust→C (115.K.2),
> zenoh-pico=Rust (deferred), uORB=Rust (won't-do).

After this phase:

> cyclonedds=C++, XRCE=Rust→C (115.K.2), zenoh-pico=Rust
> (deferred), uORB=Rust (won't-do). [dust-DDS retired Phase 169.]

---

## Work items

### 169.A — Rename `dds` → `cyclonedds` in code surface

Mechanical rename across every non-example reference. Run BEFORE
any deletion so the workspace stays buildable at every step.

- [ ] **169.A.1** Workspace `Cargo.toml`: rename the workspace-
      level `nros-rmw-dds` aliases that point at the staticlib;
      add a new `rmw-cyclonedds` feature group; keep the dust-DDS
      paths intact for now (deletion is step 169.D).
- [ ] **169.A.2** `nros-core` / `nros-node` / `nros`: rename the
      `Rmw::Dds` enum variant to `Rmw::CycloneDds`. Update every
      `match` over the enum.
- [ ] **169.A.3** Root `CMakeLists.txt`: rename the cmake
      `NANO_ROS_RMW=dds` branch → `cyclonedds`. Re-export the
      `NROS_RMW_DDS` C macro as `NROS_RMW_CYCLONEDDS`.
- [ ] **169.A.4** Per-platform integration shells
      (`integrations/{zephyr,esp-idf,nuttx,px4,platformio}/`): grep
      for `dds` Kconfig / yaml / cmake values; rename each.
- [ ] **169.A.5** `book/src/`: update every reference to the
      `dds` RMW identifier. Files touched include
      `internals/rmw-backends.md`, `user-guide/rmw-backends.md`,
      `concepts/comparison-vs-microros.md`, every starter page,
      `reference/build-commands.md`.
- [ ] **169.A.6** Reserve the old `dds` identifier as a hard
      compile-time error for one release: `compile_error!("the
      'rmw-dds' Cargo feature was renamed to 'rmw-cyclonedds' in
      Phase 169 — see docs/roadmap/phase-169-... for details");`
      gated on the old feature name. Same shape for the cmake
      cache-var alias. Remove the alias after one minor version.

**Files (touched).** Every file under the grep
`rmw-dds|rmw_dds|RMW_DDS|NROS_RMW.*dds|nros-rmw-dds` outside
`docs/roadmap/archived/` and `third-party/`.

### 169.B — Rename example-tree `dds` → `cyclonedds`

For each existing `examples/<plat>/<lang>/dds/` directory, decide
whether the example actually targets dust-DDS or whether the
example is platform-agnostic enough to retarget at Cyclone DDS:

- Examples that link `nros-rmw-dds` directly (the Rust dust-DDS
  staticlib) — these get **deleted** in 169.D once Cyclone DDS has
  a matching example.
- Examples that just point at "the DDS backend, whichever it is"
  via cmake / cargo feature — these get **renamed** in place.

- [ ] **169.B.1** Survey every `examples/*/*/dds/` directory.
      Classify: dust-DDS-bound vs backend-agnostic. Output:
      `tmp/phase-169-example-classify.md` table.
- [ ] **169.B.2** For dust-DDS-bound examples (every Rust RTOS DDS
      example today): mark for deletion + matching cyclonedds
      replacement under 169.C.
- [ ] **169.B.3** For backend-agnostic examples (native C / cpp /
      rust DDS, Zephyr-side DDS examples): `git mv
      examples/<plat>/<lang>/dds/ examples/<plat>/<lang>/cyclonedds/`.
      Update each example's per-dir `Cargo.toml` /
      `CMakeLists.txt` to select the cyclonedds backend.
- [ ] **169.B.4** Update `examples/README.md` matrix: drop the
      `dds` column, mark every renamed cell under `cyclonedds`.

### 169.C — Complete the cyclonedds example matrix

Fill every `<plat>/<lang>/cyclonedds/` cell that Cyclone DDS can
build on. Each cell gets the canonical six-example set (talker,
listener, service-{server,client}, action-{server,client}) unless
the platform has a known constraint (Phase 118's empty-cell rule).

Target matrix (after rename + new cells):

| Platform               | Language | cyclonedds cell |
|------------------------|----------|-----------------|
| `native`               | c        | full 6          |
| `native`               | cpp      | full 6          |
| `native`               | rust     | full 6 (via `nros-rmw-cyclonedds-staticlib`) |
| `zephyr`               | c        | full 6          |
| `zephyr`               | cpp      | full 6 + `talker-aemv8r` (existing) |
| `zephyr`               | rust     | full 6 (via staticlib) |
| `threadx-linux`        | c        | full 6          |
| `threadx-linux`        | cpp      | full 6          |
| `threadx-linux`        | rust     | full 6 (via staticlib) |
| `qemu-arm-freertos`    | c        | full 6 (gated on Cyclone DDS FreeRTOS port — Phase 169.C.gate) |
| `qemu-arm-freertos`    | cpp      | full 6 (same gate) |
| `qemu-arm-freertos`    | rust     | full 6 (same gate) |
| `qemu-arm-nuttx`       | c        | full 6 (gated on Cyclone DDS NuttX port) |
| `qemu-arm-nuttx`       | cpp      | full 6 (same gate) |
| `qemu-arm-nuttx`       | rust     | full 6 (same gate) |
| `qemu-riscv64-threadx` | c, cpp, rust | full 6 each (gated on Cyclone DDS NetX-Duo BSD port) |
| `qemu-arm-baremetal`   | rust     | gated — Cyclone DDS needs a POSIX-ish runtime; likely won't fit |
| `qemu-esp32-baremetal` | rust     | same gate as baremetal |
| `esp32`                | rust     | full 6 IF Cyclone DDS esp-hal-compatible port lands (Phase 117 follow-up); otherwise empty cell with documented reason |
| `stm32f4`              | rust     | same gate as baremetal |
| `px4`                  | cpp      | (uORB-only, unchanged) |

- [ ] **169.C.1** **`native` × {c,cpp,rust}** — extend the existing
      native dds examples (3 langs × 6 cases = 18 examples) to
      Cyclone DDS. Native is POSIX so Cyclone DDS works out of
      the box.
- [ ] **169.C.2** **`zephyr` × {c, rust}** — fill the gap left by
      having only `zephyr/cpp/cyclonedds/` today. Cyclone DDS has
      a Zephyr port in upstream tree.
- [ ] **169.C.3** **`threadx-linux` × {c, cpp, rust}** — Cyclone
      DDS via the existing NetX-Duo / NSOS BSD shim
      (`packages/drivers/nsos-netx`).
- [ ] **169.C.4** **`qemu-arm-{freertos, nuttx}` × {c, cpp, rust}**
      — gated on Cyclone DDS RTOS-port viability assessment
      (169.C.gate). If viable, add the 18 cells; if not, mark
      empty with documented reason in the README matrix.
- [ ] **169.C.5** **`qemu-riscv64-threadx` × {c, cpp, rust}** —
      same gate as qemu-arm RTOS cells.
- [ ] **169.C.6** **`esp32` × rust** — gated on esp-hal Cyclone
      DDS port (a real engineering question — Cyclone DDS expects
      a hosted RTOS; esp-hal is bare-metal Rust). Likely empty
      cell.
- [ ] **169.C.gate** **Cyclone DDS RTOS port assessment** — before
      committing to 169.C.4 / 169.C.5 / 169.C.6, spike one cell
      end-to-end (suggested: `qemu-arm-nuttx/c/cyclonedds/talker/`)
      and decide: viable / gated on upstream patch / won't-do.
      Output: gate decision in `tmp/phase-169-rtos-cyclone-gate.md`,
      then update the matrix accordingly. Don't fill all 18 RTOS
      cells without the gate clearing first.

**`no_std + no-alloc` discipline.** Each new Rust example:
`#![no_std]`, `heapless::*` only, static-arena message storage.
Each new C example: no `malloc` in user code, fixed `char[N]`
scratch buffers. Each new C++ example: `NROS_CPP_STD=OFF`,
freestanding C++14 only.

### 169.D — Delete dust-dds + nros-rmw-dds

After 169.A + 169.B + 169.C land, dust-DDS has zero remaining
in-tree consumers. Delete it.

- [ ] **169.D.1** Delete `packages/dds/nros-rmw-dds/` + 
      `packages/dds/nros-rmw-dds-staticlib/` crates.
- [ ] **169.D.2** Delete `third-party/dust-dds/` submodule: remove
      from `.gitmodules`, run `git rm`, drop the gitignore entries.
- [ ] **169.D.3** Remove every workspace member / Cargo dep /
      build alias referencing the deleted crates. Run `cargo
      check` from a clean state; expect zero "unresolved
      dependency" errors.
- [ ] **169.D.4** Delete the dust-DDS-bound examples flagged in
      169.B.2 (every `examples/*/rust/dds/` that hard-links
      `nros-rmw-dds`).
- [ ] **169.D.5** Delete the `compile_error!` aliases from 169.A.6
      after one minor-version release — kept for one release so
      out-of-tree consumers get a clear error rather than a
      missing-feature failure.
- [ ] **169.D.6** Remove dust-DDS specific tests from
      `packages/testing/nros-tests/tests/`:
      `dds_ros2_interop.rs` (rewrite for Cyclone),
      `server_available_e2e.rs` (uses `nros_rmw_dds as _`),
      every test referencing `dust_dds::*`.
- [ ] **169.D.7** Update `book/src/internals/rmw-backends.md` host-
      language policy table — drop the dust-DDS row, leave the
      "retired Phase 169" footnote.

### 169.E — `no_std + no-alloc` audit on `nros-rmw-cyclonedds`

The wrapper package itself (not Cyclone DDS core) is freestanding
C++14 today. Tighten the audit:

- [ ] **169.E.1** Grep `packages/dds/nros-rmw-cyclonedds/` for
      every `std::vector`, `std::string`, `std::shared_ptr`,
      `std::unique_ptr`, `new` / `delete`. Replace with `nros::`
      equivalents or stack-allocated fixed-capacity types where
      possible.
- [ ] **169.E.2** Document remaining `alloc`-touching call sites
      (Cyclone DDS's own API takes `dds_qos_t*` from
      `dds_create_qos()` which `malloc`s internally — that's
      transparent to nano-ros's wrapper but document the
      transitive allocation budget per-platform).
- [ ] **169.E.3** Add a CI check that
      `nros-rmw-cyclonedds` compiles with
      `-fno-exceptions -fno-rtti -fno-threadsafe-statics` on every
      target — same flags Phase 117 already uses, but make the
      assertion explicit.

### 169.F — Acceptance + cleanup

- [ ] **169.F.1** `just ci` clean from root.
- [ ] **169.F.2** `rg -i "dust[ -_]dds|nros[-_]rmw[-_]dds\b"` 
      returns only hits under `docs/roadmap/archived/` (historical)
      and `book/src/changelog.md`-style files (history).
- [ ] **169.F.3** `examples/README.md` matrix updated: `dds` column
      gone, `cyclonedds` column populated per 169.C target.
- [ ] **169.F.4** `book/src/internals/rmw-backends.md` policy table
      updated.
- [ ] **169.F.5** Archive Phase 117 once 117.X.1–117.X.5
      stock-RMW interop slices are done (separate from this
      phase but enabled by the rename).
- [ ] **169.F.6** Archive Phase 166.F — dust-DDS Xtensa actor
      deadlock — as "won't-fix, dust-DDS retired".

---

## Files (touched)

Code:
- `Cargo.toml` (workspace members + aliases)
- `CMakeLists.txt` (NANO_ROS_RMW branch)
- `packages/core/nros/src/rmw.rs` (or wherever `Rmw::Dds` lives)
- `packages/dds/nros-rmw-dds/` (delete)
- `packages/dds/nros-rmw-dds-staticlib/` (delete)
- `packages/dds/nros-rmw-cyclonedds/` (audit; possibly add Rust
  staticlib sibling per 169.C.1 if Rust users need a static
  archive)
- `packages/testing/nros-tests/tests/dds_ros2_interop.rs` (rewrite)
- `packages/testing/nros-tests/tests/server_available_e2e.rs`
  (rewrite)
- `packages/testing/nros-tests/tests/zephyr.rs` (drop the
  `NROS_RMW_DDS` test branch)
- `third-party/dust-dds/` (submodule delete)

Examples (per 169.B + 169.C tables — likely 60-100 directories
moved or created).

Docs:
- `examples/README.md` (matrix)
- `book/src/internals/rmw-backends.md` (host-language policy)
- `book/src/user-guide/rmw-backends.md` (user-facing RMW pick
  guide)
- `book/src/concepts/comparison-vs-microros.md` (drops the
  dust-DDS reference)
- Every starter page that mentions the `dds` RMW option:
  `book/src/getting-started/{freertos,zephyr,native,esp32,
  threadx,bare-metal,integration-*}.md`.

Integrations:
- `integrations/zephyr/Kconfig` (`CONFIG_NROS_RMW` choice)
- `integrations/esp-idf/Kconfig.projbuild`
- `integrations/nuttx/Kconfig`
- `integrations/platformio/library.json`
- `integrations/px4/module-template/CMakeLists.txt`

---

## Acceptance criteria

- [ ] `cargo check --workspace --all-features` clean — no
      `nros-rmw-dds` / `dust-dds` references in the resolved
      graph.
- [ ] `git ls-files | rg "dust|nros-rmw-dds"` returns hits only
      under `docs/roadmap/archived/` (history) and `CHANGELOG`-style
      files.
- [ ] `examples/<plat>/<lang>/cyclonedds/` populated per the
      169.C matrix; every cell either has the canonical 6 examples
      OR an entry in the "Intentionally empty cells" section of
      `examples/README.md` explaining why.
- [ ] `just test-all` passes — every test that previously depended
      on dust-DDS either passes against Cyclone DDS (renamed +
      rewired) or is removed.
- [ ] Every new Rust example declares `#![no_std]` and contains
      no `extern crate alloc` line.
- [ ] Every new C example contains zero `malloc` / `free` in user
      code (Cyclone DDS internal allocation is acceptable).
- [ ] Every new C++ example compiles with `-fno-exceptions -fno-rtti`
      and `NROS_CPP_STD=OFF`.
- [ ] `book/src/internals/rmw-backends.md` host-language policy
      table no longer lists dust-DDS.

---

## Notes

- **Why C++ for `nros-rmw-cyclonedds`, not Rust?** RMW backend
  host-language policy (Phase 117 era): backend's host language
  matches its underlying library's native language unless overridden.
  Cyclone DDS is C++ (the OMG DDS API binding) on top of a C core.
  A Rust adapter is feasible but adds maintenance burden for zero
  capability gain — same wire format, same DCPS semantics, just a
  thicker FFI surface. Rust users consume Cyclone DDS via a
  `nros-rmw-cyclonedds-staticlib` C wrapper (analogous to
  `nros-rmw-zenoh-staticlib`).
- **Why retire dust-DDS now?** Three pressures converge:
  1. Phase 166.F (Xtensa LX7 Actor deadlock) blocks Phase 117
     close-out and the fix path is "rewrite the actor mailbox" or
     "swap critical-section impl" — both are large investments in
     a backend we'd otherwise retire.
  2. Cyclone DDS is the reference DDS for ROS 2 — wire-compat with
     stock `rmw_cyclonedds_cpp` is THE interop goal. dust-DDS
     interop has been "close, with footnotes" for a year.
  3. Maintaining two DDS backends doubles the test matrix +
     security review surface for no capability gain.
- **What about `nros-rmw-dust-dds` as a separate optional
  external crate?** Out of scope. If a downstream wants to keep
  dust-DDS support they can fork pre-169 and maintain it; nano-ros
  itself ships one DDS backend.
- **`no_std + no-alloc` in `nros-rmw-cyclonedds`** is bounded by
  Cyclone DDS's own allocation model. The wrapper crate can be
  alloc-free but Cyclone DDS's `dds_create_qos()`, sample
  allocation, etc. allocate internally — document the per-platform
  allocation budget rather than pretending it's zero.
- **Submodule deletion** (`third-party/dust-dds/`) is the only
  destructive `git rm` in this phase; double-check no
  downstream-fork branches are pinned at that submodule tree.
