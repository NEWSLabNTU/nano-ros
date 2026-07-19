//! issue 0096 (regression guard) — the IN-PROCESS AddTwoInts service round-trip.
//!
//! `nros::main!(launch = "demo_bringup:service_inprocess.launch.xml")` emits BOTH
//! `service_server_pkg::register(runtime)?;` (the `AddTwoInts` server on
//! /add_two_ints) AND `service_client_pkg::register(runtime)?;` (`add_client`,
//! which calls the server each tick and republishes the sum on /sum) into ONE
//! process driven by the native board's executor + spin loop. Both nodes share a
//! single zenoh session, so the client's `call_for_name` exercises same-session
//! (loopback) query→queryable delivery — the path issue 0096 is about. The
//! cross-process variant lives in `native_service_{server,client}_entry`.

nros::main!(model = "demo_bringup:config/service_inprocess_model.yaml");
