//! Entry pkg — boots the cross-RMW bridge on the native host (phase-267 B3/W1c).
//!
//! Plain `nros::main!(model = "demo_bringup")` — no build.rs, no bridge code
//! here. The resolved `demo_bringup/config/system_model.yaml` carries
//! `execution.bridges` (from the bringup `[[bridge]]`) AND `nros sync` generated
//! `demo_bringup/nros-bridge.toml`, so the macro emits a bridge entry: it
//! `include_str!`s the generated config and runs the data-driven
//! `nros_bridge::run_from_config_str` (open_multi over the zenoh + cyclonedds
//! sessions, a `PubSubBridge` per bridge, spin+pump forever).
//!
//! The two RMW backends are registered for us: the macro reads the bridge's
//! RMWs from the model's `execution.bridges` and emits `nros_rmw_<x>::register()`
//! in the generated `main`, so the linker can't dead-strip their `.init_array`
//! self-register ctors (issue 0106 — previously needed a hand
//! `extern crate … as _` force-link here).
//!
//! Forwarding is GREEN (phase-267 W-B): `nros sync` carries the forwarded type's
//! flat field schema in `nros-bridge.toml`, and `run_from_config` stages the
//! Cyclone descriptor at runtime via `register_type_descriptor` (issue 0107) and
//! pins each session's `domain_id` (issue 0109). Verified end-to-end: a stock
//! `rmw_cyclonedds_cpp` subscriber receives `std_msgs/Int32` forwarded
//! zenoh→cyclonedds.

nros::main!(model = "demo_bringup");
