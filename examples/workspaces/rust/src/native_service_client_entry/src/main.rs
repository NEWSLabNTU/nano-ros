//! phase-263 A1 (Track D) — client process of the cross-process AddTwoInts
//! service round-trip.
//!
//! `nros::main!(launch = "demo_bringup:service_client.launch.xml")` emits
//! `service_client_pkg::register(runtime)?;`. `add_client` calls `add_server`
//! (a SEPARATE process, `native_service_server_entry`) on /add_two_ints each
//! tick and republishes the server-computed sum on /sum. In-process
//! server+client do not talk (issue 0096), hence the two-process split.

nros::main!(model = "demo_bringup:config/service_client_model.yaml");
