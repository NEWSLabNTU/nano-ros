//! Entry pkg — boots the remap/private-name system on the native board.
//!
//! `nros::main!(model = …)` reads the committed resolved model at expansion
//! time and **compile-bakes** each node's `<remap from= to=/>` rules into
//! `runtime.remaps` before that node's `register` call (phase-306 W3, issue
//! 0255), plus the node identity (`remap_talker` @ `/island`) from the model
//! FQN. Entity creation then expands the node's PRIVATE `~/out` against that
//! identity and resolves it through the rules — the wire topic is
//! `/remapped_out`, with no per-app glue.

nros::main!(model = "demo_bringup:config/rust_remap_model.yaml");
