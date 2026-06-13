//! # nros-board-fvp-aemv8r-smp
//!
//! Board crate for ARM FVP `Base_RevC AEMv8-R` Cortex-A SMP under Zephyr.
//! Target use: Phase-117 Cyclone DDS RMW on the Autoware safety-island
//! reference platform.
//!
//! Zephyr handles boot, MMU, network stack, and the ethernet driver via
//! its Kconfig + DTS pipeline. This crate is a thin Cargo + config bundle:
//! a [`Config`] type with FVP-specific defaults plus the matching
//! `prj.conf` / board overlay shipped under `boards/`. Apps consume the
//! crate from a Zephyr `west build -b fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`
//! invocation; see `boards/README.md` for the runner setup (FVP license
//! gated).
//!
//! # Transports
//!
//! - `ethernet` (default) — Zephyr's net stack drives the FVP's emulated
//!   GICv3-attached ethernet controller. IPv4 only; multicast handled by
//!   the kernel.
//!
//! # Status
//!
//! Phase 117.10 — config + skeleton only. Build smoke (Phase 117.13) is
//! the gating end-to-end check; runtime needs ARM FVP `Base_RevC AEMv8R`
//! and an `aarch64-zephyr-elf` toolchain in the Zephyr SDK install.

#![no_std]

mod config;
mod node;

pub use config::Config;
pub use node::{init_hardware, run};

// Phase 248 C5a (#60 T4) — the board owns Cyclone DDS backend linking.
// Force-link the `nros-rmw-cyclonedds-sys` rlib so its `RMW_INIT_ENTRIES`
// self-register section + the `register` symbol survive stable-Rust rlib
// pruning and reach the final Zephyr image, WITHOUT a consumer naming the
// backend (`nros/rmw-cyclonedds`). Mirrors `__FORCE_LINK_CYCLONEDDS_SYS` in
// `nros/src/lib.rs`. Zephyr (`target_os = "none"`) is linkme-blind + the
// section walker is a no-op, so `node::init_hardware` also calls `register()`
// explicitly — this static guarantees the rlib is not pruned before that call.
// The C++ libddsc + `nros_rmw_cyclonedds_register` symbol come from CMake.
// Inert unless `rmw-cyclonedds` selects the backend.
#[cfg(feature = "rmw-cyclonedds")]
#[doc(hidden)]
#[used]
pub static __FORCE_LINK_CYCLONEDDS_SYS: fn() -> Result<(), nros_rmw_cyclonedds_sys::RegisterError> =
    nros_rmw_cyclonedds_sys::register;
