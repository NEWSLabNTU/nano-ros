//! Rust mixed-workspace consumer — Entry pkg.
//!
//! `nros::main!()` (Form-1 self-bringup) reads
//! `[package.metadata.nros.entry] deploy = "native"` from this pkg's
//! `Cargo.toml`, maps the deploy key to `nros_board_native::NativeBoard`,
//! and emits the host boot scaffold: it brings up the board, opens the
//! executor, registers this pkg's `Consumer` node (its sibling `lib.rs`
//! `nros::node!` export) and spins. The application logic — importing
//! msgs from both the workspace and AMENT — lives in `src/lib.rs`.
//!
//! Build:
//!
//!   $ cd <fixture>
//!   $ NROS_REPO_DIR=<nano-ros-root> nros ws sync
//!   $ cd src/rust_consumer && cargo build      # plain cargo, no wrapper
//!
//! Run (zenoh router must be up):
//!
//!   $ zenohd --listen tcp/127.0.0.1:7447 &
//!   $ ./target/debug/rust_consumer

nros::main!();
