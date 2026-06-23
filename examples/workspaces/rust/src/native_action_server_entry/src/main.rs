//! phase-263 A4 (Track D) — server process of the cross-process Fibonacci action demo.
//!
//! `nros::main!(launch = "demo_bringup:action_server.launch.xml")` emits
//! `action_server_pkg::register(runtime)?;` — the Fibonacci action server on
//! /fibonacci — and runs the native board's executor + spin loop. The client lives
//! in a SEPARATE process (`native_action_client_entry`); in-process node-to-node
//! delivery does not happen (issue 0096).

nros::main!(launch = "demo_bringup:action_server.launch.xml");
