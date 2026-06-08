---
rfc: 0012
title: "Board / BSP Integration Architecture"
status: Stable
since: 2026-05
last-reviewed: 2026-05
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# Board / BSP Integration Architecture

**Date.** 2026-05-18
**Status.** Design proposal. Drafted in response to "vendor BSPs vary; we
can't ship a board crate per (vendor × board × SDK-variant) combo."
**Author.** Synthesised from per-RTOS research (Zephyr / ESP-IDF /
NuttX / FreeRTOS / ThreadX) + existing nano-ros code.
**Related phases.** 116 (unified config — north-star), 137-140
(source-distribution refactor), 139 (per-RTOS integration shells),
144 (example migration), 138 (per-platform CMake consolidation),
136 (manifest-driven zpico-sys build).

---

## Problem statement

Today's nano-ros expects one Cargo **board crate** per
`(vendor × board × SDK-variant)` combination:
`nros-board-mps2-an385-freertos`, `nros-board-threadx-linux`,
`nros-board-threadx-qemu-riscv64`, `nros-board-orin-spe`, etc. Each
crate's `build.rs` compiles the RTOS kernel + drivers + nros platform
glue into a Cargo binary.

Vendor reality breaks the model:

- **NXP MCUXpresso**, **STM32CubeIDE**, **Espressif's FreeRTOS fork**,
  **Renesas Synergy** (ThreadX), **NVIDIA FSP** (FreeRTOS V10.4.3 for
  Orin SPE) each ship a forked kernel + their own driver APIs
  (`fsl_*` / `HAL_*` / `R_*` / `esp_*`) + their own build glue
  (`west` + `EXTRA_MCUX_MODULES` / `.ioc` codegen with `USER_CODE`
  markers / `idf_component_register` / Synergy SSP GUI / plain CMake).
- There is **no common BSP abstraction** outside Zephyr's DTS.
- There is **no package registry** outside ESP-IDF's
  `components.espressif.com` and the Zephyr `west.yml` ecosystem.
- The combo space is N × M × K; a per-combo Cargo crate doesn't scale
  past hand-curated demos.

Forcing every user to fork-and-edit a board crate is high-friction.
Shipping a board crate per vendor SKU is impossible.

## Approach

Combine two strategies — neither alone is enough:

- **B: Generic board crates with env-var / overlay-crate SDK pointers.**
  One crate per RTOS-kernel family (`nros-board-freertos`,
  `nros-board-threadx`) that takes SDK paths via env vars + optional
  overlay crates for vendor forks. Covers users who pick "stock RTOS
  source + my own drivers" workflow.
- **E: Per-RTOS native integration shells.** Stop competing with the
  RTOS's own package manager (`west` / `idf.py` / `apps/external` /
  CMake `add_subdirectory`). Ship `integrations/<rtos>/` shells so
  each RTOS pulls nano-ros in using its own idiomatic mechanism;
  vendor BSPs already handle board / driver wiring inside that
  ecosystem.

The split: **B owns the Cargo-driven Rust user path**; **E owns the
RTOS-native vendor-IDE path**. Same nano-ros library underneath;
different consumption surfaces.

## Layered model

```
┌─────────────────────────────────────────────────────────────────┐
│ Layer 5 — User app                                              │
│   - Rust crate (Cargo) OR C/C++ CMake project OR vendor IDE     │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│ Layer 4 — Vendor BSP                                            │
│   - STM HAL / NXP MCUXpresso / Espressif ESP-IDF / Synergy SSP  │
│   - DTS overlays (Zephyr) / .ioc (Cube) / defconfig (NuttX)     │
│   - Owned BY the vendor, NOT by nano-ros                        │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│ Layer 3 — Integration shell (one per RTOS)                      │
│   - zephyr/         (west module + module.yml)     │
│   - integrations/nano-ros/        (idf_component.yml)            │
│   - integrations/platformio/     (library.json)                 │
│   - integrations/nuttx/          (Make.defs + Kconfig + Rust.mk)│
│   - integrations/px4/            (EXTERNAL_MODULES_LOCATION)    │
│   - cmake/platform/nano-ros-<plat>.cmake (Phase 138 modules)    │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│ Layer 2 — Generic board crate (one per RTOS-kernel family)      │
│   - nros-board-freertos     (stock kernel + lwIP; env vars      │
│                              point at FREERTOS_DIR / LWIP_DIR;  │
│                              overlay crates for vendor forks)   │
│   - nros-board-threadx      (stock kernel + NetX Duo; env vars  │
│                              point at THREADX_DIR / NETX_DIR)   │
│   - nros-board-nuttx        (thin wrapper; NuttX owns kernel)   │
│   - nros-board-baremetal-cortex-m (smoltcp glue)                │
│   - Per-board overlay crates (small, ~50 LOC):                  │
│       nros-board-mps2-an385-freertos  → re-exports + linker     │
│       nros-board-orin-spe             → FSP-FreeRTOS overlay    │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│ Layer 1 — Platform contract (FROZEN, Phase 79 / 134)            │
│   - <nros/platform_*.h>: net / time / mem / sync / random       │
│   - ~30 C functions; each platform-provider implements them     │
│   - nros-platform-{posix,freertos,nuttx,threadx,zephyr,esp-idf} │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│ Layer 0 — nros core (platform-agnostic)                         │
│   - nros-rmw + nros-rmw-{zenoh,dds,xrce,cyclonedds}             │
│   - nros-c / nros-cpp / nros (Rust API)                         │
│   - zpico-sys (zenoh-pico) — Phase 136 manifest-driven build    │
└─────────────────────────────────────────────────────────────────┘
```

Layers 0 + 1 are stable. Layers 2-4 are where vendor variation lives.
Layer 3 (the integration shells) is the new abstraction this design
introduces alongside the existing per-board Cargo crates.

## Per-RTOS path summary

Detailed findings on each RTOS's native conventions feed the design.
The full research output (URLs + concrete file snippets) is captured
in the per-RTOS sections of `docs/roadmap/phase-139-rtos-integration-shells.md`;
this section distils the integration contract.

### Zephyr — `west` + `module.yml`

- Any git tree with `zephyr/module.yml` is a discoverable module.
- User adds `projects:` entry to their own `west.yml` (T2 topology);
  no requirement for nano-ros to be the manifest repo.
- DTS overlays (`app.overlay`, `boards/<board>.overlay`) hook drivers.
  Vendor HALs (`hal_stm32`, `hal_nordic`, `hal_espressif`) are
  themselves modules.
- nano-ros's existing `zephyr/module.yml` + Phase 139's planned
  `zephyr/west.yml` are the canonical shape.
- **Generic board crate not needed** for Zephyr; Zephyr owns the
  board contract via DTS. Layer 2 collapses into Layer 3 for this
  RTOS.

### ESP-IDF — `idf_component.yml` + ESP Component Registry

- `idf.py add-dependency nano-ros` pulls from
  `components.espressif.com`; local development uses `path:` overrides.
- Each component ships `CMakeLists.txt` with one
  `idf_component_register(...)` call + optional `Kconfig.projbuild`.
- ESP-IDF auto-handles chip variation via `REQUIRED_IDF_TARGETS`
  gating; one component manifest covers ESP32 / S2 / S3 / C3 / C6 /
  H2 / P4.
- Rust integration today: `esp-idf-sys`'s `[package.metadata.esp-idf-sys]`
  + `extra_components` injection is the bridge.
- Phase 139.2 + 139.3 land `integrations/nano-ros/` +
  `integrations/platformio/` shells.

### NuttX — `apps/external/` + Make.defs + Kconfig + `Rust.mk`

- Out-of-tree apps drop into `apps/external/<name>/` (symlink or
  submodule; `.gitignore` whitelists this).
- Required files: `Make.defs` (adds to `CONFIGURED_APPS`,
  `EXTRA_LIBS`), `Makefile` (with `context::` hook for Cargo build),
  `Kconfig` (auto-sourced under "Application Configuration").
- Upstream `apps/tools/Rust.mk` provides
  `RUST_CARGO_BUILD` / `RUST_GET_BINDIR` / `RUST_TARGET_TRIPLE`
  macros; Rust has first-class `*-nuttx-*` targets since rustc 1.82.
- Vendor variation handled by NuttX's own `boards/<arch>/<chip>/<board>/`
  + `defconfig` system. PX4 replaces `apps/` entirely via
  `CONFIG_APPS_DIR`.
- **Generic board crate is thin** — NuttX owns the kernel build;
  Cargo only produces the staticlib that gets pulled into NuttX's
  monolithic link.

### FreeRTOS — vendor-IDE pluralism, no common BSP

- Stock FreeRTOS ships kernel + ports only. **No drivers.** Vendor
  SDKs (NXP MCUXpresso, STM32Cube, Espressif IDF, AWS LTS) each ship
  their own integration glue.
- No package registry; integration is per-vendor:
  `EXTRA_MCUX_MODULES` (NXP), `.ioc` + `USER_CODE` markers
  (STM32CubeMX), `idf_component_register` (ESP-IDF),
  hand-managed CMake `add_subdirectory` elsewhere.
- ESP-IDF's FreeRTOS is a hard fork (per-core scheduler suspension,
  best-effort round-robin, unsynchronised tick ISRs); SMP semantics
  differ from upstream. Treat as a separate platform-provider.
- **Generic `nros-board-freertos`** crate covers "stock kernel + lwIP"
  combo. Vendor variants get **overlay crates** that re-export +
  patch:

  ```rust
  // nros-board-stm32f4-freertos/src/lib.rs (overlay)
  pub use nros_board_freertos::{Config, run};

  pub fn init_clocks() { /* STM HAL clock tree */ }
  pub fn init_eth() { /* HAL_ETH_Init + lwIP netif */ }
  ```

  Overlay is ~50 LOC + a `build.rs` that adds vendor HAL sources via
  `cc-rs`.

### ThreadX — Eclipse Foundation + vendor SDK pluralism

- Eclipse ThreadX (`github.com/eclipse-threadx/*`) ships as
  CMake-buildable source. No registry.
- Vendor variants: Renesas Synergy SSP (GUI), STM32 X-CUBE-AZRTOS-*,
  NXP MCUXpresso ThreadX. Same pattern as FreeRTOS — each vendor
  wraps the kernel with their config tooling.
- NetX-Duo Ethernet integration via `NX_IP_DRIVER`
  function-pointer contract; vendor supplies the driver.
- **Generic `nros-board-threadx`** + overlay-crate pattern, same
  shape as FreeRTOS.

## Concrete proposal

### Layer 2 — generic board crates

Ship one per kernel family with stock-source assumption:

| Crate | Covers | SDK pointer env vars |
|---|---|---|
| `nros-board-freertos` | stock FreeRTOS + lwIP | `FREERTOS_DIR`, `FREERTOS_PORT`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR` |
| `nros-board-threadx`  | stock ThreadX + NetX Duo | `THREADX_DIR`, `THREADX_CONFIG_DIR`, `NETX_DIR`, `NETX_CONFIG_DIR` |
| `nros-board-nuttx`    | stock NuttX (kernel built by NuttX, not by us) | `NUTTX_DIR` |
| `nros-board-baremetal-cortex-m` | cortex-m + smoltcp | `BOARD_LINKER_SCRIPT_DIR` |
| `nros-board-baremetal-cortex-a` | cortex-a + smoltcp | same |

Each generic crate's `build.rs` consumes a TOML manifest analogous to
Phase 136's `zenoh_platforms.toml` — declarative source + define +
cflags data; one place to edit when SDK paths change. Reuse the
manifest parser landed in `packages/zpico/zpico-sys/build/manifest.rs`.

### Layer 2.5 — vendor overlay crates

Small (~50 LOC) crates that depend on a generic board crate and patch:

- `nros-board-orin-spe` (overlay on `nros-board-freertos`) —
  consumes NVIDIA FSP via `NV_SPE_FSP_DIR`, replaces lwIP with
  IVC link.
- `nros-board-stm32f4-freertos` — adds STM HAL clock/ETH init.
- `nros-board-nxp-mcuxpresso-freertos` — adds NXP fsl_* driver wiring.
- `nros-board-renesas-synergy-threadx` — consumes Synergy SSP-generated
  init code.

Naming convention: `nros-board-<vendor>-<chip-or-board>-<rtos>`.
Vendors / community can publish their own overlays without nano-ros
project ownership. Crates.io namespace is open per Phase 111.B.2
audit.

### Layer 3 — integration shells (one per RTOS package manager)

Phase 139 ships these. Updates needed:

- **Zephyr**: `zephyr/{west.yml, module.yml}` so users
  add nano-ros to their existing `west.yml` and `west update`. Vendor
  HALs (`hal_stm32`, etc.) come from mainline Zephyr separately.
  No board crate consumed — Zephyr's DTS owns board config.
- **ESP-IDF**: `integrations/nano-ros/{CMakeLists.txt, idf_component.yml}`
  with `idf_component_register(...)` + `add_subdirectory(<repo-root>)`
  to delegate to Phase 137 root CMake. Publish to ESP Component
  Registry once stable.
- **PlatformIO**: `integrations/platformio/library.json` —
  `frameworks: ["espidf", "arduino", "zephyr"]` with documented
  `EXTRA_COMPONENT_DIRS` workaround for ESP-IDF, since PIO's
  `lib_deps` doesn't auto-register libraries as IDF components.
- **NuttX**: `integrations/nuttx/{Make.defs, Makefile, Kconfig,
  CMakeLists.txt}` — invokes upstream `Rust.mk` macros; `Kconfig`
  becomes a `choice` for the RMW backend (drives Cargo `--features`
  via a Makefile-assembled `CARGO_FEATURES` var).
- **PX4**: `integrations/px4/module-template/` — discovers via
  `EXTERNAL_MODULES_LOCATION`; user copies to their PX4 board's
  modules dir.

### Layer 1 — platform contract (unchanged)

Already stable (Phase 79 / 134). Frozen ~30 C functions in
`<nros/platform_*.h>`. Each `nros-platform-<rtos>` crate implements
them against that RTOS's APIs.

## User-facing consumption matrix

| User profile | Recommended path | What they write |
|---|---|---|
| Cargo-first Rust dev, has SDK sources | Generic board crate + env vars | `[dependencies] nros-board-<kernel>` + env vars |
| Vendor-IDE user (STM32CubeIDE etc.) | Vendor's existing FreeRTOS / ThreadX integration + nano-ros as a CMake `add_subdirectory` library | Copy generated code + `add_subdirectory` line |
| Zephyr user (any board) | `zephyr/` shell via `west` | `projects:` entry in `west.yml` + `CONFIG_NROS=y` |
| ESP-IDF user (any chip) | `integrations/nano-ros/` shell | `idf.py add-dependency nano-ros` |
| NuttX user (any board) | `integrations/nuttx/` shell | `ln -s … apps/external/nano-ros` + `make menuconfig` |
| PX4 user | `integrations/px4/` shell | Set `EXTERNAL_MODULES_LOCATION`, add to module list |
| PlatformIO user | `integrations/platformio/` shell | `lib_deps = nano-ros` |
| Niche RTOS / out-of-tree vendor fork | Generic board crate + overlay crate they author | ~50 LOC overlay crate that re-exports + adds vendor glue |

## Non-goals

- **No common driver HAL.** Each vendor's `HAL_*` / `fsl_*` / `R_*`
  API stays vendor-owned. nano-ros doesn't try to abstract over them.
- **No DSL like Zephyr's DTS.** That's Zephyr's job. For non-Zephyr
  platforms, board config lives in whatever the vendor's IDE
  produces.
- **No mandatory board crate per SKU.** Generic board crate + overlay
  pattern covers the long tail.

## Work breakdown (proposed new phase)

Suggest **Phase 152 — Board / BSP abstraction layer**:

1. **148.1 — Carve generic board crates from existing per-board ones.**
   Refactor `nros-board-mps2-an385-freertos` to depend on a new
   `nros-board-freertos` generic crate + ~50 LOC overlay.
2. **148.2 — TOML manifest for board / RTOS data.**
   `nros-board-<kernel>/board_platforms.toml` mirrors Phase 136's
   `zenoh_platforms.toml`. Reuse the parser from
   `packages/zpico/zpico-sys/build/manifest.rs`.
3. **148.3 — Overlay-crate template + docs.**
   Cookiecutter under `templates/overlay-board/`; book chapter under
   `book/src/porting/vendor-overlay.md`.
4. **148.4 — Migrate ThreadX overlays.** `nros-board-threadx-linux`
   + `nros-board-threadx-qemu-riscv64` become overlays on
   `nros-board-threadx`.
5. **148.5 — Migrate `nros-board-orin-spe` overlay.** Already an
   FSP-FreeRTOS variant — perfect first canonical overlay.
6. **148.6 — Polish Phase 139 shells per per-RTOS research findings.**
   NuttX needs `Rust.mk` wiring; ESP-IDF needs `extra_components`
   bridge to `esp-idf-sys`; PlatformIO needs the
   `EXTRA_COMPONENT_DIRS` workaround documented.
7. **148.7 — Doc page.** `book/src/concepts/board-integration.md`
   explaining the consumption matrix above.
8. **148.8 — Migration of existing examples.** Each
   `examples/<plat>/...` README points at the appropriate
   consumption path.

Phase 152 depends on Phase 137 (already landed), Phase 138 (already
landed), Phase 139 (already landed), Phase 136 (already landed —
provides the manifest parser to reuse). No new infrastructure
needed; this phase is restructuring + documentation.

## Open questions

1. **Should generic board crates ship in nano-ros's main repo or
   under a `nano-ros-boards` sister repo?** Main repo is simpler now;
   sister repo isolates vendor-specific concerns when overlay
   ecosystem grows.
2. **How aggressive should we be on retiring per-board crates?**
   Backward compatibility: existing
   `nros-board-mps2-an385-freertos` consumers shouldn't break. The
   overlay-on-generic refactor must preserve the public API.
3. **Who owns `nros-board-stm32*` / `nros-board-nxp-*`?** Three
   options:
   - nano-ros project ships one canonical per major vendor
   - Vendor / community owns them on crates.io
   - Both — official "supported" set + community add-ons
   Lean toward "community-owned with one nano-ros-blessed example
   per vendor."
4. **`std`-aware vs `no_std` generic board crates.** Linux sim
   (`nros-board-threadx-linux`) wants `std`; bare-metal wants
   `no_std`. Either split crates by `std` capability or feature-gate
   inside one crate. Current per-board crates do the latter; keep
   that.

## Sources

Per-RTOS research output (research agent reports captured during
this design pass):
- Zephyr modules + west + DTS + vendor HALs.
- ESP-IDF component manifest + registry + Espressif's FreeRTOS fork
  + esp-rs Rust stack.
- NuttX `apps/external/` + Make.defs + `Rust.mk` + PX4 board
  porting guide.
- FreeRTOS + ThreadX vendor pluralism (NXP MCUXpresso, STM32CubeMX,
  ESP-IDF, AWS LTS, Renesas Synergy, X-CUBE-AZRTOS).

URLs are inlined in the matching `docs/roadmap/phase-139-rtos-integration-shells.md`
sections.
