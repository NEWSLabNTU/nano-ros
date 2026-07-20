//! Phase 211.F — per-host entry for `robot2`.
//!
//! `nros::main!(model = "demo_bringup:config/multihost_model.yaml", host = "robot2")`
//! resolves the multi-host launch and applies the macro's `host` filter: only
//! `<node machine="robot2">` (the listener) survives, so the macro emits
//! `listener_pkg::register(runtime)?;`. Run alongside `native_entry_robot1`
//! (the talker) as a second process to exercise the multi-host topology — the
//! listener receives the talker's `/chatter` cross-process through `zenohd`.

nros::main!(
    model = "demo_bringup:config/multihost_model.yaml",
    host = "robot2"
);
