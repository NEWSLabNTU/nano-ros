//! Entry pkg — boots the QoS-override showcase on the native board.
//!
//! `nros::main!()` emits one `<node_pkg>::register` per `<node>` in the launch
//! file (reliable_talker + qos_listener), so the bin links both Node pkgs. The
//! QoS profile is declared per-entity inside each pkg's `register()` via the
//! `*_with_qos` declarative API — there is no system.toml QoS section; QoS is a
//! code-level contract that both endpoints must agree on to connect.
//!
//! Note (issue 0096): same-process publisher→subscriber delivery does not occur,
//! so the in-process `qos_listener` observes the talker only via an external
//! broker round-trip; a cross-process subscriber on `/qos_ok` / `/qos_chatter`
//! sees the QoS-matched stream (the Track-D runtime assertion).

nros::main!(launch = "demo_bringup");
