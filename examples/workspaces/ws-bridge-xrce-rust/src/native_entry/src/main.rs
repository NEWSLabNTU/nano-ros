//! Entry pkg — boots the cross-RMW bridge on the native host (phase-267 B3/W1c).
//!
//! Plain `nros::main!` — no build.rs, no bridge code here. Because
//! `demo_bringup/system.toml` declares a `[[bridge]]` AND `nros sync` generated
//! `demo_bringup/nros-bridge.toml`, the macro emits a bridge entry: it
//! `include_str!`s the generated config and runs the data-driven
//! `nros_bridge::run_from_config_str` (open_multi over the zenoh + xrce sessions,
//! a `PubSubBridge` per `[[bridge]]`, spin+pump forever).
//!
//! The two RMW backends are registered for us: `nros::main!` reads the bridge's
//! RMWs from `system.toml` and emits `nros_rmw_<x>::register()` in the generated
//! `main`, so the linker can't dead-strip their `.init_array` self-register ctors
//! (issue 0106).
//!
//! XRCE variant: the egress dials a Micro-XRCE-DDS Agent at the `[[domain]]
//! agent` locator (`udp/127.0.0.1:8888`, override via `NROS_BRIDGE_S1_LOCATOR`).
//! xrce uses LAZY type registration, so — unlike the cyclonedds sibling
//! (`ws-bridge-rust`) — there is NO descriptor staging / field schema / typed
//! `register::<M>()`; the agent republishes onto DDS for a stock ros2/DDS peer.

nros::main!(launch = "demo_bringup");
