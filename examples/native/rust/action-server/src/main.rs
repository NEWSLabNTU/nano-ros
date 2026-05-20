//! Native Action Server — pure-`cargo` entry. Logic in
//! `lib.rs::run()` (shared with the cyclonedds CMake build path). The
//! cyclonedds variant builds via this dir's `CMakeLists.txt`.
//!
//! ```bash
//! cargo run -p native-rs-action-server   # then native-rs-action-client
//! ```

fn main() {
    env_logger::init();
    native_rs_action_server::run();
}
