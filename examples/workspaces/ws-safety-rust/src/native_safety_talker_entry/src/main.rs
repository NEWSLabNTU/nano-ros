//! phase-263 B1 (Track D) — talker process of the cross-process E2E-safety demo.
//!
//! `nros::main!(launch = "demo_bringup:safety_talker.launch.xml")` emits
//! `talker_pkg::register(runtime)?;`. The board bakes `safety-e2e`, so each
//! /chatter publish carries a backend-attached CRC. The `safe_listener` lives in a
//! SEPARATE process (`native_safety_listener_entry`) — in-process node-to-node
//! delivery does not happen (same zenoh session; issue 0096).

nros::main!(model = "demo_bringup:config/safety_talker_model.yaml");
