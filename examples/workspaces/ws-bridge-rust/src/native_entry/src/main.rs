//! Entry pkg — boots the cross-RMW bridge on the native host (phase-267 B3/W1c).
//!
//! Plain `nros::main!` — no build.rs, no bridge code here. Because
//! `demo_bringup/system.toml` declares a `[[bridge]]` AND `nros sync` generated
//! `demo_bringup/nros-bridge.toml`, the macro emits a bridge entry: it
//! `include_str!`s the generated config and runs the data-driven
//! `nros_bridge::run_from_config_str` (open_multi over the zenoh + cyclonedds
//! sessions, a `PubSubBridge` per `[[bridge]]`, spin+pump forever).
//!
//! The two RMW backends are registered for us: `nros::main!` reads the bridge's
//! RMWs from `system.toml` and emits `nros_rmw_<x>::register()` in the generated
//! `main`, so the linker can't dead-strip their `.init_array` self-register ctors
//! (issue 0106 — previously needed a hand `extern crate … as _` force-link here).
//!
//! KNOWN RUNTIME GAP (issue 0107): the Cyclone egress publisher creation still
//! fails (`PublisherCreationFailed`) because the baked `std_msgs/Int32` topic
//! descriptor does not auto-stage in a consumer binary; `run_from_config` has no
//! schema to stage it. The bridge BUILDS + opens both sessions; full forwarding
//! is blocked on 0107.

nros::main!(launch = "demo_bringup");
