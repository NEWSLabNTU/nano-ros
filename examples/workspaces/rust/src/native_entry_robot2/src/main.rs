//! phase-326 (issue 0364) — per-host entry for `robot2`.
//!
//! The partition happens at RESOLVE time: `multihost_robot2_model.yaml`
//! (resolved with `host:=robot2`) contains only the listener, so the macro
//! emits `listener_pkg::register(runtime)?;`. Run alongside
//! `native_entry_robot1` (the talker) as a second process to exercise the
//! multi-host topology — the listener receives the talker's `/chatter`
//! cross-process through `zenohd`. (The retired `host = "…"` key partitioned
//! at bake time from the ROS 1-ism `<node machine=…>`.)

nros::main!(model = "demo_bringup:config/multihost_robot2_model.yaml");
