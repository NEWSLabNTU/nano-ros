//! phase-263 A4 (Track D) — client process of the cross-process Fibonacci action demo.
//!
//! `nros::main!(launch = "demo_bringup:action_client.launch.xml")` emits
//! `action_client_pkg::register(runtime)?;`. The client sends one goal to the
//! /fibonacci server (a SEPARATE process, `native_action_server_entry`) and republishes
//! the result's last sequence element on /fib_result. In-process node-to-node delivery
//! does not happen (issue 0096), hence the two-process split.

nros::main!(launch = "demo_bringup:action_client.launch.xml");
