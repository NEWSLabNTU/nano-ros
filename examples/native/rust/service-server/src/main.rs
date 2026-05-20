//! Native Service Server — pure-`cargo` entry. Logic in
//! `lib.rs::run()` (shared with the cyclonedds CMake build path). The
//! cyclonedds variant builds via this dir's `CMakeLists.txt`.
//!
//! ```bash
//! cargo run -p native-rs-service-server   # then native-rs-service-client
//! ```

fn main() {
    env_logger::init();
    native_rs_service_server::run();
}
