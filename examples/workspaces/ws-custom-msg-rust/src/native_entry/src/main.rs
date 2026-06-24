//! Entry pkg — boots the custom-message showcase on the native board.
//!
//! `nros::main!()` emits one `<node_pkg>::register` per `<node>` in the launch
//! file (reading_talker + reading_listener), so the bin links both Node pkgs and
//! the generated `custom_msgs` crate they share. Run `nros ws sync` once first to
//! codegen `generated/custom_msgs` from `src/custom_msgs/msg/Reading.msg`.

nros::main!(launch = "demo_bringup");
