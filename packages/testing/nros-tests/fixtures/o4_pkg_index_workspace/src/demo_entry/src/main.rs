//! Phase 212.O.4 fixture — Entry pkg `main.rs`.
//!
//! `nros::main!(model = "demo_bringup")` exercises
//! the §212.N.10 workspace pkg-index resolution: the macro walks every
//! `package.xml` under the workspace root, finds `<name>demo_bringup</name>`
//! in the sibling directory, then loads
//! `demo_bringup/launch/system.launch.xml` from the resolved dir.
//!
//! Critically, `demo_bringup` has NO `Cargo.toml` — proving the
//! resolver consults `package.xml` (not cargo-metadata).

nros::main!(model = "demo_bringup");
