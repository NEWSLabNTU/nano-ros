//! Entry pkg — boots the `demo_bringup` feature-showcase topology on the native
//! board.
//!
//! The `launch = "demo_bringup:showcase.launch.xml"` form makes the
//! `nros::main!()` macro emit a `register` call per `<node>` in the showcase
//! launch — `talker_pkg`, `listener_pkg`, `service_server_pkg`,
//! `service_client_pkg` — then drive the board's executor + spin loop. The
//! minimal `native_entry` (system.launch.xml) stays the quickstart.

nros::main!(launch = "demo_bringup:showcase.launch.xml");
