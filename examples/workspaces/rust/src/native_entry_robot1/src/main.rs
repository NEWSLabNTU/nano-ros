//! phase-326 (issue 0364) — per-host entry for `robot1`.
//!
//! The partition happens at RESOLVE time: `multihost_robot1_model.yaml`
//! (resolved with `host:=robot1`) contains only the talker, so the macro
//! emits `talker_pkg::register(runtime)?;` and the native board runs it. The
//! sibling `native_entry_robot2` bakes the robot2 model (the listener);
//! booting both as two processes is the multi-host runtime topology. (The
//! retired `host = "…"` key partitioned at bake time from the ROS 1-ism
//! `<node machine=…>`.)

nros::main!(
    launch = "demo_bringup:multihost.launch.xml",
    args = [("host", "robot1")]
);
