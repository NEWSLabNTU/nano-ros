//! phase-263 B1 (Track D) — listener process of the cross-process E2E-safety demo.
//!
//! `nros::main!(launch = "demo_bringup:safety_listener.launch.xml")` emits
//! `safe_listener_pkg::register(runtime)?;`. The safety subscription validates the
//! CRC the talker process (`native_safety_talker_entry`) attached, reads
//! `CallbackCtx::integrity()`, and republishes the running CRC-validated count on
//! /safe_ok. In-process node-to-node delivery does not happen (issue 0096), hence
//! the two-process split.

nros::main!(launch = "demo_bringup:safety_listener.launch.xml");
