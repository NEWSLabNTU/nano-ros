# Phase 149 — Board / BSP Abstraction Layer

**Goal.** Stop requiring a hand-written Cargo board crate per
`(vendor × board × SDK-variant)` combo. Carve generic board crates
per RTOS-kernel family (one each for FreeRTOS, ThreadX, NuttX,
bare-metal-cortex-m, bare-metal-cortex-a), plus a small overlay-crate
pattern (~50 LOC) for vendor forks. Combine with Phase 139's per-RTOS
integration shells so vendor BSPs flow through their RTOS's native
package manager rather than through nano-ros code.

**Status.** Not started. Design fixed by
`docs/design/board-bsp-integration-architecture.md` (2026-05-18).

**Priority.** P2 — usability + scalability win. Existing per-board
crates work for the curated demo set; this phase makes nano-ros
consumable on vendor SKUs we never enumerated.

**Depends on.** Phase 136 (manifest parser to reuse), Phase 137
(root CMake entry — landed), Phase 138 (per-platform CMake
consolidation — landed), Phase 139 (per-RTOS shells — landed).

**Related.** Phase 111.B.2 (crates.io name availability — confirmed
13/13 open; community overlay crates can publish freely),
Phase 116 (unified config + extensibility north-star — this phase
delivers the platform side of that vision but punts BSP DSL),
Phase 144 (example migration — overlays land underneath the example
migration's `add_subdirectory` consumers).

---

## Overview

Today's board crates (`nros-board-mps2-an385-freertos`,
`nros-board-threadx-linux`, `nros-board-threadx-qemu-riscv64`,
`nros-board-orin-spe`, `nros-board-nuttx-qemu-arm`) each duplicate:
the kernel build glue (cc-rs invocation, source globbing, port-dir
selection), the platform-provider wiring (`nros-platform-<rtos>`
init), the network-stack hookup (lwIP / NetX-Duo / nsos-netx), and
the `run(config, closure)` entry-point shape. Adding a new board =
copy a sibling crate + ~200 LOC of edits.

This doesn't scale to NXP MCUXpresso (40+ boards), STM32 (50+
families), Espressif (7 chip families × boards), Renesas Synergy,
NVIDIA FSP-FreeRTOS, etc. Vendors won't write Cargo crates per SKU.

Per-RTOS research (captured in
`docs/design/board-bsp-integration-architecture.md`) shows two
patterns vendors actually use:

1. Zephyr's DTS + `west` modules; ESP-IDF's component registry;
   NuttX's `apps/external/`. These RTOSes already have a BSP +
   package-manager story. nano-ros should ride those rails rather
   than re-invent.
2. Stock FreeRTOS + lwIP + a vendor HAL is the "build it yourself"
   path. One generic crate per kernel + tiny overlay crates per
   vendor fork covers this surface.

Phase 149 lands both.

---

## Architecture

See `docs/design/board-bsp-integration-architecture.md` for the
five-layer model + per-RTOS findings + consumption matrix. This
phase's scope = Layers 2 + 2.5 + the Phase 139 follow-ups in
Layer 3.

### A. Generic board crates (Layer 2)

| Crate | Covers | SDK env vars | TOML manifest |
|---|---|---|---|
| `nros-board-freertos` | stock FreeRTOS + lwIP | `FREERTOS_DIR`, `FREERTOS_PORT`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR` | `nros_board_freertos_platforms.toml` |
| `nros-board-threadx`  | stock ThreadX + NetX Duo | `THREADX_DIR`, `THREADX_CONFIG_DIR`, `NETX_DIR`, `NETX_CONFIG_DIR` | `nros_board_threadx_platforms.toml` |
| `nros-board-nuttx`    | NuttX (kernel built by NuttX) | `NUTTX_DIR` | n/a — thin wrapper |
| `nros-board-baremetal-cortex-m` | cortex-m + smoltcp | `BOARD_LINKER_SCRIPT_DIR` | board-arch toml |
| `nros-board-baremetal-cortex-a` | cortex-a + smoltcp | same | board-arch toml |

Each manifest reuses
`packages/zpico/zpico-sys/build/manifest.rs` — the same parser
Phase 136 landed for `zenoh_platforms.toml`. Schema mirrors:
per-target `arch` profiles, `extra_sources` (`if_env` +
`with_define`), `required_env`, `include_paths_conditional`
(`when.target_match` / `target_not` / `if_env`), `compile`,
`pic`, `rerun_if_env_changed`. One parser, two consumers.

### B. Overlay crates (Layer 2.5)

Tiny (~50 LOC) crates that depend on a generic board crate +
patch vendor-specific deltas:

```rust
// nros-board-stm32f4-freertos/src/lib.rs (example shape)
pub use nros_board_freertos::{Config, run};

pub fn init_clocks() { /* STM HAL clock-tree config */ }
pub fn init_eth() { /* HAL_ETH_Init + lwIP netif binding */ }
```

```rust
// nros-board-stm32f4-freertos/build.rs
// Add STM32 HAL sources via cc-rs.
fn main() {
    let stm_hal_dir = env::var("STM32_HAL_DIR")
        .expect("set STM32_HAL_DIR to your STMicroelectronics HAL source");
    let mut hal = cc::Build::new();
    hal.flag("-mcpu=cortex-m4").flag("-mthumb").flag("-mfpu=fpv4-sp-d16");
    for f in &["stm32f4xx_hal_eth.c", "stm32f4xx_hal_uart.c", ...] {
        hal.file(format!("{stm_hal_dir}/Src/{f}"));
    }
    hal.compile("stm32f4_hal");
}
```

Vendor / community publishes these under `nros-board-<vendor>-<chip-or-board>-<rtos>`
on crates.io. nano-ros project ships canonical examples — not a
crate per SKU.

### C. Phase 139 shell polish (Layer 3)

Per-RTOS research surfaced concrete gaps in the already-landed
shells:

- **NuttX**: `integrations/nuttx/Make.defs` doesn't invoke
  upstream `apps/tools/Rust.mk`. Should `include $(APPDIR)/tools/Rust.mk`
  + append `EXTRA_LIBS += $(call RUST_GET_BINDIR,nros_c,…)`.
  `integrations/nuttx/Makefile` needs a `context::` hook calling
  `RUST_CARGO_BUILD`. `Kconfig` should expose `NROS_RMW` as a
  `choice` rather than free-form `string`, driving Cargo features
  via a `CARGO_FEATURES` Make var.
- **ESP-IDF**: shell should document `[package.metadata.esp-idf-sys]`
  `extra_components` + `bindings_header` injection — the bridge for
  Rust crates to land C glue into the IDF build tree. Currently
  `integrations/esp-idf/idf_component.yml` just registers the
  component; add a `book/src/getting-started/integration-esp-idf.md`
  section on the `esp-idf-sys` flow.
- **PlatformIO**: `lib_deps`-resolved libraries are NOT registered
  as IDF components. Document `EXTRA_COMPONENT_DIRS +=
  .pio/libdeps/${board}` workaround in
  `integrations/platformio/README.md`.
- **Zephyr**: shell already correct (T2 topology, `module.yml`,
  west manifest). Cross-link the per-RTOS book pages.

### D. Documentation (Layer 3 supporting docs)

`book/src/concepts/board-integration.md` (new) — consumption-matrix
chapter from the design doc; user picks their profile and follows
the matching path. `book/src/porting/vendor-overlay.md` (new) —
overlay-crate cookbook with the `nros-board-orin-spe` walkthrough.

---

## Work Items

- [~] **149.1 — Carve `nros-board-freertos` generic crate.** (149.1.A
      landed 2026-05-18; 149.1.B deferred)
      Split into two sub-steps:
      - **149.1.A — Scaffolding** (landed): new
        `packages/boards/nros-board-freertos/` crate claims the
        Layer-2 namespace + documents the public contract in
        `src/lib.rs`. Behind the opt-in `reference-mps2` feature
        it re-exports `Config` + `run` from
        `nros-board-mps2-an385-freertos` so future overlays can
        depend on the generic crate today and switch wiring
        transparently when 149.1.B completes the build-glue
        carve-out. Workspace `Cargo.toml` excludes the new crate
        from members (standalone like every other board crate);
        `cargo check` clean (default + `reference-mps2 --target
        thumbv7m-none-eabi`); native nano2nano E2E unchanged.
      - **149.1.B — Build-glue carve-out** (deferred): move the
        FreeRTOS kernel + lwIP + sys_arch + nros-platform-freertos
        compile pipeline out of
        `nros-board-mps2-an385-freertos/build.rs` (~600 of its 816
        LOC) into `nros-board-freertos/build.rs`. Parameterise
        per-target compiler flags via `FREERTOS_CFLAGS` env var so
        the generic crate is arch-agnostic. Leave LAN9118 driver +
        linker script + startup.c in the per-board overlay (~200
        LOC). Defer because a careful split + per-board (MPS2 +
        future overlays) verification is multi-day work; the
        scaffolding lets the rest of Phase 149 progress without
        blocking on it.
      **Files.** `packages/boards/nros-board-freertos/` (new),
      `packages/boards/nros-board-mps2-an385-freertos/` (refactor —
      149.1.B), `Cargo.toml` (exclude list — 149.1.A landed).

- [ ] **149.2 — Carve `nros-board-threadx` generic crate.**
      Same shape as 149.1 but for ThreadX kernel + NetX Duo. Existing
      `nros-board-threadx-linux` + `nros-board-threadx-qemu-riscv64`
      become overlays (`nsos-netx` Linux sim vs full NetX Duo TCP/IP
      + virtio-net on RISC-V).
      **Files.** `packages/boards/nros-board-threadx/` (new),
      `packages/boards/nros-board-threadx-linux/` (refactor),
      `packages/boards/nros-board-threadx-qemu-riscv64/` (refactor),
      `packages/boards/nros-board-threadx/nros_board_threadx_platforms.toml` (new).

- [ ] **149.3 — Refactor `nros-board-orin-spe` as canonical overlay.**
      Become a true overlay on `nros-board-freertos` — re-exports
      `Config` / `run` + adds NVIDIA FSP wiring (consumes
      `NV_SPE_FSP_DIR`, replaces lwIP with IVC link). Demonstrates
      the vendor-fork overlay pattern + the in-tree precedent for
      future `nros-board-stm32*-freertos` / `nros-board-nxp-*`
      community crates.
      **Files.** `packages/boards/nros-board-orin-spe/` (refactor).

- [ ] **149.4 — Migrate NuttX board crate.**
      `nros-board-nuttx-qemu-arm` → overlay on a thin generic
      `nros-board-nuttx` crate. NuttX owns the kernel build via its
      own apps/external path; the generic crate is mostly
      `Config` + `run` shape + `nros-platform-nuttx` registration.
      **Files.** `packages/boards/nros-board-nuttx/` (new),
      `packages/boards/nros-board-nuttx-qemu-arm/` (refactor).

- [ ] **149.5 — Reuse Phase 136 manifest parser.**
      Land `packages/boards/nros-board-common/` containing the
      shared `manifest.rs` + `policy.rs` shims that the per-kernel
      generic crates pull in (avoid duplicating the
      `packages/zpico/zpico-sys/build/manifest.rs` body across four
      crates). Could be a `pub use` re-export or a workspace-level
      build-script helper.
      **Files.** `packages/boards/nros-board-common/` (new) or shared
      shim under `packages/core/`.

- [ ] **149.6 — Overlay-crate template + cookbook.**
      `templates/overlay-board/` cookiecutter-style skeleton + book
      chapter `book/src/porting/vendor-overlay.md` walking through
      the `nros-board-orin-spe` overlay. Include a "publish to
      crates.io as `nros-board-<vendor>-<chip>-<rtos>`" naming
      contract.
      **Files.** `templates/overlay-board/` (new),
      `book/src/porting/vendor-overlay.md` (new).

- [x] **149.7 — Phase 139 shell polish (NuttX, ESP-IDF, PlatformIO).**
      (landed 2026-05-18)
      - NuttX: `integrations/nuttx/Make.defs` now `-include`s
        upstream `apps/tools/Rust.mk` and appends the Cargo-built
        staticlib paths to `EXTRA_LIBS` + `EXTRA_LIBPATHS` via
        `RUST_GET_BINDIR` / `RUST_GET_LIBDIR`. The Makefile gained
        a `context::` hook running `RUST_CARGO_BUILD` (+ `clean::`
        mirror). `Kconfig` promoted free-form `string` knobs to
        `choice` blocks (`NROS_RMW_{ZENOH,DDS,XRCE,CYCLONEDDS}` +
        `NROS_ROS_{HUMBLE,IRON}`) that the Makefile reads to
        assemble a `CARGO_FEATURES` env var driving Cargo's
        `--features` + `--no-default-features` flags. Optional
        include of `Rust.mk` keeps older NuttX trees building (just
        skips the `EXTRA_LIBS` append).
      - ESP-IDF: `book/src/getting-started/integration-esp-idf.md`
        appended an "Rust glue via `esp-idf-sys`" section
        documenting the canonical `[package.metadata.esp-idf-sys]`
        `extra_components` + `bindings_header` bridge. Links to
        `esp-rs/esp-idf-template` + `esp-idf-sys/BUILD-OPTIONS.md`
        for the full schema.
      - PlatformIO:
        `book/src/getting-started/integration-platformio.md`
        appended an "ESP-IDF gotcha" section explaining that
        `lib_deps`-resolved libraries are NOT registered as IDF
        components by default; the user's root `CMakeLists.txt`
        must append `EXTRA_COMPONENT_DIRS` pointing at
        `.pio/libdeps/<board>/nano-ros/integrations/esp-idf` for
        `idf_component_register(...)` to fire.

- [ ] **149.8 — Consumption-matrix doc.**
      `book/src/concepts/board-integration.md` (new) covering the
      seven user profiles + recommended path per the design doc's
      matrix. Cross-link from `book/src/getting-started/installation.md`
      + the per-RTOS pages added in Phase 139.
      **Files.** `book/src/concepts/board-integration.md` (new),
      `book/src/SUMMARY.md`, `book/src/getting-started/installation.md`.

- [ ] **149.9 — Migrate examples to consume generic + overlay path.**
      Each `examples/<plat>/...` README points at the appropriate
      consumption profile from 149.8. Existing Cargo `[dependencies]
      nros-board-<board>` entries unchanged — overlay re-exports
      preserve the public API.
      **Files.** `examples/**/README.md`, possibly some
      `examples/**/Cargo.toml` if dependency targets shift.

---

## Acceptance

- [ ] `cargo build` of every `examples/**` consumer keeps producing
      identical output binaries vs. pre-148 (overlay refactor is
      pure code motion).
- [ ] `cargo build -p nros-board-orin-spe` succeeds with the same
      `NV_SPE_FSP_DIR` env requirement as today.
- [ ] Adding a new overlay crate is < 100 LOC of Rust + < 50 LOC
      `build.rs`; verified by writing a `nros-board-stm32f4-freertos`
      skeleton during 149.6.
- [ ] Each per-RTOS integration smoke test (Phase 139's set: NuttX,
      PlatformIO, Zephyr, PX4, ESP-IDF) stays green when its SDK
      env is sourced.
- [ ] `book/src/concepts/board-integration.md` covers the seven
      user profiles + working consumption recipe per profile.
- [ ] `just ci` green after the refactor.

---

## Non-goals

- **No common driver HAL.** Vendor `HAL_*` / `fsl_*` / `R_*` /
  `esp_*` stays vendor-owned. Overlay crates wrap them; nano-ros
  doesn't abstract them.
- **No DTS-equivalent for non-Zephyr.** Zephyr keeps its DTS story.
  Other RTOSes use whatever board config format their vendor IDE
  produces (CubeMX `.ioc`, NuttX `defconfig`, ESP-IDF `sdkconfig`,
  etc.).
- **No mandatory board crate per SKU.** Generic + overlay covers
  long tail. A user with an exotic board + custom HAL writes an
  overlay; nano-ros project doesn't catalog them.
- **No nano-ros-managed vendor crates.** `nros-board-stm32*-freertos`
  and friends are community / vendor crates published independently
  to crates.io. nano-ros ships canonical examples for guidance, not
  a maintained per-vendor matrix.
- **No retirement of existing per-board crates in this phase.**
  Public APIs preserve; the per-board crate names users `[dependencies]`
  against today keep working via overlay-style re-exports. Future
  phase can deprecate names if community moves to publishing under
  the new naming convention.

---

## Notes

- The Phase 136 manifest parser already proves the TOML-driven
  build-data approach at scale. 149.5 reuses it to avoid
  reinventing per-kernel.
- Phase 139's smoke matrix (NuttX / PlatformIO / Zephyr / PX4 /
  ESP-IDF) validated 2026-05-18 — the integration shells work even
  before the Phase 149 board-crate refactor; 149.7 is polish, not
  rebuilding.
- Phase 116 ("unified config and extensibility") is the long-term
  north-star where this design sits. Phase 149 delivers the platform
  side; the configuration-DSL side (a la Zephyr's Kconfig + DTS)
  stays a Phase 116 open question.
- Open question from the design doc: **monorepo vs sister repo for
  vendor / community overlay crates?** Lean monorepo for the
  initial canonical set (Orin SPE + the three existing board
  refactors); spin a `nano-ros-boards` sister repo when the
  community publishes more than ~5 overlays.
- Open question: **who owns `nros-board-stm32*` / `nros-board-nxp-*`?**
  Plan: community-owned with one nano-ros-blessed example per
  vendor (149.6 covers the example shape).
