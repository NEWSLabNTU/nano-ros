//! Rust mixed-workspace consumer — Entry pkg.
//!
//! `nros::main!()` (Form-1 self-bringup) reads
//! `[package.metadata.nros.entry] deploy = "native"` from this pkg's
//! `Cargo.toml`, maps the deploy key to `nros_board_linux::LinuxBoard`,
//! and emits the host boot scaffold: it brings up the board, opens the
//! executor, registers this pkg's `Consumer` node (its sibling `lib.rs`
//! `nros::node!` export) and spins. The application logic — importing
//! msgs from both the workspace and AMENT — lives in `src/lib.rs`.
//!
//! Build:
//!
//!   $ cd <fixture>
//!   $ NROS_REPO_DIR=<nano-ros-root> nros sync
//!   $ cd src/rust_consumer && cargo build      # plain cargo, no wrapper
//!
//! Run (zenoh router must be up):
//!
//!   $ ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' /opt/ros/$ROS_DISTRO/lib/rmw_zenoh_cpp/rmw_zenohd &
//!   $ ./target/debug/rust_consumer

nros::main!();
