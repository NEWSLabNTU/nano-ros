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
