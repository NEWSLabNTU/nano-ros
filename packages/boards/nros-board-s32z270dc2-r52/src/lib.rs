//! # nros-board-s32z270dc2-r52
//!
//! Board crate for the NXP X-S32Z270-DC (DC2) evaluation board, RTU0
//! Cortex-R52 cores running Zephyr. Phase-117 reference deployment for
//! the Cyclone DDS RMW backend on real Autoware safety-island silicon.
//!
//! Zephyr handles boot, MPU, ENETC + Mailbox driver wiring, and the
//! network stack via its Kconfig + DTS pipeline. This crate ships the
//! `Config` defaults plus the matching `prj.conf` / board overlay
//! under `boards/`. Apps consume the crate from a Zephyr `west build
//! -b s32z2xxdc2/s32z270/rtu0/D` invocation.
//!
//! # Transports
//!
//! - `ethernet` (default) — Zephyr's `net_eth_nxp_s32` driver attaches
//!   to ENETC PSI0; the eval board exposes the port on the J22
//!   gigabit RJ45.
//!
//! # Status
//!
//! Phase 117.11 — config + skeleton landed. Build smoke under Phase
//! 117.13 covers compilation; runtime needs the X-S32Z270-DC eval
//! board + NXP S32 Design Studio JTAG probe (license / hardware
//! gated).

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
