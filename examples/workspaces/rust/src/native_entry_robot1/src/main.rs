//! Phase 211.F — per-host entry for `robot1`.
//!
//! `nros::main!(model = "demo_bringup:config/multihost_model.yaml", host = "robot1")`
//! resolves the multi-host launch and applies the macro's `host` filter: only
//! `<node machine="robot1">` (the talker) survives, so the macro emits
//! `talker_pkg::register(runtime)?;` and the native board runs it. The sibling
//! `native_entry_robot2` bakes `robot2` (the listener); booting both as two
//! processes is the multi-host runtime topology.

nros::main!(
    model = "demo_bringup:config/multihost_model.yaml",
    host = "robot1"
);
