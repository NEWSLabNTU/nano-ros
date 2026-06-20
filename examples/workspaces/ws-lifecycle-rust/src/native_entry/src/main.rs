//! Entry pkg — boots the managed (lifecycle) system on the native board.
//!
//! `system.toml` declares `[lifecycle] autostart = "active"`, so the
//! `nros::main!()` macro (phase-264 W2) emits `runtime.apply_lifecycle(...)` after
//! the node `register` calls — registering the 5 REP-2002 lifecycle services and
//! driving Configure → Activate at boot. `lifecycle-services` is enabled on the
//! `nros` dep in Cargo.toml.

nros::main!(launch = "demo_bringup:system.launch.xml");
