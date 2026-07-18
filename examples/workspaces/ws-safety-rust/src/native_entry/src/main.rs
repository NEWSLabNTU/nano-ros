//! Entry pkg — boots the E2E-safety showcase (`talker` + `safe_listener`) on the
//! native board, with the zenoh backend's CRC attach/validate enabled (the
//! `safety-e2e` features wired in Cargo.toml; the system declares
//! `[system].features = ["safety"]`).

nros::main!(model = "demo_bringup");
