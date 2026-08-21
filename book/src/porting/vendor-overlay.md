# Vendor Overlay Board Crate

An **overlay** crate is a small Cargo crate that depends
on a generic per-kernel board crate (`nros-board-freertos`,
`nros-board-threadx`, `nros-board-nuttx` — there is no bare-metal
family crate; that one was deliberately deleted in phase-337 W7.c)
and patches the deltas a specific vendor board / fork needs:

- Vendor HAL source files (NXP `fsl_*`, STM `HAL_*`, NVIDIA FSP, …).
- Per-chip linker script + startup assembly.
- Custom kernel-config header (`FreeRTOSConfig.h`, `tx_user.h`).
- Custom network-stack glue (vendor Ethernet driver wired to lwIP /
  NetX-Duo).
- Custom clock-tree / pin-mux init.

This page documents the contract: what the generic crate exposes,
what the overlay overrides, and how to distribute an overlay.
(nano-ros is source-only — nothing is published to crates.io — so
overlays are consumed as path or git dependencies.)

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

A generic per-kernel board crate (taking `nros-board-freertos` as the
worked case) exposes:

| Item | Type | Purpose |
|---|---|---|
| `Config` / `BaseConfig` | structs | Network + transport config; overlay can extend. |
| `run_bare` / `run_entry` / `run_tiers_entry` | generic functions | Entry points, generic over a board **marker type**. Initialise kernel + network, then call your closure inside the app task. |
| `nros_platform::BoardInit` | trait | One required method — `init_hardware()`, called once before the executor opens. Your marker type implements it (plus `BoardPrint` / `BoardExit` / `BoardEntry` from the same family). |
| `nros_board_register_netif` / `nros_board_poll_netif` | **weak C hooks** (`c/network_glue.c`) | The network attach points: the overlay provides strong definitions wiring the vendor Ethernet driver into lwIP. |

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

// Re-export the generic config types from the kernel-family crate.
pub use nros_board_freertos::{BaseConfig, Config};

/// Per-board marker for trait dispatch into `nros_board_freertos::run_*`.
pub struct Stm32F4;

impl nros_platform::BoardInit for Stm32F4 {
    fn init_hardware() {
        // HAL_RCC_OscConfig + HAL_RCC_ClockConfig + pin mux + ...
    }
}
// (implement BoardPrint / BoardExit / BoardEntry the same way —
//  `packages/boards/nros-board-mps2-an385-freertos/src/lib.rs` is the
//  working precedent to copy.)

/// The network attach point is a strong definition of the generic
/// crate's WEAK C hook (c/network_glue.c) — typically provided from a
/// C file your build.rs compiles, wiring the vendor Ethernet driver
/// into lwIP:
///
///   int nros_board_register_netif(const uint8_t *mac, ...);
///   void nros_board_poll_netif(void);
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
# Path into your vendored nano-ros checkout (or a git dep on your fork).
# nano-ros is source-only: these names are NOT on crates.io.
nros-board-freertos = { path = "../nano-ros/packages/boards/nros-board-freertos" }
nros-platform = { path = "../nano-ros/packages/platform/nros-platform", default-features = false }

[build-dependencies]
cc = "1.0"
```

User application code stays identical to the generic-crate case
except for the `[dependencies]` line:

```rust
use nros_board_stm32f4_freertos::{Config, Stm32F4};

// The `nros::main!(board = Stm32F4)` macro emits this dispatch; the
// manual form is:
nros_board_freertos::run_entry::<Stm32F4, _, _>(Config::default(), None, |runtime| {
    // register your nodes on `runtime`
    Ok(())
})
```

## Canonical in-tree precedent

`packages/boards/nros-board-mps2-an385-freertos/` is the canonical
"stock kernel + custom Ethernet driver" overlay, and the ONLY in-tree
one (the former orin-spe overlay was removed in phase-337 W7.b):

- Re-exports `BaseConfig` + `Config` from `nros-board-freertos`.
- Defines the `Mps2An385` marker and implements the
  `nros_platform` board traits on it.
- `build.rs` adds the LAN9118 driver C sources + per-board linker
  script.
- Provides the strong `nros_board_register_netif` /
  `nros_board_poll_netif` definitions binding LAN9118 into lwIP.

Read it for working code; [Adding a FreeRTOS
Board](freertos-board.md) walks the same contract step by step.

## Naming convention

Name the crate **`nros-board-<vendor>-<chip-or-board>-<rtos>`**.
Examples:

- `nros-board-stm32f4-freertos`
- `nros-board-stm32h7-threadx`
- `nros-board-nxp-mimxrt1064-freertos`
- `nros-board-renesas-synergy-s7g2-threadx`
- `nros-board-nordic-nrf5340-zephyr` (rare — Zephyr generally owns
  board contract via DTS; only needed when a non-Zephyr nano-ros
  consumer wants to target an nRF board outside the Zephyr build)

The `nros-board-` prefix is the informal namespace; keep it so a
reader can tell a board crate from an app crate at a glance.

## What overlays DO

- ✅ Re-export `Config` + `run` (or extend `Config` with vendor-
  specific fields and re-implement `run` if needed).
- ✅ Add vendor HAL C sources via `cc::Build`.
- ✅ Provide strong definitions of the generic crate's weak C hooks
  (`nros_board_register_netif`, `nros_board_poll_netif`).
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
- `src/lib.rs.template` — marker type + trait-impl and hook stubs.
- `build.rs.template` — cc-rs HAL-source injection scaffold.
- `README.md` — env vars + setup recipe.

Copy the directory, rename the placeholder, and fill in the
vendor-specific bits. See `templates/overlay-board/README.md` for
the per-file walkthrough.

## Distributing an overlay

nano-ros is **source-only** (decision 2026-05-14): nothing — including
board crates — is on crates.io, so do not `cargo publish`. Ship the
overlay as a git repository and have consumers take it as a `git` or
`path` dependency next to their vendored nano-ros checkout. Tag
releases against the `nros-v<X.Y.Z>` tag of nano-ros you developed
against, and state that pairing in your README — the board-trait
surface is versioned with the tree, not semver-independent.

## Related

- `docs/design/0012-board-bsp-integration-architecture.md` — the layered
  model + the consumption matrix.
- `docs/roadmap/phase-152-board-bsp-abstraction-layer.md` — the
  phase doc.
- [Custom Board Package](custom-board.md) — older guide; covers
  monolithic board crates before the overlay split.
- [Custom Platform](custom-platform.md) — `nros-platform-<rtos>`
  guide (the Layer 1 contract overlays rely on).
