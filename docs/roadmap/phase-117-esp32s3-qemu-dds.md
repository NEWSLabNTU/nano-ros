# Phase 117 — ESP32-S3-QEMU DDS pubsub bring-up

**Goal:** unblock full-RTPS DDS on an ESP32-class chip via **ESP32-S3 + QEMU
+ PSRAM**. Closes Phase 97.4.esp32-qemu (currently `[ ]`) and Phase 101.7
(blocked on ESP32-C3 heap budget) without sacrificing RTPS conformance — i.e.
keep all dust-dds builtin discovery/metatraffic entities (full ROS 2 interop),
trade chip target instead of trimming protocol surface.

**Status:** **In progress 2026-05-19.** 117.0 through 117.5
landed on `phase-117.0-esp32s3-toolchain`. The build path is
green end-to-end (`xtensa-esp32s3-none-elf` talker + listener
link clean under `cargo +esp build --release`). The runtime
path is gated on 117.2b (PSRAM heap region init), so the
`tests/esp32s3_qemu_dds.rs` E2E is `#[ignore]`'d — promote to
default-run once PSRAM lands. 117.6 (docs + CI) partially done
via the build-commands.md update in 117.0.
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

- [x] **117.0 — Toolchain + QEMU smoke check.** (2026-05-19)
      Espressif QEMU's `xtensa-softmmu` target builds + lands as
      `~/.local/bin/qemu-system-xtensa` with `esp32s3` machine
      model. `just esp32 setup-xtensa` (new recipe) drives
      `scripts/esp32/install-espressif-qemu.sh` with
      `NROS_ESP32_QEMU_TARGETS=riscv32,xtensa`. `+esp` rustc
      channel + `xtensa-esp32s3-none-elf` target installed via
      `espup install --targets esp32s3`. Documented in
      `book/src/reference/build-commands.md` (new section).
      **Files:** `scripts/esp32/install-espressif-qemu.sh`,
      `just/esp32.just`, `book/src/reference/build-commands.md`.

- [x] **117.1 — `nros-platform-esp32s3-qemu` crate.** (2026-05-19)
      Mirrors `nros-platform-esp32-qemu` on the Xtensa LX7 side.
      Two structural differences: (1) critical-section uses
      Xtensa `rsil` / `wsr.ps` (PS.INTLEVEL) instead of RISC-V
      `mstatus.MIE`; (2) `dds-heap` HEAP region intended for PSRAM
      via `#[link_section = ".ext_ram.bss"]` (currently a 192 KiB
      internal-SRAM carve-out as the transitional default —
      PSRAM routing gated on 117.2b). Wired into `nros-platform`
      as `platform-esp32s3-qemu`. Builds clean under
      `cargo +esp build --release --target xtensa-esp32s3-none-elf
      -Z build-std=core,alloc`.
      **Files:** `packages/platforms/nros-platform-esp32s3-qemu/`.

- [x] **117.2 — `nros-board-esp32s3-qemu` crate.** (2026-05-19)
      Mirrors `nros-board-esp32-qemu`. esp-hal / esp-backtrace /
      esp-bootloader-esp-idf / esp-println all use `esp32s3`
      feature; OpenETH base addr identical to C3 (verified via
      `third-party/esp32/qemu/include/hw/misc/esp32s3_reg.h:77`,
      added `ESP32S3_BASE` const alias in `openeth-smoltcp`);
      `portable-atomic` keeps default features (LX7 has native
      pointer-CAS); `dds-heap` forwards to platform crate.
      **Files:** `packages/boards/nros-board-esp32s3-qemu/`,
      `packages/drivers/openeth-smoltcp/src/{regs,lib}.rs`.

- [~] **117.2b — PSRAM init plumbing (partial 2026-05-19).** Board
      crate adds the `esp-hal/psram` feature + threads
      `PsramConfig::default()` through `esp_hal::init(Config::
      default().with_psram(...))` under `dds-heap`. After init,
      `psram_raw_parts(&peripherals.PSRAM)` returns
      `(ptr, byte_count)` for the mapped PSRAM region; the board
      currently prints the region info but does NOT register it
      as a global allocator — see 117.2c. Build path is green
      end-to-end through this stage (release builds of both
      talker + listener link clean).

      **Atomic-in-PSRAM caveat (blocks 117.2c).** Per esp-alloc's
      `psram_allocator!` rustdoc, ESP32-S3 atomic instructions
      misbehave on PSRAM-backed addresses. dust-dds places `Arc`
      refcounts (atomics) inside its allocated state, so routing
      the global allocator into PSRAM is NOT safe on real
      hardware. QEMU emulation likely tolerates it, but a
      hardware-deployable path needs Allocator-API surgery: pin
      atomic-bearing types (Arc, Mutex, etc.) to internal SRAM
      via explicit allocator, route bulk byte buffers (sample
      payloads, history caches) to PSRAM. That's 117.2c below.

- [ ] **117.2c — Allocator-API split for atomic-safe DDS on
      ESP32-S3.** Wire `nros-rmw-dds`'s `DcpsDomainParticipant`
      builder (and dust-dds's internal types where ours owns the
      allocation site) to accept a `Allocator` parameter, route
      atomic-bearing types to an SRAM `EspHeap` and bulk-byte
      types to the PSRAM `EspHeap`. Once this lands, drop the
      `#[ignore]` on `tests/esp32s3_qemu_dds.rs` because the
      runtime heap budget stops gating runtime correctness.
      Substantial upstream work — track as a separate sub-phase
      after 117.0–117.6 close.

- [x] **117.3 — DDS talker / listener example crates.** (2026-05-19)
      Cloned from the C3 templates; target triple
      `xtensa-esp32s3-none-elf`, `+esp` toolchain via espup,
      esp-hal `esp32s3` feature, `nros-platform/platform-esp32s3-qemu`,
      no `portable_atomic_unsafe_assume_single_core` (LX7 has
      hardware atomics). Regenerated `std_msgs` +
      `builtin_interfaces` bindings via `cargo nano-ros
      generate-rust`. Both crates excluded from the workspace
      (same shape as the C3 siblings).
      **Files:** `examples/qemu-esp32s3-baremetal/rust/dds/`.

- [x] **117.4 — Test infra: `nros_tests::esp32s3` launcher +
      nextest group.** (2026-05-19) `is_qemu_xtensa_available` /
      `require_qemu_xtensa` probes for `esp32s3` machine model;
      `is_xtensa_esp32s3_target_available` / `require_*` probe for
      `+esp` toolchain via `$HOME/.rustup/toolchains/esp/`;
      `start_esp32s3_qemu_mcast` mirrors the C3 launcher on
      `qemu-system-xtensa -M esp32s3`. Port 7457 in
      `nros_tests::platform::ESP32S3`. Nextest group
      `qemu-esp32s3` (`max-threads = 1`). `justfile` skip-group
      alternation updated.
      **Files:** `packages/testing/nros-tests/src/{esp32s3,lib,platform}.rs`,
      `.config/nextest.toml`, `justfile`.

- [x] **117.5 — `tests/esp32s3_qemu_dds.rs` E2E.** (2026-05-19)
      Talker + listener under `qemu-system-xtensa -M esp32s3
      -nic socket,model=open_eth,mcast=…`, asserts ≥1
      `Received:` line. Marked `#[ignore]` pending 117.2b —
      build-path is green but runtime PSRAM gate still applies.
      Supporting: factored `create_esp32_flash_image` into a
      chip-parameterised `create_esp_flash_image(elf, output,
      chip, flash_size)` so `--chip esp32s3 --flash-size 8mb`
      shares the helper. New fixture builders
      (`build_esp32s3_qemu_dds_{talker,listener,…_flash}`).
      **Files:** `packages/testing/nros-tests/tests/esp32s3_qemu_dds.rs`,
      `packages/testing/nros-tests/src/esp32.rs`,
      `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`.

- [~] **117.6 — Documentation + CI.** *Partial:*
      `book/src/reference/build-commands.md` got an "ESP32 /
      ESP32-S3 QEMU Setup" section in 117.0. `book/src/getting-
      started/esp32.md` rewrite (S3 section + dual-chip toolchain
      flow) is the remaining work. No CI matrix — the project
      doesn't ship one beyond `deploy-book.yml`.
      **Files:** `book/src/...` (remaining).

## Acceptance Criteria

- [ ] `cargo +esp build -p esp32s3-qemu-dds-talker --release` on
      `xtensa-esp32s3-none-elf` succeeds.
- [ ] `cargo +esp build -p esp32s3-qemu-dds-listener --release` on
      `xtensa-esp32s3-none-elf` succeeds.
- [ ] Two-instance ESP32-S3-QEMU talker↔listener E2E ≥80 % delivery
      (same bar as every existing QEMU DDS slice).
- [ ] Phase 97.4.esp32-qemu retargeted in Phase 97 doc → checked.
- [ ] Phase 101 acceptance #5 (E2E ≥80 %) reachable via this slice.
      Phase 101 closed its line by moving the criterion here; this
      phase carries it as the canonical home.
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
