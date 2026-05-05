# Phase 117 — ESP32-S3-QEMU DDS pubsub bring-up

**Goal:** unblock full-RTPS DDS on an ESP32-class chip via **ESP32-S3 + QEMU
+ PSRAM**. Closes Phase 97.4.esp32-qemu (currently `[ ]`) and Phase 101.7
(blocked on ESP32-C3 heap budget) without sacrificing RTPS conformance — i.e.
keep all dust-dds builtin discovery/metatraffic entities (full ROS 2 interop),
trade chip target instead of trimming protocol surface.

**Status:** Not Started.
**Priority:** Medium — promotes Phase 97 / 101 from "DDS works on every other
RTOS slice" to "DDS works on ESP32 too." Bonus coverage for the ESP32 line
without requiring real hardware.
**Depends on:** Phase 101 (`portable-atomic-util::Arc` substitution — landed,
acceptance #5 deferred here). Reuses fork branch
`nano-ros/phase-101-portable-atomic`.

## Background

Phase 101 shipped the `Arc` / `Weak` polyfill and `regex` removal that
make dust-dds compile on `riscv32imc` (ESP32-C3). Build path is green;
runtime panics in `DcpsDomainParticipant::new → handle_alloc_error`
because ESP32-C3 has only ~400 KiB SRAM total and **no PSRAM bus**, so
the largest static heap that links is ~192 KiB — well below the
~hundreds-of-KiB dust-dds needs for its ~13 builtin actors + history
caches + mailboxes.

Trimming builtin entities is rejected: Phase 117 maintains **full DDS /
RTPS conformance** so ROS 2 peers (rmw_zenoh_cpp, FastDDS, RTI Connext)
discover and interoperate without surprises. The fix is therefore
chip-side, not protocol-side.

**ESP32-S3** is the right target:

| Chip | SRAM | PSRAM bus | Core | Notes |
|---|---|---|---|---|
| ESP32-C3 | 400 KiB | none | RV32IMC | Phase 101.7 blocker |
| ESP32 (Xtensa) | 520 KiB | up to 8 MiB | LX6 dual | mainline QEMU partial |
| **ESP32-S3** | **512 KiB** | **up to 16 MiB octal** | **LX7 dual** | **QEMU supported via Espressif fork; PSRAM in QEMU** |

ESP32-S3 keeps us in the ESP32 ecosystem, gains PSRAM head-room (8 MiB+
heap available), and runs under `qemu-system-xtensa` (Espressif maintains
ESP32-S3 SoC support; mainline `qemu-system-xtensa` already in `/usr/bin`
on the dev box, but full SoC may need the Espressif build —
investigate in 117.0).

## Architecture

Three orthogonal axes per existing nano-ros convention:

- **Platform:** `platform-bare-metal` (esp-hal-managed bare-metal)
- **RMW:** `rmw-dds` (dust-dds via nros-rmw-dds, with
  `rmw-dds-portable-atomic` toggle inherited from Phase 101)
- **Target:** `xtensa-esp32s3-none-elf` (Xtensa LX7, **needs `espup` /
  `+esp` toolchain channel — not stock rustc**)

Crate plan mirrors ESP32-C3 layout one-for-one:

| ESP32-C3 (existing) | ESP32-S3 (new) |
|---|---|
| `packages/platforms/nros-platform-esp32-qemu/` | `packages/platforms/nros-platform-esp32s3-qemu/` |
| `packages/boards/nros-board-esp32-qemu/` | `packages/boards/nros-board-esp32s3-qemu/` |
| `examples/qemu-esp32-baremetal/rust/dds/{talker,listener}/` | `examples/qemu-esp32s3-baremetal/rust/dds/{talker,listener}/` |
| `nros_tests::esp32::start_esp32_qemu_mcast` | `nros_tests::esp32s3::start_esp32s3_qemu_mcast` |
| nextest group `qemu-esp32` (port 7454) | nextest group `qemu-esp32s3` (new port — pick 7457) |

**PSRAM strategy:** dust-dds heap allocations route to PSRAM region via
esp-alloc's region API. Stack + small fast allocations stay in internal
SRAM. `esp_alloc::heap_allocator!(... HEAP_REGION_DEFAULT)` plus a
PSRAM region init from `esp-hal::psram`.

**Toolchain:** `espup install` lands `xtensa-esp32s3-none-elf` plus a
custom rustc fork (Xtensa support is out-of-tree). CI / contributor
docs need an `espup`-managed install step. `just setup esp32s3` target
to add.

## Work Items

- [ ] **117.0 — Toolchain + QEMU smoke check.**
      Install `espup`, run `espup install` to get `+esp` channel +
      `xtensa-esp32s3-none-elf`. Verify `qemu-system-xtensa` on the dev
      box can boot an ESP32-S3 image (mainline vs. Espressif fork —
      Espressif's `qemu-xtensa` may be needed for full SoC). Document
      QEMU command shape in `book/src/reference/build-commands.md`.
      **Files:** `book/src/reference/build-commands.md`, optionally
      `Justfile` (`just setup esp32s3`).

- [ ] **117.1 — `nros-platform-esp32s3-qemu` crate.**
      Mirror `nros-platform-esp32-qemu`. Differences:
      * `esp-hal` features: `esp32s3` (not `esp32c3`)
      * Xtensa target — link script differences (`memory.x` may need
        update for IRAM/DRAM partitioning + PSRAM region)
      * `critical-section` impl from `esp-hal` (already xtensa-aware)
      * Clock + sleep primitives identical shape; backend differs.
      **Files:** new `packages/platforms/nros-platform-esp32s3-qemu/`.

- [ ] **117.2 — `nros-board-esp32s3-qemu` crate + PSRAM heap init.**
      Mirror `nros-board-esp32-qemu`. New: PSRAM region added to
      `esp-alloc` so dust-dds gets ≥4 MiB heap (target 8 MiB if QEMU
      model supports octal PSRAM). Internal SRAM reserved for stack +
      small allocations.
      **Files:** new `packages/boards/nros-board-esp32s3-qemu/`.
      Plumb `nros::platform-esp32s3-qemu` umbrella feature.

- [ ] **117.3 — DDS talker / listener example crates.**
      Copy `examples/qemu-esp32-baremetal/rust/dds/{talker,listener}/`
      to `examples/qemu-esp32s3-baremetal/rust/dds/{talker,listener}/`.
      Adjust target triple, board crate, esp-hal feature.
      `rmw-dds-portable-atomic` feature kept on (still
      Xtensa-friendly — `portable-atomic` works on any
      target; `critical-section` impl provided by esp-hal).
      **Files:** new `examples/qemu-esp32s3-baremetal/rust/dds/`.

- [ ] **117.4 — Test infra: `nros_tests::esp32s3` launcher + nextest
      group.** Mirror `nros_tests::esp32`. Pick `qemu-system-xtensa
      -M esp32s3 -nic socket,model=open_eth,mcast=…`. New port 7457
      in `nros_tests::platform`. Nextest group `qemu-esp32s3` with
      `max-threads = 1`.
      **Files:** `packages/testing/nros-tests/src/`,
      `.config/nextest.toml`.

- [ ] **117.5 — `tests/esp32s3_qemu_dds.rs` E2E.**
      Modelled on `tests/esp32_qemu_dds.rs`. Runs talker + listener
      under qemu-system-xtensa, asserts ≥80 % delivery — same bar as
      every other QEMU DDS slice.
      **Files:** `packages/testing/nros-tests/tests/esp32s3_qemu_dds.rs`.

- [ ] **117.6 — Documentation + CI.**
      Update `book/src/getting-started/`, `book/src/porting/`,
      reference build-commands. Add `qemu-esp32s3` to CI matrix.
      **Files:** `book/src/...`, GitHub Actions if applicable.

## Acceptance Criteria

- [ ] `cargo +esp build -p esp32s3-qemu-dds-talker --release` on
      `xtensa-esp32s3-none-elf` succeeds.
- [ ] `cargo +esp build -p esp32s3-qemu-dds-listener --release` on
      `xtensa-esp32s3-none-elf` succeeds.
- [ ] Two-instance ESP32-S3-QEMU talker↔listener E2E ≥80 % delivery
      (same bar as every existing QEMU DDS slice).
- [ ] Phase 97.4.esp32-qemu retargeted in Phase 97 doc → checked.
- [ ] Phase 101 acceptance #5 (E2E ≥80 %) reachable via this slice;
      original ESP32-C3 line stays `[blocked]` with cross-link.
- [ ] No regression in any existing QEMU slice — full nextest pass
      including `qemu-esp32` group (must keep ESP32-C3 build-time
      tests green).

## Notes

- **Toolchain caveat:** Xtensa rustc support is out-of-tree
  (`esp-rs/rust` fork). Contributors need `espup install` once. CI
  needs the same. Stable rustc cannot build this crate.
- **QEMU caveat:** mainline `qemu-system-xtensa` may not model
  ESP32-S3 SoC fully. If it doesn't, fall back to Espressif's
  `qemu-xtensa` build (`espressif/qemu` repo). 117.0 settles which
  path. If neither works headlessly, the slice may need real hardware
  — at that point reconsider scope (would split this phase into
  emulator vs hardware bring-up).
- **PSRAM and `portable-atomic-util`:** PSRAM access is slower than
  internal SRAM (~10×). `Arc` refcount atomics live in PSRAM if `Arc`
  itself is allocated there — check perf if discovery latency drags.
  Mitigation: pin discovery actor allocations to internal SRAM via
  esp-alloc region selectors.
- **No protocol changes:** unlike a hypothetical "trim builtin
  entities" fix, Phase 117 keeps full RTPS / DDS-XTypes / type-lookup
  surface. ROS 2 peers see the participant exactly as they would from
  any FastDDS / RTI Connext / dust-dds-on-Linux node.
- **Why not ESP32 (LX6) instead?** ESP32 (original) PSRAM is
  quad-mode, 4 MiB cap practically. ESP32-S3 octal PSRAM up to 16 MiB
  — more head-room, same dev-kit price tier. Also LX7 has better
  rustc support story going forward (esp-rs targets newer LX7).
- **ESP32-C3 stays useful:** for `rmw-zenoh` / `rmw-xrce` (lighter
  footprint). DDS-on-C3 = wrong-tool. Phase 117 closes the DDS
  coverage gap; ESP32-C3 keeps the lighter-RMW slices (Phase 97.3.
  esp32-qemu — already green).
