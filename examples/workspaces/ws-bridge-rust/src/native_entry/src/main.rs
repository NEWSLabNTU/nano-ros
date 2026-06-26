//! Entry pkg — boots the cross-RMW bridge on the native host (phase-267 B3/W1c).
//!
//! Plain `nros::main!` — no build.rs, no bridge code here. Because
//! `demo_bringup/system.toml` declares a `[[bridge]]` AND `nros sync` generated
//! `demo_bringup/nros-bridge.toml`, the macro emits a bridge entry: it
//! `include_str!`s the generated config and runs the data-driven
//! `nros_bridge::run_from_config_str` (open_multi over the zenoh + cyclonedds
//! sessions, a `PubSubBridge` per `[[bridge]]`, spin+pump forever). The two RMW
//! backends are linked via this pkg's deps and self-register on the native host.

nros::main!(launch = "demo_bringup");
