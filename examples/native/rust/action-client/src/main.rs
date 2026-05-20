//! Native Action Client — pure-`cargo` entry. Logic in
//! `lib.rs::run()` (shared with the cyclonedds CMake build path). The
//! cyclonedds variant builds via this dir's `CMakeLists.txt`.
//!
//! ```bash
//! cargo run -p native-rs-action-server   # then this client
//! cargo run -p native-rs-action-client
//! ```

fn main() {
    env_logger::init();
    std::process::exit(native_rs_action_client::run());
}
