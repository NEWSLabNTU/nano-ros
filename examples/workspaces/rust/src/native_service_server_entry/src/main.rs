//! phase-263 A1 (Track D) — server process of the cross-process AddTwoInts
//! service round-trip.
//!
//! `nros::main!(launch = "demo_bringup:service_server.launch.xml")` emits
//! `service_server_pkg::register(runtime)?;` — the `AddTwoInts` server on
//! /add_two_ints — and runs the native board's executor + spin loop. The client
//! lives in a SEPARATE process (`native_service_client_entry`); in-process
//! server+client do not talk (issue 0096).

nros::main!(model = "demo_bringup:config/service_server_model.yaml");
