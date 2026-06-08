# Vendor Overlay Board Crate

An **overlay** crate is a small (~50–150 LOC) Cargo crate that depends
on a generic per-kernel board crate (`nros-board-freertos`,
`nros-board-threadx`, `nros-board-nuttx`, `nros-board-baremetal-cortex-{m,a}`)
and patches the deltas a specific vendor board / fork needs:

- Vendor HAL source files (NXP `fsl_*`, STM `HAL_*`, NVIDIA FSP, …).
- Per-chip linker script + startup assembly.
- Custom kernel-config header (`FreeRTOSConfig.h`, `tx_user.h`).
- Custom network-stack glue (vendor Ethernet driver wired to lwIP /
  NetX-Duo).
- Custom clock-tree / pin-mux init.

This page documents the contract: what the generic crate exposes,
what the overlay overrides, and how to publish a community / vendor
overlay to crates.io.

## Why overlays

nano-ros's generic board crates cover the "stock RTOS source + your
own drivers" workflow. Vendor SDKs (NXP MCUXpresso, STM32Cube,
Espressif ESP-IDF, Renesas Synergy, NVIDIA FSP) ship forked kernels +
custom drivers; bolting those into a generic crate would force a
build-script branch per vendor. The overlay pattern keeps the
generic crate clean: nano-ros ships the kernel-family scaffolding,
vendors / community ship the per-fork glue.

See `docs/roadmap/phase-152-board-bsp-abstraction-layer.md` for the
phase that landed the architecture.

## Contract

A generic per-kernel board crate exposes:

| Item | Type | Purpose |
|---|---|---|
| `Config` | struct | TOML-loaded network + zenoh config; overlay can extend. |
| `run(Config, FnOnce(&Config) -> Result<()>)` | function | Entry point. Initialises kernel + network, calls closure inside the app thread. |
| `BoardInit` | trait | Hooks the overlay implements: `init_clocks`, `init_eth`, `init_extra_drivers`. |
| `init_hardware()` | function | Default no-op; overlay re-exports a board-specific version. |

The overlay's `build.rs`:

1. Inherits the generic crate's `FREERTOS_DIR` / `THREADX_DIR` /
   etc. env-var contract (overlay doesn't override unless needed).
2. Adds vendor HAL `.c` sources via its own `cc::Build`.
3. Optionally regenerates the linker script (e.g. STM32F4 vs
   STM32F7 sector layout).

## Minimal overlay shape

```rust
// nros-board-stm32f4-freertos/src/lib.rs
#![no_std]

// Re-export the generic Config + run from the upstream kernel crate.
pub use nros_board_freertos::{Config, run};

/// Board-specific clock-tree configuration.
/// Called from `run()` before lwIP init.
#[no_mangle]
pub extern "C" fn nros_board_init_clocks() {
    // HAL_RCC_OscConfig + HAL_RCC_ClockConfig + ...
}

/// Wire the STM32 ETH peripheral into lwIP.
/// Called from `run()` after kernel start, before app callback.
#[no_mangle]
pub extern "C" fn nros_board_init_eth() {
    // HAL_ETH_Init + lwIP netif_add binding
}
```

```rust
// nros-board-stm32f4-freertos/build.rs
use std::{env, path::PathBuf};

fn main() {
    let stm_hal_dir = env::var("STM32_HAL_DIR")
        .expect("set STM32_HAL_DIR to your STMicroelectronics HAL source dir");

    let mut hal = cc::Build::new();
    hal.flag("-mcpu=cortex-m4")
       .flag("-mthumb")
       .flag("-mfpu=fpv4-sp-d16")
       .flag("-mfloat-abi=hard")
       .include(format!("{stm_hal_dir}/Inc"));

    for f in &[
        "Src/stm32f4xx_hal_eth.c",
        "Src/stm32f4xx_hal_uart.c",
        "Src/stm32f4xx_hal_rcc.c",
        // ...
    ] {
        hal.file(format!("{stm_hal_dir}/{f}"));
    }
    hal.compile("stm32f4_hal");

    // Board-specific linker script wired via the generic crate's
    // BOARD_LINKER_SCRIPT_DIR env var.
    println!(
        "cargo:rustc-env=BOARD_LINKER_SCRIPT_DIR={}",
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("config")
            .display()
    );
    println!("cargo:rerun-if-env-changed=STM32_HAL_DIR");
}
```

```toml
# nros-board-stm32f4-freertos/Cargo.toml
[package]
name = "nros-board-stm32f4-freertos"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
authors = ["Your Name <you@example.com>"]
description = "STM32F4 + FreeRTOS overlay on nros-board-freertos"
repository = "https://github.com/<you>/nros-board-stm32f4-freertos"

[dependencies]
nros-board-freertos = "0.1"

[build-dependencies]
cc = "1.0"
```

User application code stays identical to the generic-crate case
except for the `[dependencies]` line:

```rust
use nros_board_stm32f4_freertos::{Config, run};
use nros::prelude::*;

run(Config::from_toml(include_str!("../config.toml")), |config| {
    let exec_config = ExecutorConfig::new(config.zenoh_locator);
    let mut executor = Executor::open(&exec_config)?;
    // ...
    Ok::<(), NodeError>(())
})
```

## Canonical in-tree precedent

`packages/boards/nros-board-orin-spe/` is the canonical FSP-FreeRTOS
overlay (refactors it explicitly into this shape):

- Re-exports `Config` + `run` from `nros-board-freertos`.
- `build.rs` reads `NV_SPE_FSP_DIR`, pulls FreeRTOS V10.4.3 headers
  from NVIDIA's FSP install.
- Replaces lwIP with IVC link via `zpico-link-ivc`.
- Provides `nros_board_init_ivc()` instead of `init_eth()`.

`packages/boards/nros-board-mps2-an385-freertos/` is the canonical
"stock kernel + custom Ethernet driver" overlay:

- Re-exports `Config` + `run` from `nros-board-freertos`.
- `build.rs` adds the LAN9118 driver C sources + per-board linker
  script.
- Provides LAN9118 IRQ-binding code in `init_eth()`.

Read both for working code.

## Naming convention

Publish to crates.io as
**`nros-board-<vendor>-<chip-or-board>-<rtos>`**. Examples:

- `nros-board-stm32f4-freertos`
- `nros-board-stm32h7-threadx`
- `nros-board-nxp-mimxrt1064-freertos`
- `nros-board-renesas-synergy-s7g2-threadx`
- `nros-board-nordic-nrf5340-zephyr` (rare — Zephyr generally owns
  board contract via DTS; only needed when a non-Zephyr nano-ros
  consumer wants to target an nRF board outside the Zephyr build)

Crates.io has no namespacing; the `nros-board-` prefix is the
informal namespace. The `nros-board-` names listed
audit are all unclaimed today.

## What overlays DO

- ✅ Re-export `Config` + `run` (or extend `Config` with vendor-
  specific fields and re-implement `run` if needed).
- ✅ Add vendor HAL C sources via `cc::Build`.
- ✅ Provide `#[no_mangle]` hooks the generic crate's C glue calls
  (`nros_board_init_clocks`, `nros_board_init_eth`, etc.).
- ✅ Ship board-specific config files (linker script,
  `FreeRTOSConfig.h`, `tx_user.h`).
- ✅ Read vendor-SDK env vars (`STM32_HAL_DIR`, `NXP_SDK_DIR`,
  `NV_SPE_FSP_DIR`) and inject paths into cc-rs.

## What overlays DON'T

- ❌ Re-implement kernel build glue (that's the generic crate's job).
- ❌ Add features that should live in the generic crate (push them
  upstream instead).
- ❌ Duplicate `nros-platform-<rtos>` registration (the generic
  crate handles it).
- ❌ Override `nros-rmw-*` selection (user picks RMW via Cargo
  features on `nros`, same as any nano-ros consumer).
- ❌ Ship a fork of zenoh-pico / Cyclone DDS / mbedTLS (use the upstream's manifest).

## Testing an overlay locally

```bash
# 1. Clone or scaffold the overlay crate next to your application.
git clone https://github.com/<you>/nros-board-<your-vendor>-<rtos>

# 2. Point your application's Cargo.toml at it (path dep for dev).
[dependencies]
nros-board-<your-vendor>-<rtos> = { path = "../nros-board-<your-vendor>-<rtos>" }

# 3. Build with the vendor SDK env vars set.
export FREERTOS_DIR=$HOME/sdk/freertos/kernel
export FREERTOS_PORT=GCC/ARM_CM4F
export LWIP_DIR=$HOME/sdk/freertos/lwip
export STM32_HAL_DIR=$HOME/sdk/stm32cube/STM32F4xx_HAL_Driver
cargo build --release --target thumbv7em-none-eabihf
```

## Skeleton template

`templates/overlay-board/` ships a minimal skeleton:

- `Cargo.toml.template` — deps on the generic kernel crate.
- `src/lib.rs.template` — `pub use` re-exports + `#[no_mangle]` hook
  stubs.
- `build.rs.template` — cc-rs HAL-source injection scaffold.
- `README.md.template` — env vars + setup recipe.

Copy the directory, rename the placeholder, and fill in the
vendor-specific bits. See `templates/overlay-board/README.md` for
the per-file walkthrough.

## Publishing to crates.io

Same flow as any Rust crate. Recommend:

1. `cargo publish --dry-run` to sanity-check.
2. Pin the generic crate dep to a minor-version range
   (`nros-board-freertos = "0.1"`); avoid `^0.0` — that won't lock
   on patch bumps.
3. Tag a release on your repo for traceability.
4. Open a PR against
   `book/src/getting-started/community-board-crates.md` (TODO —
   landed by) to add a link to your crate.

## Related

- `docs/design/0012-board-bsp-integration-architecture.md` — the layered
  model + the consumption matrix.
- `docs/roadmap/phase-152-board-bsp-abstraction-layer.md` — the
  phase doc.
- [Custom Board Package](custom-board.md) — older guide; covers
  monolithic board crates before the overlay split.
- [Custom Platform](custom-platform.md) — `nros-platform-<rtos>`
  guide (the Layer 1 contract overlays rely on).
