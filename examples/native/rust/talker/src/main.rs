//! Native Talker Example — pure-`cargo` entry point.
//!
//! Demonstrates publishing `std_msgs/Int32` using nros on native x86.
//! The talker logic lives in `lib.rs::run()` (shared with the cyclonedds
//! CMake build path's `rust_main`); this binary is the `cargo build` /
//! `cargo run` entry for the `rmw-zenoh` (default) and `rmw-xrce`
//! features.
//!
//! # Usage
//!
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//! # Then run the talker:
//! cargo run -p native-rs-talker
//! ```
//!
//! Override the locator at runtime with `NROS_LOCATOR` (or the legacy
//! `ZENOH_LOCATOR`). Enable debug logs with `RUST_LOG=debug`. The
//! cyclonedds variant is built via this dir's `CMakeLists.txt`
//! (`cargo build --features rmw-cyclonedds` cannot link Cyclone — see
//! Phase 170.A).

fn main() {
    env_logger::init();
    native_rs_talker::run();
}
